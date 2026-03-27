#!/usr/bin/env python3
"""
Stereo channel-separated transcription pipeline.

Input: stereo WAV (ch0 = system/remote audio, ch1 = microphone/you)
Pipeline:
  1. Split stereo into two mono channels
  2. Mic channel (ch1): find speech segments via energy-based VAD, transcribe each → label "Me"
  3. System channel (ch0): run pyannote diarization to separate remote speakers,
     transcribe each segment → label "Remote" or "Remote (SPEAKER_XX)" if multiple
  4. Merge all segments by timestamp, output JSON
"""
import argparse, json, sys, os, time, tempfile, threading
from concurrent.futures import ThreadPoolExecutor, as_completed
import soundfile as sf
import numpy as np
import requests


def progress(current, total, message):
    """Report progress on stderr for Swift app to parse"""
    print(f"PROGRESS:{current}/{total}:{message}", file=sys.stderr, flush=True)


def transcribe_segment(audio_data, sample_rate, cohere_key, language, max_retries=3):
    """Send audio segment to Cohere API, return transcribed text"""
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
        sf.write(f.name, audio_data, sample_rate)
        temp_path = f.name

    try:
        for attempt in range(max_retries):
            try:
                with open(temp_path, "rb") as audio_file:
                    response = requests.post(
                        "https://api.cohere.com/v2/audio/transcriptions",
                        headers={"Authorization": f"Bearer {cohere_key}"},
                        files={"file": ("segment.wav", audio_file, "audio/wav")},
                        data={"model": "cohere-transcribe-03-2026", "language": language},
                        timeout=60
                    )

                if response.status_code == 200:
                    return response.json().get("text", "").strip()
                elif response.status_code in (429, 500, 502, 503, 504):
                    delay = (2 ** attempt)
                    print(f"PROGRESS:0/0:Retrying after {response.status_code}... (attempt {attempt+1})", file=sys.stderr, flush=True)
                    time.sleep(delay)
                else:
                    return f"[transcription failed: HTTP {response.status_code}]"
            except requests.exceptions.Timeout:
                if attempt < max_retries - 1:
                    time.sleep(2 ** attempt)
                    continue
                return "[transcription failed: timeout]"
            except requests.exceptions.RequestException as e:
                return f"[transcription failed: {str(e)}]"

        return "[transcription failed: max retries exceeded]"
    finally:
        os.unlink(temp_path)


def format_time(seconds):
    """Format seconds as MM:SS or H:MM:SS"""
    h = int(seconds) // 3600
    m = (int(seconds) % 3600) // 60
    s = int(seconds) % 60
    if h > 0:
        return f"{h}:{m:02d}:{s:02d}"
    return f"{m:02d}:{s:02d}"


def find_speech_segments(audio_mono, sample_rate, min_duration=0.3, energy_threshold=0.005, merge_gap=1.0):
    """
    Energy-based VAD: find contiguous regions above an energy threshold.
    Returns list of {"start": float, "end": float} in seconds.
    """
    # Compute energy in 30ms frames
    frame_len = int(sample_rate * 0.03)
    hop = frame_len
    n_frames = len(audio_mono) // hop

    segments = []
    in_speech = False
    start = 0.0

    for i in range(n_frames):
        frame = audio_mono[i * hop : i * hop + frame_len]
        energy = np.sqrt(np.mean(frame ** 2))
        t = i * hop / sample_rate

        if energy > energy_threshold and not in_speech:
            in_speech = True
            start = t
        elif energy <= energy_threshold and in_speech:
            in_speech = False
            if t - start >= min_duration:
                segments.append({"start": start, "end": t})

    # Close final segment
    if in_speech:
        end = len(audio_mono) / sample_rate
        if end - start >= min_duration:
            segments.append({"start": start, "end": end})

    # Merge segments with small gaps
    merged = []
    for seg in segments:
        if merged and seg["start"] - merged[-1]["end"] < merge_gap:
            merged[-1]["end"] = seg["end"]
        else:
            merged.append(dict(seg))

    return merged


def diarize_system_channel(audio_mono, sample_rate, audio_path_for_pyannote, hf_token, min_speakers=1, max_speakers=10):
    """
    Run pyannote diarization on the system audio channel.
    Writes a temp mono WAV for pyannote, returns list of segments with speaker labels.
    """
    from pyannote.audio import Pipeline

    # Write mono system audio to temp file for pyannote
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
        sf.write(f.name, audio_mono, sample_rate)
        temp_path = f.name

    try:
        progress(0, 0, "Loading diarization model...")
        pipeline = Pipeline.from_pretrained(
            "pyannote/speaker-diarization-3.1",
            token=hf_token
        )

        progress(0, 0, "Diarizing remote speakers...")
        diarization = pipeline(
            temp_path,
            min_speakers=min_speakers,
            max_speakers=max_speakers
        )

        annotation = getattr(diarization, 'speaker_diarization', diarization)
        raw_segments = []
        for segment, _, speaker in annotation.itertracks(yield_label=True):
            if segment.end - segment.start < 0.3:
                continue
            raw_segments.append({
                "start": segment.start,
                "end": segment.end,
                "speaker": speaker
            })

        # Merge adjacent segments from same speaker
        merged = []
        for seg in raw_segments:
            if merged and merged[-1]["speaker"] == seg["speaker"] and seg["start"] - merged[-1]["end"] < 1.5:
                merged[-1]["end"] = seg["end"]
            else:
                merged.append(dict(seg))

        return merged
    finally:
        os.unlink(temp_path)


def main():
    parser = argparse.ArgumentParser(description="Stereo channel-separated transcription")
    parser.add_argument("--audio", required=True, help="Path to stereo WAV file (ch0=system, ch1=mic)")
    parser.add_argument("--hf-token", required=True, help="HuggingFace token for pyannote")
    parser.add_argument("--cohere-key", required=True, help="Cohere API key")
    parser.add_argument("--language", default="en", help="Language code (ISO 639-1)")
    parser.add_argument("--min-speakers", type=int, default=1)
    parser.add_argument("--max-speakers", type=int, default=10)
    args = parser.parse_args()

    if not os.path.exists(args.audio):
        json.dump({"error": f"Audio file not found: {args.audio}"}, sys.stdout)
        sys.exit(1)

    # Load stereo audio
    progress(0, 0, "Loading audio...")
    audio_data, sample_rate = sf.read(args.audio)

    if len(audio_data.shape) == 1:
        # Mono file — treat as system-only (legacy/fallback)
        system_audio = audio_data
        mic_audio = np.zeros_like(audio_data)
        progress(0, 0, "Mono input detected — treating as system audio only")
    else:
        system_audio = audio_data[:, 0]  # ch0 = remote
        mic_audio = audio_data[:, 1]     # ch1 = you

    duration = len(system_audio) / sample_rate
    progress(0, 0, f"Audio loaded: {format_time(duration)} duration, {audio_data.shape[-1] if len(audio_data.shape) > 1 else 1} channels")

    # Check which channels have audio
    sys_has_audio = np.sqrt(np.mean(system_audio ** 2)) > 0.001
    mic_has_audio = np.sqrt(np.mean(mic_audio ** 2)) > 0.001
    progress(0, 0, f"System audio: {'yes' if sys_has_audio else 'silent'}, Mic audio: {'yes' if mic_has_audio else 'silent'}")

    all_segments = []  # Will collect {"start", "end", "speaker", "audio_data"}
    speakers = set()

    # --- Mic channel: energy-based VAD + direct transcription ---
    if mic_has_audio:
        progress(0, 0, "Finding speech in mic channel...")
        mic_segments = find_speech_segments(mic_audio, sample_rate)
        progress(0, 0, f"Found {len(mic_segments)} mic speech segments")

        for seg in mic_segments:
            start_sample = int(seg["start"] * sample_rate)
            end_sample = int(seg["end"] * sample_rate)
            all_segments.append({
                "start": seg["start"],
                "end": seg["end"],
                "speaker": "Me",
                "audio_data": mic_audio[start_sample:end_sample]
            })
        if mic_segments:
            speakers.add("Me")

    # --- System channel: pyannote diarization ---
    if sys_has_audio:
        progress(0, 0, "Processing system audio (remote speakers)...")
        sys_segments = diarize_system_channel(
            system_audio, sample_rate, args.audio,
            args.hf_token, args.min_speakers, args.max_speakers
        )

        # Determine speaker labels
        unique_remote = sorted(set(s["speaker"] for s in sys_segments))
        if len(unique_remote) == 1:
            speaker_map = {unique_remote[0]: "Remote"}
        else:
            speaker_map = {spk: f"Remote ({spk})" for spk in unique_remote}

        progress(0, 0, f"Found {len(unique_remote)} remote speaker(s), {len(sys_segments)} segments")

        for seg in sys_segments:
            start_sample = int(seg["start"] * sample_rate)
            end_sample = int(seg["end"] * sample_rate)
            label = speaker_map[seg["speaker"]]
            all_segments.append({
                "start": seg["start"],
                "end": seg["end"],
                "speaker": label,
                "audio_data": system_audio[start_sample:end_sample]
            })
            speakers.add(label)

    # Sort all segments by start time
    all_segments.sort(key=lambda s: s["start"])

    total = len(all_segments)
    if total == 0:
        progress(0, 0, "No speech detected in either channel")
        json.dump({"segments": [], "speakers": []}, sys.stdout)
        return

    progress(0, total, f"Transcribing {total} segments across {len(speakers)} speaker(s)...")

    # Transcribe all segments in parallel
    results = [None] * total
    completed_count = [0]
    lock = threading.Lock()

    def transcribe_worker(index, seg):
        text = transcribe_segment(seg["audio_data"], sample_rate, args.cohere_key, args.language)
        with lock:
            completed_count[0] += 1
            progress(completed_count[0], total, f"Transcribed {completed_count[0]}/{total} segments")
        return index, seg, text

    with ThreadPoolExecutor(max_workers=8) as executor:
        futures = [
            executor.submit(transcribe_worker, i, seg)
            for i, seg in enumerate(all_segments)
        ]
        for future in as_completed(futures):
            idx, seg, text = future.result()
            if text and not text.startswith("[transcription failed"):
                results[idx] = {
                    "start": round(seg["start"], 2),
                    "end": round(seg["end"], 2),
                    "speaker": seg["speaker"],
                    "text": text
                }

    results = [r for r in results if r is not None]
    progress(total, total, "Done")

    output = {
        "segments": results,
        "speakers": sorted(speakers)
    }
    json.dump(output, sys.stdout, ensure_ascii=False, indent=2)


if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        json.dump({"error": str(e)}, sys.stdout)
        sys.exit(1)
