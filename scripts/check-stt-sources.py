"""Checks which speech-to-text and voice-activity models are actually reachable.

Phase 3 needs two more models, and both have to be fetched from somewhere real.
Guessing a URL and discovering it 404s at install time is the kind of failure
that only shows up on a user's machine, so every candidate is checked here
first, with its size, and the answer is recorded in `manifest.rs`.

Two engines are possible for speech-to-text:

* **whisper.cpp** via `whisper-rs`. The fast, well-trodden route — but
  `whisper-rs-sys` compiles C++ from source, so it needs a C toolchain. This
  machine has cmake but no MSVC, so it will fail exactly where `espeak-rs` did.
* **ONNX Whisper** via the `ort` runtime that voice mode already ships. No C
  toolchain, but the mel front-end has to be written in Rust.

This script settles which is viable by checking whether the ONNX exports exist.

Run: python scripts/check-stt-sources.py
"""

import json
import os
import pathlib
import subprocess
import urllib.error
import urllib.request

# Candidate models, grouped so a missing group is obvious.
SILERO_VAD = [
    "https://github.com/snakers4/silero-vad/raw/master/src/silero_vad/data/silero_vad.onnx",
    "https://raw.githubusercontent.com/snakers4/silero-vad/master/src/silero_vad/data/silero_vad.onnx",
    "https://huggingface.co/onnx-community/silero-vad/resolve/main/onnx/model.onnx",
]

# The two-model split a decoder-loop needs. `decoder_model.onnx` has no past
# key/value cache, which is the simple case: one call per token, no cache to
# thread through. Slower, but a dictation clip is seconds long.
WHISPER = {
    "whisper-tiny.en": "https://huggingface.co/onnx-community/whisper-tiny.en/resolve/main",
    "whisper-base.en": "https://huggingface.co/onnx-community/whisper-base.en/resolve/main",
}

# Files each Whisper export needs, in the order they matter.
WHISPER_FILES = [
    "onnx/encoder_model.onnx",
    "onnx/decoder_model.onnx",
    "onnx/decoder_model_merged.onnx",
    "config.json",
    "tokenizer.json",
    "preprocessor_config.json",
    "generation_config.json",
]


def head(url, timeout=30):
    """Returns (status, bytes) for a URL, following redirects."""
    request = urllib.request.Request(url, method="HEAD")
    request.add_header("User-Agent", "loom-voice-probe")
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            length = response.headers.get("Content-Length")
            return response.status, int(length) if length else None
    except urllib.error.HTTPError as error:
        return error.code, None
    except Exception as error:  # noqa: BLE001 - any network failure is the answer
        return f"error: {type(error).__name__}", None


def human(size):
    if size is None:
        return "?"
    if size >= 1_000_000_000:
        return f"{size / 1e9:.2f} GB"
    if size >= 1_000_000:
        return f"{size / 1e6:.1f} MB"
    return f"{size / 1e3:.0f} KB"


def check_toolchain():
    """Whether the whisper.cpp route is even open."""
    print("=== toolchain ===")
    for tool in ("cmake", "cl", "clang", "gcc"):
        result = subprocess.run(
            ["where", tool],
            capture_output=True,
            text=True,
            shell=True,
        )
        found = result.stdout.strip().splitlines()
        print(f"  {tool:<6} {'yes' if found else 'NO':<4} {found[0] if found else ''}")
    print()
    print("  whisper-rs-sys compiles whisper.cpp from source, so it needs a C")
    print("  compiler. Without one, the ONNX route below is the only option.")
    print()


def check_vad():
    print("=== Silero VAD ===")
    hits = []
    for url in SILERO_VAD:
        status, size = head(url)
        mark = "ok " if status == 200 else "   "
        print(f"  {mark}{status}  {human(size):>9}  {url}")
        if status == 200:
            hits.append((url, size))
    print()
    return hits


def check_whisper():
    print("=== Whisper ONNX ===")
    best = None
    for name, base in WHISPER.items():
        print(f"  {name}")
        present = {}
        for path in WHISPER_FILES:
            status, size = head(f"{base}/{path}")
            if status == 200:
                present[path] = size
                print(f"    ok  {human(size):>9}  {path}")
            else:
                print(f"    --  {str(status):>9}  {path}")
        # The simple loop needs the encoder, the cacheless decoder, and the
        # config files that describe the mel front-end.
        usable = (
            "onnx/encoder_model.onnx" in present
            and "onnx/decoder_model.onnx" in present
            and "config.json" in present
        )
        total = sum(present.values())
        print(f"    {'USABLE' if usable else 'incomplete'} - {human(total)} for what is present")
        if usable and (best is None or name.endswith("base.en")):
            best = (name, present, total)
        print()
    return best


def main():
    check_toolchain()
    vad = check_vad()
    whisper = check_whisper()

    print("=" * 70)
    print("SUMMARY")
    if vad:
        print(f"  VAD      : {vad[0][0]} ({human(vad[0][1])})")
    else:
        print("  VAD      : nothing reachable")

    if whisper:
        name, files, total = whisper
        print(f"  Whisper  : {name} ({human(total)})")
    else:
        print("  Whisper  : no usable ONNX export found")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
