"""Installs everything voice mode needs on a development machine.

Three prebuilt pieces, none of them built from source and none of them shipped
in the installer:

    ONNX Runtime   the inference engine, loaded at run time by `ort`
    espeak-ng      phonemization, reached by hand-written FFI
    Kokoro         the voice model and its style matrix

Kept as one script because the three have to agree with each other and doing
them separately invites a half-installed state that is hard to diagnose. The
per-component scripts remain for targeted re-fetching.

Run: python scripts/setup-voice.py
     python scripts/setup-voice.py --only kokoro
"""

import argparse
import os
import pathlib
import subprocess
import sys

SCRIPTS = pathlib.Path(__file__).parent
HOME = pathlib.Path(os.environ["USERPROFILE"]) / ".loom"

STEPS = [
    ("onnxruntime", "ONNX Runtime", SCRIPTS / "fetch-onnxruntime.py"),
    ("espeak", "espeak-ng", SCRIPTS / "fetch-espeak.py"),
    ("kokoro", "Kokoro model and voices", None),
]


def banner(text):
    print()
    print("=" * 68)
    print(text)
    print("=" * 68)


def run_script(path):
    """Runs a sibling script in this interpreter and returns its exit code."""
    result = subprocess.run([sys.executable, str(path)], check=False)
    return result.returncode


def fetch_kokoro():
    """Downloads the two Kokoro files through the crate's own downloader.

    Deliberately not reimplemented here: `voice::assets::ensure` already does
    resumable, hash-verified downloads, and having a second implementation that
    might disagree about the expected hash is how a corrupt model gets in.

    The Rust downloader also verifies against the pins in `manifest.rs`, which
    is the whole point.
    """
    print("Downloading through the crate's verified downloader...")
    result = subprocess.run(
        ["cargo", "run", "-p", "loom-core", "--example", "fetch_voice_assets"],
        cwd=SCRIPTS.parent,
        check=False,
    )
    return result.returncode


def report():
    """Prints what is now present, and what is still missing."""
    banner("result")
    checks = [
        ("ONNX Runtime", HOME / "ort", "onnxruntime.dll"),
        ("Kokoro model", HOME / "voice", "kokoro-v1.0.onnx"),
        ("Kokoro voices", HOME / "voice", "voices-v1.0.bin"),
        ("espeak-ng library", HOME / "espeak", "espeak-ng.dll"),
        ("espeak-ng data", HOME / "espeak", "espeak-ng-data"),
    ]

    missing = []
    for label, directory, name in checks:
        target = directory / name
        if target.exists():
            if target.is_dir():
                count = sum(1 for _ in target.rglob("*"))
                print(f"  ok       {label:<20} {target}  ({count} entries)")
            else:
                print(f"  ok       {label:<20} {target}  ({target.stat().st_size:,} bytes)")
        elif any(directory.glob(name)) or (
            directory.is_dir() and any(directory.iterdir())
        ):
            # A nested directory: ONNX Runtime extracts to a versioned folder.
            print(f"  ok       {label:<20} {directory}  (nested)")
        else:
            print(f"  MISSING  {label:<20} expected {target}")
            missing.append(label)

    print()
    if missing:
        print(f"{len(missing)} component(s) missing: {', '.join(missing)}")
        return 1
    print("voice mode is ready. Try:")
    print("  cargo run -p loom-core --example hello_kokoro")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--only",
        choices=[name for name, _, _ in STEPS],
        help="install just one component",
    )
    args = parser.parse_args()

    for name, label, script in STEPS:
        if args.only and args.only != name:
            continue
        banner(f"{label}")
        if script is None:
            code = fetch_kokoro()
        else:
            code = run_script(script)
        if code != 0:
            print(f"\n{label} failed (exit {code}). Stopping.")
            return code

    return report()


if __name__ == "__main__":
    sys.exit(main())
