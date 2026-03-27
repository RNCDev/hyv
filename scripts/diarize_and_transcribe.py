#!/usr/bin/env python3
import argparse, json, sys, os, time, tempfile, threading
from concurrent.futures import ThreadPoolExecutor, as_completed
import soundfile as sf
import numpy as np
import requests
from pyannote.audio import Pipeline

def progress(current, total, message):
    """Report progress on stderr for Swift app to parse"""
    print(f"PROGRESS:{current}/{total}:{message}", file=sys.stderr, flush=True)

def transcribe_segment(audio_data, sample_rate, cohere_key, language, max_retries=3):
    """Send audio segment to Cohere API, return transcribed text"""
    # Write segment to temp WAV file
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

def main():
    parser = argparse.ArgumentParser(description="Diarize and transcribe audio")
    parser.add_argument("--audio", required=True, help="Path to WAV file")
    parser.add_argument("--hf-token", required=True, help="HuggingFace token")
    parser.add_argument("--cohere-key", required=True, help="Cohere API key")
    parser.add_argument("--language", default="en", help="Language code (ISO 639-1)")
    parser.add_argument("--min-speakers", type=int, default=2)
    parser.add_argument("--max-speakers", type=int, default=10)
    args = parser.parse_args()

    # Validate input file
    if not os.path.exists(args.audio):
        json.dump({"error": f"Audio file not found: {args.audio}"}, sys.stdout)
        sys.exit(1)

    # Load audio
    progress(0, 0, "Loading audio...")
    audio_data, sample_rate = sf.read(args.audio)

    # If stereo, convert to mono
    if len(audio_data.shape) > 1:
        audio_data = audio_data.mean(axis=1)

    duration = len(audio_data) / sample_rate
    progress(0, 0, f"Audio loaded: {format_time(duration)} duration")

    # Run diarization
    progress(0, 0, "Loading diarization model...")
    pipeline = Pipeline.from_pretrained(
        "pyannote/speaker-diarization-3.1",
        token=args.hf_token
    )

    progress(0, 0, "Running speaker diarization...")
    diarization = pipeline(
        args.audio,
        min_speakers=args.min_speakers,
        max_speakers=args.max_speakers
    )

    # Collect segments (pyannote 4.x returns DiarizeOutput dataclass)
    annotation = getattr(diarization, 'speaker_diarization', diarization)
    raw_segments = []
    for segment, _, speaker in annotation.itertracks(yield_label=True):
        # Skip very short segments (< 0.3s)
        if segment.end - segment.start < 0.3:
            continue
        raw_segments.append({
            "start": segment.start,
            "end": segment.end,
            "speaker": speaker
        })

    # Merge adjacent segments from the same speaker
    merged_segments = []
    for seg in raw_segments:
        if merged_segments and merged_segments[-1]["speaker"] == seg["speaker"] and seg["start"] - merged_segments[-1]["end"] < 1.5:
            merged_segments[-1]["end"] = seg["end"]
        else:
            merged_segments.append(dict(seg))
    raw_segments = merged_segments

    total = len(raw_segments)
    speakers = sorted(set(s["speaker"] for s in raw_segments))
    progress(0, total, f"Diarization complete. Found {len(speakers)} speakers, {total} segments.")

    # Prepare segment audio data
    prepared = []
    for seg in raw_segments:
        start_sample = int(seg["start"] * sample_rate)
        end_sample = int(seg["end"] * sample_rate)
        prepared.append((seg, audio_data[start_sample:end_sample]))

    # Transcribe segments in parallel
    results = [None] * total
    completed = [0]
    lock = threading.Lock()

    def transcribe_worker(index, seg, segment_audio):
        text = transcribe_segment(segment_audio, sample_rate, args.cohere_key, args.language)
        with lock:
            completed[0] += 1
            progress(completed[0], total, f"Transcribed {completed[0]}/{total} segments")
        return index, seg, text

    with ThreadPoolExecutor(max_workers=8) as executor:
        futures = [
            executor.submit(transcribe_worker, i, seg, audio)
            for i, (seg, audio) in enumerate(prepared)
        ]
        for future in as_completed(futures):
            idx, seg, text = future.result()
            if text and text not in ("[transcription failed", ""):
                results[idx] = {
                    "start": round(seg["start"], 2),
                    "end": round(seg["end"], 2),
                    "speaker": seg["speaker"],
                    "text": text
                }

    # Remove None entries (failed transcriptions)
    results = [r for r in results if r is not None]

    progress(total, total, "Done")

    # Output JSON
    output = {
        "segments": results,
        "speakers": speakers
    }
    json.dump(output, sys.stdout, ensure_ascii=False, indent=2)

if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        json.dump({"error": str(e)}, sys.stdout)
        sys.exit(1)
