#!/usr/bin/env python3
import argparse, json, sys, os, time, tempfile, threading
from collections import Counter
from concurrent.futures import ThreadPoolExecutor, as_completed
import soundfile as sf
import numpy as np
import requests
from pyannote.audio import Pipeline

def progress(current, total, message):
    """Report progress on stderr for Swift app to parse"""
    print(f"PROGRESS:{current}/{total}:{message}", file=sys.stderr, flush=True)

def transcribe_segment_api(audio_data, sample_rate, cohere_key, language, max_retries=3):
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

def load_local_model(models_dir=None, device="mps"):
    """Load Cohere Transcribe model for local inference"""
    import torch
    from transformers import AutoProcessor, AutoModelForSpeechSeq2Seq

    model_path = os.path.join(models_dir, "cohere-transcribe") if models_dir else "CohereLabs/cohere-transcribe-03-2026"
    trust_remote = models_dir is None  # only needed when downloading from HF

    progress(0, 0, "Loading local transcription model...")
    processor = AutoProcessor.from_pretrained(model_path, trust_remote_code=True)
    model = AutoModelForSpeechSeq2Seq.from_pretrained(model_path, trust_remote_code=True).to(device)
    model.eval()
    progress(0, 0, "Local model loaded")
    return model, processor

def transcribe_segment_local(audio_data, sample_rate, model, processor, language="en"):
    """Transcribe audio segment using local Cohere model"""
    try:
        texts = model.transcribe(
            processor=processor,
            audio_arrays=[audio_data.astype(np.float32)],
            sample_rates=[sample_rate],
            language=language
        )
        return texts[0].strip() if texts else ""
    except Exception as e:
        return f"[transcription failed: {str(e)}]"

def is_speech_segment(audio_data, sample_rate, rms_threshold=0.005):
    """Return False if segment is likely silence or noise, not speech."""
    if len(audio_data) == 0:
        return False
    rms = np.sqrt(np.mean(audio_data ** 2))
    if rms < rms_threshold:
        return False
    # Very low energy + high zero-crossing rate = noise, not speech
    if rms < 0.01:
        zero_crossings = np.sum(np.abs(np.diff(np.sign(audio_data))) > 0)
        zcr = zero_crossings / len(audio_data)
        if zcr > 0.3:
            return False
    return True

def is_garbage_transcription(text):
    """Detect ASR hallucination: multilingual word salad or excessive repetition."""
    if not text or len(text.strip()) < 3:
        return True

    # Check Unicode script diversity — real speech uses 1-2 scripts
    categories = Counter()
    for ch in text:
        if ch.isalpha():
            cp = ord(ch)
            if cp < 0x0080: categories['latin'] += 1
            elif cp < 0x0530: categories['european'] += 1
            elif 0x0600 <= cp < 0x0700: categories['arabic'] += 1
            elif 0x3040 <= cp < 0x3100: categories['japanese'] += 1
            elif 0x4E00 <= cp < 0x9FFF: categories['cjk'] += 1
            elif 0xAC00 <= cp < 0xD7AF: categories['korean'] += 1
            elif 0x0370 <= cp < 0x0400: categories['greek'] += 1
            elif 0x0400 <= cp < 0x0530: categories['cyrillic'] += 1
            elif 0x0900 <= cp < 0x0980: categories['devanagari'] += 1
            else: categories['other'] += 1
    if len(categories) >= 4:
        return True

    # Check excessive word repetition
    words = text.lower().split()
    if len(words) >= 4:
        word_counts = Counter(words)
        most_common_count = word_counts.most_common(1)[0][1]
        if most_common_count / len(words) > 0.5:
            return True

    return False

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
    parser.add_argument("--cohere-key", default=None, help="Cohere API key (for API mode)")
    parser.add_argument("--local", action="store_true", help="Use local model instead of API")
    parser.add_argument("--models-dir", default=None, help="Local models directory (from download_models.py)")
    parser.add_argument("--language", default="en", help="Language code (ISO 639-1)")
    parser.add_argument("--min-speakers", type=int, default=2)
    parser.add_argument("--max-speakers", type=int, default=10)
    args = parser.parse_args()

    if not args.local and not args.cohere_key:
        json.dump({"error": "Must provide --cohere-key or use --local"}, sys.stdout)
        sys.exit(1)

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
    if args.models_dir:
        pipeline_path = os.path.join(args.models_dir, "pyannote-diarization", "pipeline")
        pipeline = Pipeline.from_pretrained(pipeline_path)
    else:
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

    progress(0, 0, f"Diarization complete. Found {len(raw_segments)} segments.")

    # Prepare segment audio data, filtering silent segments
    prepared = []
    skipped_silent = 0
    for seg in raw_segments:
        start_sample = int(seg["start"] * sample_rate)
        end_sample = int(seg["end"] * sample_rate)
        segment_audio = audio_data[start_sample:end_sample]
        if is_speech_segment(segment_audio, sample_rate):
            prepared.append((seg, segment_audio))
        else:
            skipped_silent += 1

    if skipped_silent:
        progress(0, 0, f"Skipped {skipped_silent} silent segments")

    total = len(prepared)
    speakers = sorted(set(s[0]["speaker"] for s in prepared)) if prepared else []

    if total == 0:
        progress(0, 0, "No speech detected in audio")
        json.dump({"segments": [], "speakers": [], "empty": True}, sys.stdout)
        sys.exit(0)

    progress(0, total, f"Transcribing {total} speech segments from {len(speakers)} speakers")

    # Transcribe segments
    results = [None] * total

    if args.local:
        # Local inference: sequential (GPU not safely concurrent)
        model, processor = load_local_model(models_dir=args.models_dir)
        for i, (seg, segment_audio) in enumerate(prepared):
            progress(i + 1, total, f"Transcribing {seg['speaker']} [{format_time(seg['start'])}-{format_time(seg['end'])}]")
            text = transcribe_segment_local(segment_audio, sample_rate, model, processor, args.language)
            if text and not text.startswith("[transcription failed") and not is_garbage_transcription(text):
                results[i] = {
                    "start": round(seg["start"], 2),
                    "end": round(seg["end"], 2),
                    "speaker": seg["speaker"],
                    "text": text
                }
    else:
        # API mode: parallel requests
        completed = [0]
        lock = threading.Lock()

        def transcribe_worker(index, seg, segment_audio):
            text = transcribe_segment_api(segment_audio, sample_rate, args.cohere_key, args.language)
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
                if text and not text.startswith("[transcription failed") and not is_garbage_transcription(text):
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
