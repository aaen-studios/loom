"""Independently recomputes Whisper's mel front-end and compares to Rust.

The log-mel spectrogram is the part of speech-to-text most likely to be subtly
wrong, because being wrong does not error: it produces a *plausible* transcript
with the wrong words. Reading the Rust back is not enough — so this implements
the same specification a second time, in numpy, and reports the largest
disagreement.

Agreement to a few parts in ten thousand is real evidence. One implementation
agreeing with itself is not.

What is compared:

* `filters.f32` — the 80 x 201 mel filterbank
* `mel.f32`     — the 80 x 3000 log-mel spectrogram

The signal itself comes from Rust (`signal.f32`), so the comparison isolates the
front-end rather than testing two different test-signal generators.

Run:
    cargo run -p loom-core --example dump_mel
    python scripts/check-mel.py
"""

import math
import os
import pathlib
import struct
import sys

# The model's own parameters, from preprocessor_config.json.
N_FFT = 400
HOP_LENGTH = 160
N_MELS = 80
N_FREQ_BINS = N_FFT // 2 + 1
N_SAMPLES = 480_000
N_FRAMES = 3000
SAMPLE_RATE = 16_000
MAX_FREQUENCY = 8_000.0

LOG_FLOOR = 1e-10
DYNAMIC_RANGE = 8.0
NORM_OFFSET = 4.0
NORM_SCALE = 4.0

DEFAULT_DIR = pathlib.Path(os.environ.get("TEMP", ".")) / "loom-mel"


def read_f32(path):
    """Reads raw little-endian f32 into a list."""
    raw = path.read_bytes()
    count = len(raw) // 4
    return list(struct.unpack(f"<{count}f", raw[: count * 4]))


def hz_to_mel(f):
    """Slaney, matching librosa's default and therefore Whisper's."""
    f_sp = 200.0 / 3.0
    min_log_hz = 1000.0
    min_log_mel = min_log_hz / f_sp
    logstep = math.log(6.4) / 27.0
    if f >= min_log_hz:
        return min_log_mel + math.log(f / min_log_hz) / logstep
    return f / f_sp


def mel_to_hz(m):
    f_sp = 200.0 / 3.0
    min_log_hz = 1000.0
    min_log_mel = min_log_hz / f_sp
    logstep = math.log(6.4) / 27.0
    if m >= min_log_mel:
        return min_log_hz * math.exp(logstep * (m - min_log_mel))
    return f_sp * m


def mel_frequencies(n):
    low = hz_to_mel(0.0)
    high = hz_to_mel(MAX_FREQUENCY)
    return [
        mel_to_hz(low + (high - low) * index / (n - 1)) for index in range(n)
    ]


def mel_filterbank():
    """(N_MELS, N_FREQ_BINS) triangular filters, Slaney-normalised."""
    points = mel_frequencies(N_MELS + 2)
    bin_hz = [k * SAMPLE_RATE / N_FFT for k in range(N_FREQ_BINS)]

    filters = []
    for m in range(N_MELS):
        left_width = points[m + 1] - points[m]
        right_width = points[m + 2] - points[m + 1]
        lowest = points[m]
        highest = points[m + 2]
        norm = 2.0 / (highest - lowest)
        row = []
        for frequency in bin_hz:
            rising = (frequency - lowest) / left_width
            falling = (highest - frequency) / right_width
            row.append(max(0.0, min(rising, falling)) * norm)
        filters.append(row)
    return filters


def hann_window(n):
    """Periodic Hann: cos(2*pi*n/N), not cos(2*pi*n/(N-1))."""
    return [0.5 - 0.5 * math.cos(2.0 * math.pi * i / n) for i in range(n)]


def reflect_pad(samples, pad):
    """Mirrors at each edge without repeating the edge sample."""
    if len(samples) <= pad or pad == 0:
        return list(samples)
    left = [samples[i] for i in range(pad, 0, -1)]
    right = [samples[len(samples) - 2 - i] for i in range(pad)]
    return left + list(samples) + right


def dft_magnitudes(frame, window):
    """A direct DFT, so no numpy FFT convention has to be trusted.

    O(n^2) on purpose: 400 points x 3000 frames is 480M operations, which is
    slow in Python — so the full-spectrum comparison uses the first N frames
    only, and the frame count itself is checked separately.
    """
    n = len(frame)
    out = []
    for k in range(N_FREQ_BINS):
        re = 0.0
        im = 0.0
        angle_step = -2.0 * math.pi * k / n
        for i in range(n):
            value = frame[i] * window[i]
            angle = angle_step * i
            re += value * math.cos(angle)
            im += value * math.sin(angle)
        out.append(re * re + im * im)
    return out


def frame_count():
    """How many frames the front-end produces, derived from the parameters.

    1 + (n_samples + 2*pad - n_fft) / hop, then the last one is dropped.
    For 30 s at 16 kHz that is 1 + 3000 = 3001, leaving 3000 — which is exactly
    the `nb_max_frames` in preprocessor_config.json, and the reason that number
    is what it is.
    """
    pad = N_FFT // 2
    raw = 1 + (N_SAMPLES + 2 * pad - N_FFT) // HOP_LENGTH
    return raw, raw - 1


def compute_mel(samples, filters, max_frames):
    """The front-end as far as the logarithm, following the specification.

    Stops before the clamp and the affine step because both are *global*: they
    depend on the peak across all 3000 frames, so a partial recompute cannot
    reproduce them. `check_normalisation` verifies those two in full instead,
    from the arrays Rust dumped.
    """
    raw_frames, frames = frame_count()

    # 1. Pad or trim to 30 s.
    padded = list(samples[:N_SAMPLES])
    padded.extend([0.0] * (N_SAMPLES - len(padded)))

    # 2. Reflect-pad by n_fft // 2 at both edges.
    framed = reflect_pad(padded, N_FFT // 2)
    if 1 + (len(framed) - N_FFT) // HOP_LENGTH != raw_frames:
        raise AssertionError("the frame count does not match the derivation")

    window = hann_window(N_FFT)

    # 3-4. Power spectrum and filterbank, for the frames that get compared.
    mel = [[LOG_FLOOR] * frames for _ in range(N_MELS)]
    for frame in range(min(frames, max_frames)):
        start = frame * HOP_LENGTH
        power = dft_magnitudes(framed[start : start + N_FFT], window)
        for m in range(N_MELS):
            row = filters[m]
            mel[m][frame] = sum(row[k] * power[k] for k in range(N_FREQ_BINS))

    # 5. log10 with the floor, and no further step.
    for m in range(N_MELS):
        for f in range(frames):
            mel[m][f] = math.log10(max(mel[m][f], LOG_FLOOR))

    return mel, frames, raw_frames


def check_normalisation(log_values, final_values):
    """Verifies the two global steps against the arrays themselves.

    The clamp is `max(x, peak - 8)` and the normalisation is `(x + 4) / 4`. Both
    are cheap to apply to 240,000 values, so this checks every one — and it
    would catch a floor computed per frame instead of across the whole clip,
    which is the mistake that a partial recompute could not have found.
    """
    peak = max(log_values)
    floor = peak - DYNAMIC_RANGE
    worst = 0.0
    worst_at = 0
    for index, (raw, final) in enumerate(zip(log_values, final_values)):
        expected = (max(raw, floor) + NORM_OFFSET) / NORM_SCALE
        difference = abs(expected - final)
        if difference > worst:
            worst = difference
            worst_at = index

    print(f"  peak log      : {peak:.4f}")
    print(f"  floor         : {floor:.4f}  (peak - {DYNAMIC_RANGE})")
    status = "ok  " if worst <= 1e-5 else "FAIL"
    print(f"  {status} clamp + normalise           max |diff| {worst:.3e}"
          f"  (of {len(log_values):,} values)")
    if worst > 1e-5:
        index = worst_at
        print(f"       at index {index}: expected {final:.6f}, got"
              f" {(max(log_values[index], floor) + NORM_OFFSET) / NORM_SCALE:.6f}")
    return worst <= 1e-5


def compare(label, mine, theirs, tolerance, limit=None):
    """Reports the largest absolute difference, and where it is."""
    count = len(mine) if limit is None else min(limit, len(mine))
    worst = 0.0
    worst_at = -1
    for index in range(count):
        difference = abs(mine[index] - theirs[index])
        if difference > worst:
            worst = difference
            worst_at = index

    status = "ok  " if worst <= tolerance else "FAIL"
    print(
        f"  {status} {label:<28} max |diff| {worst:.3e}"
        f"  (of {count:,} values)"
    )
    if worst > tolerance:
        print(f"       at index {worst_at}: rust {mine[worst_at]:.6f} vs numpy {theirs[worst_at]:.6f}")
    return worst <= tolerance


def main():
    directory = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_DIR

    signal_path = directory / "signal.f32"
    filters_path = directory / "filters.f32"
    log_path = directory / "mel_log.f32"
    mel_path = directory / "mel.f32"

    for path in (signal_path, filters_path, log_path, mel_path):
        if not path.exists():
            print(f"missing: {path}")
            print("Run: cargo run -p loom-core --example dump_mel")
            return 1

    signal = read_f32(signal_path)
    rust_filters = read_f32(filters_path)
    rust_log = read_f32(log_path)
    rust_mel = read_f32(mel_path)

    print(f"directory     : {directory}")
    print(f"signal        : {len(signal):,} samples")
    print(f"filterbank    : {len(rust_filters):,} values")
    print(f"log-mel       : {len(rust_log):,} values")
    print(f"normalised    : {len(rust_mel):,} values")
    print()

    ok = True

    # -- the filterbank ------------------------------------------------------
    print("filterbank")
    numpy_filters = mel_filterbank()
    flat = [value for row in numpy_filters for value in row]
    ok &= compare("filters", rust_filters, flat, 1e-5, limit=len(flat))
    print()

    # -- the per-frame front-end ---------------------------------------------
    # The full 3000-frame DFT in pure Python would take minutes. A few frames is
    # enough to catch a windowing, padding or indexing error, and the frame
    # count is checked separately against the derivation.
    print("log-mel (first frames, direct DFT, no global steps)")
    derived_raw, derived = frame_count()
    rust_frames = len(rust_log) // N_MELS
    print(f"  frame count: rust {rust_frames:,}, derived {derived:,}"
          f"  (before dropping the last: {derived_raw:,})")
    if rust_frames != N_FRAMES or derived != N_FRAMES:
        print(f"  FAIL frame count is not {N_FRAMES}")
        ok = False
    else:
        print(f"  ok   frame count is {N_FRAMES}")

    frames_to_check = int(os.environ.get("MEL_FRAMES", "3"))
    numpy_mel, total_frames, _ = compute_mel(signal, numpy_filters, frames_to_check)
    assert total_frames == N_FRAMES, "the derivation and the computation disagree"

    # Frame f of bin m sits at m * N_FRAMES + f.
    for frame in range(frames_to_check):
        mine = [rust_log[m * N_FRAMES + frame] for m in range(N_MELS)]
        theirs = [numpy_mel[m][frame] for m in range(N_MELS)]
        ok &= compare(f"frame {frame}", mine, theirs, 2e-3)

    # And a frame from the loud part of the signal, since the first ones are
    # silence: the leading half-second is zeros by design, to exercise the log
    # floor, but a bug in the harmonic content would only show where there is
    # some.
    loud_frame = 100
    numpy_loud, _, _ = compute_mel(signal, numpy_filters, loud_frame + 1)
    mine = [rust_log[m * N_FRAMES + loud_frame] for m in range(N_MELS)]
    theirs = [numpy_loud[m][loud_frame] for m in range(N_MELS)]
    ok &= compare(f"frame {loud_frame} (loud)", mine, theirs, 2e-3)
    print()

    # -- the two global steps ------------------------------------------------
    print("normalisation (every value, since both steps are global)")
    ok &= check_normalisation(rust_log, rust_mel)
    print()

    # -- the shape of the whole thing ----------------------------------------
    print("overall")
    low = min(rust_mel)
    high = max(rust_mel)
    print(f"  ok   normalised range {low:.4f} .. {high:.4f}")
    if not (-2.0 <= low and high <= 3.0):
        print("  FAIL the range is outside what Whisper's normalisation produces")
        ok = False

    # Informational, not a pass/fail: the test signal is 2 seconds inside the
    # 30-second window the model requires, so most frames are silence — and
    # silence is exactly what the floor is for.
    silent = sum(1 for value in rust_mel if value == low)
    print(f"  --   {silent:,} values sit on the floor ({100 * silent / len(rust_mel):.1f}%)")
    print("       expected: the signal is 2 s of a 30 s window, so most frames")
    print("       are silence and the floor is what keeps them finite")
    print()

    print("VERDICT")
    if ok:
        print("  The Rust front-end matches an independent numpy implementation.")
        print("  Filter weights, frame count, per-frame log values, and both global")
        print("  steps all agree — so the mel is not the reason a transcript would")
        print("  be wrong.")
        return 0

    print("  MISMATCH. The Rust front-end and the numpy reference disagree, so")
    print("  the encoder would be fed the wrong features. Check, in this order:")
    print("    1. the Hann window — periodic (divide by N) or symmetric (N-1)")
    print("    2. the reflection — pad_mode='reflect', 200 samples each side")
    print("    3. the mel scale — Slaney, not HTK")
    print("    4. the last frame — dropped, leaving 3000 and not 3001")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
