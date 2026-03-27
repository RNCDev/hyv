#!/usr/bin/env python3
"""Download all models to a local directory for offline/bundled use.

Usage:
    python3 scripts/download_models.py --hf-token YOUR_TOKEN [--output models/]

After running, the models/ directory can be bundled with the app.
The diarize_and_transcribe.py script accepts --models-dir to use these.
"""
import argparse, os, sys

def main():
    parser = argparse.ArgumentParser(description="Download Hyv models for offline use")
    parser.add_argument("--hf-token", required=True, help="HuggingFace token")
    parser.add_argument("--output", default="models", help="Output directory (default: models/)")
    args = parser.parse_args()

    os.makedirs(args.output, exist_ok=True)

    # 1. Pyannote diarization pipeline + sub-models
    print("Downloading pyannote speaker-diarization-3.1...")
    from pyannote.audio import Pipeline
    pipeline = Pipeline.from_pretrained(
        "pyannote/speaker-diarization-3.1",
        token=args.hf_token
    )

    diarization_dir = os.path.join(args.output, "pyannote-diarization")
    os.makedirs(diarization_dir, exist_ok=True)

    # Save the segmentation model
    print("  Saving segmentation model...")
    seg_dir = os.path.join(diarization_dir, "segmentation")
    os.makedirs(seg_dir, exist_ok=True)
    pipeline._segmentation.model.save_pretrained(seg_dir)

    # Save the embedding model
    print("  Saving embedding model...")
    emb_dir = os.path.join(diarization_dir, "embedding")
    os.makedirs(emb_dir, exist_ok=True)
    # The embedding model is a wespeaker model — save via huggingface_hub
    from huggingface_hub import snapshot_download
    snapshot_download(
        "pyannote/wespeaker-voxceleb-resnet34-LM",
        local_dir=emb_dir,
        token=args.hf_token
    )

    # Save the segmentation model files too
    snapshot_download(
        "pyannote/segmentation-3.0",
        local_dir=seg_dir,
        token=args.hf_token
    )

    # Save pipeline config
    snapshot_download(
        "pyannote/speaker-diarization-3.1",
        local_dir=os.path.join(diarization_dir, "pipeline"),
        token=args.hf_token
    )

    print(f"  Pyannote models saved to {diarization_dir}/")

    # 2. Cohere Transcribe model
    print("Downloading CohereLabs/cohere-transcribe-03-2026...")
    transcribe_dir = os.path.join(args.output, "cohere-transcribe")
    os.makedirs(transcribe_dir, exist_ok=True)

    from transformers import AutoProcessor, AutoModelForSpeechSeq2Seq
    processor = AutoProcessor.from_pretrained(
        "CohereLabs/cohere-transcribe-03-2026", trust_remote_code=True
    )
    model = AutoModelForSpeechSeq2Seq.from_pretrained(
        "CohereLabs/cohere-transcribe-03-2026", trust_remote_code=True
    )
    processor.save_pretrained(transcribe_dir)
    model.save_pretrained(transcribe_dir)

    print(f"  Cohere model saved to {transcribe_dir}/")

    # Summary
    total_size = 0
    for dirpath, _, filenames in os.walk(args.output):
        for f in filenames:
            total_size += os.path.getsize(os.path.join(dirpath, f))
    print(f"\nDone. Total size: {total_size / (1024**3):.1f} GB")
    print(f"Pass --models-dir {args.output} to diarize_and_transcribe.py to use these models offline.")

if __name__ == "__main__":
    main()
