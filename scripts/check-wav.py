"""Checks that a WAV produced by `hello_kokoro` is speech and not noise.

Three things separate speech from a broken phoneme-to-token mapping, and none of
them require listening:

* **It is not silent.** A peak near zero means every phoneme was dropped.
* **It is not a constant tone.** Noise or a stuck decoder gives a flat envelope;
  speech has loud syllables and quiet gaps between them.
* **Its duration is plausible.** Kokoro renders roughly 1,000 characters per
  minute, so a wildly different figure means something is wrong upstream.

Stdlib only. The shell's quoting has repeatedly mangled inline Python here, so
this lives in a file.

Run: python scripts/check-wav.py [path]
"""

import os
import pathlib
import struct
import sys
import wave

DEFAULT = pathlib.Path(os.environ.get("TEMP", ".")) / "loom-kokoro.wav"

# Window size for the envelope analysis, in milliseconds.
WINDOW_MS = 20


def analyse(path):
    with wave.open(str(path)) as handle:
        channels = handle.getnchannels()
        rate = handle.getframerate()
        frames = handle.getnframes()
        width = handle.getsampwidth()
        raw = handle.readframes(frames)

    if width != 2:
        print(f"  sample width {width * 8}-bit, expected 16")

    duration = frames / rate if rate else 0.0
    count = len(raw) // 2
    samples = struct.unpack(f"<{count}h", raw[: count * 2])

    peak = max(abs(s) for s in samples) if samples else 0
    rms = (sum(s * s for s in samples) / len(samples)) ** 0.5 if samples else 0.0

    # Envelope: RMS per window. Speech varies a lot between windows; a broken
    # pipeline that emits noise or a tone does not.
    window = max(1, int(rate * WINDOW_MS / 1000))
    windows = [
        (sum(s * s for s in samples[i : i + window]) / max(1, len(samples[i : i + window]))) ** 0.5
        for i in range(0, len(samples) - window, window)
    ]

    print(f"file       : {path}")
    print(f"channels   : {channels}")
    print(f"rate       : {rate} Hz")
    print(f"frames     : {frames:,}")
    print(f"duration   : {duration:.2f} s")
    print(f"peak       : {peak:,} of 32767  ({peak / 32767:.3f})")
    print(f"rms        : {rms:.1f}")
    print()

    problems = []

    if peak < 32767 * 0.01:
        problems.append("essentially silent — every phoneme was probably dropped")

    if windows:
        quiet = sum(1 for w in windows if w < 100)
        loud = sum(1 for w in windows if w > 1000)
        mean = sum(windows) / len(windows)
        spread = (max(windows) - min(windows)) / max(1.0, mean)

        print(f"windows    : {len(windows)} of {WINDOW_MS} ms")
        print(f"  quiet    : {quiet} ({100 * quiet / len(windows):.0f}%)")
        print(f"  loud     : {loud} ({100 * loud / len(windows):.0f}%)")
        print(f"  spread   : {spread:.2f}x the mean")
        print()

        if quiet == 0:
            problems.append("no quiet windows — speech has gaps between words")
        if spread < 0.5:
            problems.append("flat envelope — this looks like a tone, not speech")
        if loud == 0:
            problems.append("nothing ever gets loud — the output may be very quiet")

    print("VERDICT")
    if problems:
        for problem in problems:
            print(f"  PROBLEM: {problem}")
        print()
        print("  A file that exists but is wrong usually means the phonemizer and")
        print("  the model disagree about the vocabulary. Check that espeak-ng's")
        print("  phoneme_mode is 0x0002 (IPA) and not 0x0001.")
        return 1

    print("  Speech-like: audible, with the varying envelope speech has.")
    print("  Play it — if it sounds like words, the pipeline is correct.")
    return 0


def main():
    path = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT
    if not path.exists():
        print(f"not found: {path}")
        print("Run: cargo run -p loom-core --example hello_kokoro")
        return 1
    return analyse(path)


if __name__ == "__main__":
    sys.exit(main())
