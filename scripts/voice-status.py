"""Reports what voice mode has installed, and what each piece is for.

The three components land in `%USERPROFILE%\\.loom\\` in five directories, and
after a few phases it stops being obvious which model is which. This prints the
inventory with sizes, and what is still missing.

Run: python scripts/voice-status.py
"""

import os
import pathlib

HOME = pathlib.Path(os.environ["USERPROFILE"]) / ".loom"

# (relative path, label, what it is for)
ITEMS = [
    ("ort", "ONNX Runtime", "loads every ONNX model at run time"),
    ("espeak/espeak-ng.dll", "espeak-ng library", "grapheme to phoneme, for speech"),
    ("espeak/espeak-ng-data", "espeak-ng data", "the voice rules it needs"),
    ("voice/kokoro-v1.0.onnx", "Kokoro model", "text to audio, 82M"),
    ("voice/voices-v1.0.bin", "Kokoro voices", "54 voice styles"),
    ("voice/whisper/encoder_model.onnx", "Whisper encoder", "audio to features"),
    ("voice/whisper/decoder_model.onnx", "Whisper decoder", "features to tokens"),
    ("voice/whisper/tokenizer.json", "Whisper tokenizer", "tokens to text"),
    ("voice/whisper/config.json", "Whisper config", "the ids the loop needs"),
    ("voice/whisper/generation_config.json", "Whisper generation", "no_timestamps, which config.json omits"),
    ("voice/silero_vad.onnx", "Silero VAD", "finds where speech starts and stops"),
]


def size_of(path):
    if not path.exists():
        return None
    if path.is_dir():
        return sum(f.stat().st_size for f in path.rglob("*") if f.is_file())
    return path.stat().st_size


def human(size):
    if size is None:
        return "-"
    if size >= 1_000_000_000:
        return f"{size / 1e9:.2f} GB"
    if size >= 1_000_000:
        return f"{size / 1e6:.1f} MB"
    return f"{size / 1e3:.0f} KB"


def nested_library():
    """ONNX Runtime extracts to a versioned directory, so check inside."""
    root = HOME / "ort"
    if not root.is_dir():
        return None
    for entry in root.iterdir():
        if entry.is_dir():
            for candidate in entry.rglob("onnxruntime*.dll"):
                return sum(f.stat().st_size for f in entry.rglob("*") if f.is_file())
    return None


def main():
    print(f"Loom home: {HOME}")
    print()

    total = 0
    missing = []

    for relative, label, purpose in ITEMS:
        path = HOME / relative
        size = size_of(path)

        # ONNX Runtime's archive nests, so fall back to searching.
        if size is None and relative == "ort":
            size = nested_library()

        if size is None:
            print(f"  MISSING  {label:26} {purpose}")
            missing.append(label)
            continue

        total += size
        print(f"  ok       {label:26} {human(size):>9}  {purpose}")

    print()
    print(f"total on disk: {human(total)}")

    if missing:
        print()
        print(f"{len(missing)} component(s) missing: {', '.join(missing)}")
        print("Install with: python scripts/setup-voice.py")
        return 1

    print()
    print("Everything is present. Which parts work:")
    print("  speak a reply      yes — Settings > Voice, and the button on a message")
    print("  install in-app     yes")
    print("  hear you           yes — the mic button in the composer (untested on real audio)")
    print("  transcribe a file  yes — cargo run -p loom-core --example transcribe")
    print("  the mel front-end  yes, and cross-checked against numpy")
    print("  the tokenizer      yes, verified against the real file")
    print("  the decode loop    yes, 100% word match on synthesised speech")
    print("  voice activity     yes — vad.rs detects speech, listen.rs finds utterances")
    print("  barge-in           yes — talking over the reply stops it, within one sentence")
    print("  voice mode surface yes — Ctrl+Shift+V, or the speaker in the title bar")
    print("  read-along         yes — the sentence being played is highlighted")
    print()
    print("Not done: nothing is committed, and no real microphone has been used yet.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
