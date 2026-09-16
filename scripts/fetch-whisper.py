"""Downloads the speech-to-text models: Whisper tiny.en and Silero VAD.

Four files from the `onnx-community/whisper-tiny.en` export and one for the
voice-activity detector. They land beside the Kokoro assets rather than in the
installer, for the same reason: 30 MB of a feature nobody has asked for yet.

`onnx/decoder_model.onnx` is the **cacheless** export, not
`decoder_model_merged`. That means one graph call per generated token with the
full token sequence each time — O(n^2) rather than O(n) — but no key/value cache
to thread through by hand, which is a large amount of code not written. A
dictation clip is a few dozen tokens, so the difference is milliseconds.

Stdlib only, and idempotent: a file already present is left alone.

Run: python scripts/fetch-whisper.py
"""

import os
import pathlib
import sys
import urllib.request

HOME = pathlib.Path(os.environ["USERPROFILE"]) / ".loom" / "voice"

WHISPER_BASE = "https://huggingface.co/onnx-community/whisper-tiny.en/resolve/main"

# (url, destination relative to HOME)
FILES = [
    (f"{WHISPER_BASE}/onnx/encoder_model.onnx", "whisper/encoder_model.onnx"),
    (f"{WHISPER_BASE}/onnx/decoder_model.onnx", "whisper/decoder_model.onnx"),
    (f"{WHISPER_BASE}/tokenizer.json", "whisper/tokenizer.json"),
    (f"{WHISPER_BASE}/config.json", "whisper/config.json"),
    # The decode prompt's ids live here, not in config.json: `no_timestamps`
    # and `decoder_start_token_id` are generation settings. Without this file
    # the decoder loop has to guess that 50362 suppresses timestamps.
    (f"{WHISPER_BASE}/generation_config.json", "whisper/generation_config.json"),
    (
        "https://github.com/snakers4/silero-vad/raw/master/src/silero_vad/data/silero_vad.onnx",
        "silero_vad.onnx",
    ),
]


def human(size):
    if size >= 1_000_000:
        return f"{size / 1e6:.1f} MB"
    return f"{size / 1e3:.0f} KB"


def download(url, destination):
    if destination.exists():
        print(f"  ok       {destination.name:24} already present ({human(destination.stat().st_size)})")
        return 0

    destination.parent.mkdir(parents=True, exist_ok=True)
    scratch = destination.with_suffix(destination.suffix + ".part")

    request = urllib.request.Request(url, headers={"User-Agent": "loom-voice"})
    try:
        with urllib.request.urlopen(request, timeout=300) as response:
            total = int(response.headers.get("Content-Length") or 0)
            written = 0
            with open(scratch, "wb") as out:
                while True:
                    chunk = response.read(1 << 20)
                    if not chunk:
                        break
                    out.write(chunk)
                    written += len(chunk)
    except Exception as error:  # noqa: BLE001 - any failure is the answer
        scratch.unlink(missing_ok=True)
        print(f"  FAILED   {destination.name:24} {type(error).__name__}: {error}")
        return 1

    scratch.replace(destination)
    print(f"  ok       {destination.name:24} {human(written)}")
    return 0


def main():
    print(f"destination: {HOME}")
    print()

    failures = 0
    for url, relative in FILES:
        failures += download(url, HOME / relative)

    print()
    if failures:
        print(f"{failures} file(s) failed to download")
        return 1

    print("speech-to-text models are ready")
    return 0


if __name__ == "__main__":
    sys.exit(main())
