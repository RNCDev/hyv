#!/usr/bin/env python3
"""Download all models to a local directory for offline/bundled use.

Usage:
    python3 scripts/download_models.py --hf-token YOUR_TOKEN [--output models/]

After running, the models/ directory can be bundled with the app.
The diarize_and_transcribe.py script accepts --models-dir to use these.
"""
import argparse, os

def main():
    parser = argparse.ArgumentParser(description="Download Hyv models for offline use")
    parser.add_argument("--hf-token", required=True, help="HuggingFace token")
    parser.add_argument("--output", default="models", help="Output directory (default: models/)")
    args = parser.parse_args()

    os.makedirs(args.output, exist_ok=True)
    from huggingface_hub import snapshot_download

    # 1. Pyannote diarization pipeline + sub-models
    diarization_dir = os.path.join(args.output, "pyannote-diarization")

    models = [
        ("pyannote/speaker-diarization-3.1", "pipeline"),
        ("pyannote/segmentation-3.0", "segmentation"),
        ("pyannote/wespeaker-voxceleb-resnet34-LM", "embedding"),
    ]

    for repo_id, subdir in models:
        local_dir = os.path.join(diarization_dir, subdir)
        print(f"Downloading {repo_id}...")
        snapshot_download(repo_id, local_dir=local_dir, token=args.hf_token)
        print(f"  Saved to {local_dir}/")

    # Patch pipeline config to use local paths
    config_path = os.path.join(diarization_dir, "pipeline", "config.yaml")
    if os.path.exists(config_path):
        with open(config_path, "r") as f:
            config = f.read()
        config = config.replace("pyannote/segmentation-3.0", os.path.abspath(os.path.join(diarization_dir, "segmentation")))
        config = config.replace("pyannote/wespeaker-voxceleb-resnet34-LM", os.path.abspath(os.path.join(diarization_dir, "embedding")))
        with open(config_path, "w") as f:
            f.write(config)
        print("  Patched pipeline config with local paths")

    # 2. Cohere Transcribe model
    transcribe_dir = os.path.join(args.output, "cohere-transcribe")
    print("Downloading CohereLabs/cohere-transcribe-03-2026...")

    from transformers import AutoProcessor, AutoModelForSpeechSeq2Seq
    processor = AutoProcessor.from_pretrained(
        "CohereLabs/cohere-transcribe-03-2026", trust_remote_code=True
    )
    model = AutoModelForSpeechSeq2Seq.from_pretrained(
        "CohereLabs/cohere-transcribe-03-2026", trust_remote_code=True
    )
    processor.save_pretrained(transcribe_dir)
    model.save_pretrained(transcribe_dir)
    # Also copy remote code files needed for loading
    snapshot_download(
        "CohereLabs/cohere-transcribe-03-2026",
        local_dir=transcribe_dir,
        token=args.hf_token
    )
    print(f"  Saved to {transcribe_dir}/")

    # Summary
    total_size = 0
    for dirpath, _, filenames in os.walk(args.output):
        for f in filenames:
            total_size += os.path.getsize(os.path.join(dirpath, f))
    print(f"\nDone. Total size: {total_size / (1024**3):.1f} GB")
    print(f"Pass --models-dir {args.output} to diarize_and_transcribe.py to use these models offline.")

if __name__ == "__main__":
    main()
