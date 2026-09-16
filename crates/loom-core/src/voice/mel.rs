//! Whisper's log-mel spectrogram: audio in, the encoder's input features out.
//!
//! This is the front-end the ONNX encoder expects, and every number in it comes
//! from the model's own `preprocessor_config.json` rather than from memory:
//!
//! | | | |
//! |---|---|---|
//! | `n_fft` | 400 | a 25 ms window at 16 kHz |
//! | `hop_length` | 160 | a 10 ms step |
//! | `feature_size` | 80 | mel bins |
//! | `n_samples` | 480000 | exactly 30 s |
//! | `nb_max_frames` | 3000 | 480000 / 160 |
//! | `sampling_rate` | 16000 | |
//!
//! # The five steps, and the two that are easy to get wrong
//!
//! 1. Pad or trim to 30 s at [`TARGET_RATE`].
//! 2. Reflect-pad by `n_fft / 2` on **both** sides.
//! 3. STFT with a **periodic** Hann window; drop the last frame.
//! 4. Power spectrum, then the mel filterbank.
//! 5. `log10`, clamped at `1e-10`; floor at `peak - 8`; then `(x + 4) / 4`.
//!
//! **Reflect, not zero.** `torch.stft`'s `center=True` pads by mirroring the
//! signal at each edge, and the frame count depends on it: reflecting gives
//! `1 + (480000 + 400 - 400) / 160 = 3001` frames, and dropping the last
//! leaves exactly the 3000 that `nb_max_frames` names. Zero-padding would give
//! a different count *and* a different first frame.
//!
//! **Periodic, not symmetric.** `torch.hann_window(N)` defaults to periodic:
//! `0.5 - 0.5 * cos(2πn/N)`, so `w[0] = 0` and `w[N-1] = 0.00006`, not the
//! `w[N-1] = 0` a symmetric window would give.
//!
//! Both are the sort of thing that produces slightly-wrong features, and
//! slightly-wrong features produce slightly-wrong transcripts — which is the
//! hardest failure to notice, because nothing errors. `scripts/check-mel.py`
//! recomputes all of this independently in numpy and compares.

use rustfft::num_complex::Complex32;
use rustfft::FftPlanner;

use super::audio::TARGET_RATE;

/// The window length, in samples. 25 ms at [`TARGET_RATE`].
pub const N_FFT: usize = 400;
/// The step between windows. 10 ms at [`TARGET_RATE`].
pub const HOP_LENGTH: usize = 160;
/// Mel bins, which is the encoder's input height.
pub const N_MELS: usize = 80;
/// Frequency bins from a real 400-point FFT: `n_fft / 2 + 1`.
pub const N_FREQ_BINS: usize = N_FFT / 2 + 1;
/// Samples in the 30-second window the model was trained on.
pub const N_SAMPLES: usize = 480_000;
/// Frames the encoder expects. `N_SAMPLES / HOP_LENGTH`.
pub const N_FRAMES: usize = N_SAMPLES / HOP_LENGTH;
/// The highest frequency the filterbank covers, which is Nyquist.
pub const MAX_FREQUENCY: f64 = 8_000.0;

/// The floor applied before the logarithm.
///
/// `clamp(mel, min=1e-10).log10()` — without it, a silent frame gives `-inf` and
/// the whole spectrogram becomes `NaN`.
const LOG_FLOOR: f32 = 1e-10;

/// How far below the peak the floor sits, in decades.
///
/// `maximum(log_spec, log_spec.max() - 8.0)` raises everything quieter than
/// `peak / 1e8` up to that level. It is a dynamic-range compressor, and it is
/// what stops one loud moment from making the rest of the clip vanish.
const DYNAMIC_RANGE: f32 = 8.0;

/// The offset and scale of the final normalisation: `(x + 4) / 4`.
const NORM_OFFSET: f32 = 4.0;
const NORM_SCALE: f32 = 4.0;

// ---------------------------------------------------------------------------
// The Slaney mel scale
// ---------------------------------------------------------------------------

/// Hz per mel below the logarithmic knee.
const F_SP: f64 = 200.0 / 3.0;
/// Where the scale switches from linear to logarithmic.
const MIN_LOG_HZ: f64 = 1_000.0;
/// The mel value at [`MIN_LOG_HZ`], which is `1000 / f_sp = 15`.
const MIN_LOG_MEL: f64 = MIN_LOG_HZ / F_SP;

/// The step size of the logarithmic part: `ln(6.4) / 27`.
///
/// Not a `const` because `ln` is not callable in a constant expression on
/// stable Rust. Computed once per call, which costs nothing next to an FFT.
fn log_step() -> f64 {
    6.4f64.ln() / 27.0
}

/// Hertz to mel, on the **Slaney** scale.
///
/// Slaney rather than HTK: librosa's default, and therefore Whisper's. The two
/// differ above 1 kHz — HTK stays logarithmic, Slaney breaks at 1 kHz and uses a
/// different log step — so using the wrong one shifts every filter's centre a
/// little and the features come out subtly wrong.
pub fn hz_to_mel(frequency: f64) -> f64 {
    if frequency >= MIN_LOG_HZ {
        MIN_LOG_MEL + (frequency / MIN_LOG_HZ).ln() / log_step()
    } else {
        frequency / F_SP
    }
}

/// Mel to hertz, the inverse of [`hz_to_mel`].
pub fn mel_to_hz(mel: f64) -> f64 {
    if mel >= MIN_LOG_MEL {
        MIN_LOG_HZ * (log_step() * (mel - MIN_LOG_MEL)).exp()
    } else {
        F_SP * mel
    }
}

/// `n` frequencies evenly spaced on the mel scale.
fn mel_frequencies(n: usize, min_hz: f64, max_hz: f64) -> Vec<f64> {
    let low = hz_to_mel(min_hz);
    let high = hz_to_mel(max_hz);
    (0..n)
        .map(|index| {
            let fraction = index as f64 / (n - 1).max(1) as f64;
            mel_to_hz(low + (high - low) * fraction)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The filterbank
// ---------------------------------------------------------------------------

/// The mel filterbank: [`N_MELS`] triangular filters over [`N_FREQ_BINS`] bins.
///
/// Built the way librosa's `filters.mel(htk=False, norm="slaney")` builds it,
/// which is what Whisper's weights were trained against. Each filter is a
/// triangle whose peak sits on a mel-spaced frequency, and each is scaled by
/// Slaney normalisation — `2 / (f_high - f_low)` — so a filter covering a wide
/// band is not automatically louder than a narrow one.
#[derive(Debug, Clone)]
pub struct MelFilters {
    /// Row-major, [`N_MELS`] × [`N_FREQ_BINS`].
    data: Vec<f32>,
}

impl MelFilters {
    /// Builds the filterbank.
    pub fn new() -> Self {
        // The triangle vertices: one more than the number of filters, with a
        // spare at each end so every filter has both feet on the floor.
        let points = mel_frequencies(N_MELS + 2, 0.0, MAX_FREQUENCY);
        // The centre frequency of each FFT bin: k * sample_rate / n_fft, which
        // at 16 kHz and a 400-point transform is every 40 Hz.
        let bin_hz: Vec<f64> = (0..N_FREQ_BINS)
            .map(|k| k as f64 * TARGET_RATE as f64 / N_FFT as f64)
            .collect();

        let diffs: Vec<f64> = points.windows(2).map(|pair| pair[1] - pair[0]).collect();

        let mut data = vec![0.0f32; N_MELS * N_FREQ_BINS];
        for filter in 0..N_MELS {
            // Rising edge from the previous vertex to this one, falling edge
            // from this one to the next. `min` of the two, floored at zero,
            // gives the triangle.
            let left_width = diffs[filter];
            let right_width = diffs[filter + 1];
            let lowest = points[filter];
            let highest = points[filter + 2];
            // Slaney normalisation, in the Hz domain.
            let norm = 2.0 / (highest - lowest);

            for bin in 0..N_FREQ_BINS {
                let frequency = bin_hz[bin];
                let rising = (frequency - lowest) / left_width;
                let falling = (highest - frequency) / right_width;
                let weight = rising.min(falling).max(0.0);
                data[filter * N_FREQ_BINS + bin] = (weight * norm) as f32;
            }
        }

        Self { data }
    }

    /// The filterbank as a row-major slice.
    pub fn data(&self) -> &[f32] {
        &self.data
    }

    /// One filter's weights.
    pub fn filter(&self, index: usize) -> &[f32] {
        let start = index * N_FREQ_BINS;
        &self.data[start..start + N_FREQ_BINS]
    }

    /// Applies the filterbank to a power spectrum, accumulating into `out`.
    ///
    /// Split out from [`log_mel`] so the matmul can be tested on its own, with
    /// a spectrum whose answer is known by hand.
    pub fn apply(&self, power: &[f32], out: &mut [f32]) {
        debug_assert_eq!(power.len(), N_FREQ_BINS);
        debug_assert_eq!(out.len(), N_MELS);

        for filter in 0..N_MELS {
            let row = &self.data[filter * N_FREQ_BINS..(filter + 1) * N_FREQ_BINS];
            let mut sum = 0.0f32;
            for (weight, value) in row.iter().zip(power) {
                // The filterbank is sparse — most weights are zero — but the
                // multiply is cheaper than the branch would be.
                sum += weight * value;
            }
            out[filter] = sum;
        }
    }
}

impl Default for MelFilters {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Windowing, padding, framing
// ---------------------------------------------------------------------------

/// A **periodic** Hann window of `n` points.
///
/// `0.5 - 0.5 * cos(2πn / N)`, matching `torch.hann_window(N)`'s default. The
/// symmetric form divides by `N - 1` and gives a different `w[N-1]`, which is a
/// one-sample difference that quietly shifts every frame's spectrum.
pub fn hann_window(n: usize) -> Vec<f32> {
    (0..n)
        .map(|index| {
            let angle = 2.0 * std::f64::consts::PI * index as f64 / n as f64;
            (0.5 - 0.5 * angle.cos()) as f32
        })
        .collect()
}

/// Pads or trims `samples` to exactly `n`, padding with silence on the right.
///
/// Matches `WhisperFeatureExtractor`'s `padding_side: "right"` and
/// `padding_value: 0.0`. A longer clip is truncated rather than split: the
/// caller is expected to have chunked it, since 30 seconds is the model's whole
/// receptive field and a clip cut mid-word transcribes as a cut word.
pub fn pad_or_trim(samples: &[f32], n: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(n);
    let take = samples.len().min(n);
    out.extend_from_slice(&samples[..take]);
    out.resize(n, 0.0);
    out
}

/// Mirrors `pad` samples at each edge, without repeating the edge sample.
///
/// `[a, b, c, d]` with `pad = 2` becomes `[c, b, a, b, c, d, c, b]`. This is
/// `torch.stft`'s `pad_mode='reflect'`, and it is what makes the frame count
/// come out at 3001 rather than 2998.
///
/// Returns the input unchanged when it is too short to reflect, which cannot
/// happen for a 30-second window but would panic otherwise.
pub fn reflect_pad(samples: &[f32], pad: usize) -> Vec<f32> {
    if samples.len() <= pad || pad == 0 {
        return samples.to_vec();
    }

    let mut out = Vec::with_capacity(samples.len() + 2 * pad);

    // Left edge: samples[pad] down to samples[1].
    for index in (1..=pad).rev() {
        out.push(samples[index]);
    }
    out.extend_from_slice(samples);
    // Right edge: samples[len-2] down to samples[len-1-pad].
    for offset in 0..pad {
        out.push(samples[samples.len() - 2 - offset]);
    }

    out
}

// ---------------------------------------------------------------------------
// The spectrogram
// ---------------------------------------------------------------------------

/// A log-mel spectrogram: [`N_MELS`] rows of `frames` values.
#[derive(Debug, Clone, PartialEq)]
pub struct MelSpectrogram {
    pub bins: usize,
    pub frames: usize,
    /// Row-major, `bins * frames`.
    pub data: Vec<f32>,
}

impl MelSpectrogram {
    /// One mel bin across every frame.
    pub fn bin(&self, index: usize) -> &[f32] {
        let start = index * self.frames;
        &self.data[start..start + self.frames]
    }

    /// One frame across every bin.
    pub fn frame(&self, index: usize) -> Vec<f32> {
        (0..self.bins).map(|bin| self.data[bin * self.frames + index]).collect()
    }

    /// The value range, which is a cheap sanity check on the normalisation.
    pub fn range(&self) -> (f32, f32) {
        self.data.iter().fold((f32::MAX, f32::MIN), |(low, high), value| {
            (low.min(*value), high.max(*value))
        })
    }
}

/// Computes the log-mel spectrogram the Whisper encoder expects.
///
/// The input must already be 16 kHz mono — [`super::audio::resample_to_16k`]
/// and [`super::audio::downmix`] do that — and is padded or trimmed to 30
/// seconds here. The result is always `80 × 3000`.
pub fn log_mel(samples: &[f32]) -> MelSpectrogram {
    let filters = MelFilters::new();
    log_mel_with(samples, &filters)
}

/// As [`log_mel`], but reusing an already-built filterbank.
///
/// Building the filterbank costs about a millisecond, which matters when
/// transcribing many short clips back to back — each one would otherwise
/// rebuild the same 80 × 201 matrix.
pub fn log_mel_with(samples: &[f32], filters: &MelFilters) -> MelSpectrogram {
    let mut spectrogram = log_mel_raw(samples, filters);
    normalize(&mut spectrogram);
    spectrogram
}

/// The logarithm of the mel energies, with the floor but **without** the
/// dynamic-range clamp or the affine normalisation.
///
/// Split out because those last two steps are *global*: both depend on the peak
/// across the whole spectrogram. That makes them impossible to verify by
/// recomputing only part of one — and trivially verifiable in full, which is
/// what `scripts/check-mel.py` does. Everything structural (padding,
/// reflection, windowing, the transform, the filterbank) is checked here, where
/// no global state is involved.
pub fn log_mel_raw(samples: &[f32], filters: &MelFilters) -> MelSpectrogram {
    let padded = pad_or_trim(samples, N_SAMPLES);
    let window = hann_window(N_FFT);
    let framed = reflect_pad(&padded, N_FFT / 2);

    // `center=True` gives one more frame than the hop count divides into, and
    // Whisper drops it: 3001 becomes the 3000 the encoder expects.
    let frame_count = 1 + (framed.len() - N_FFT) / HOP_LENGTH;
    let frames = frame_count.saturating_sub(1).max(1);

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(N_FFT);

    let mut buffer = vec![Complex32::new(0.0, 0.0); N_FFT];
    let mut power = vec![0.0f32; N_FREQ_BINS];
    let mut mel = vec![0.0f32; N_MELS];
    let mut data = vec![0.0f32; N_MELS * frames];

    for frame in 0..frames {
        let start = frame * HOP_LENGTH;

        // Real input, so the imaginary part starts at zero. Overwriting every
        // slot each time avoids a stale value leaking between frames.
        for index in 0..N_FFT {
            buffer[index] = Complex32::new(framed[start + index] * window[index], 0.0);
        }
        fft.process(&mut buffer);

        // Power, not magnitude: Whisper squares the magnitude spectrum.
        for bin in 0..N_FREQ_BINS {
            let value = buffer[bin];
            power[bin] = value.re * value.re + value.im * value.im;
        }

        filters.apply(&power, &mut mel);

        for (bin, energy) in mel.iter().enumerate() {
            data[bin * frames + frame] = *energy;
        }
    }

    // log10 with a floor. Without the floor, a fully silent frame gives -inf
    // and the whole spectrogram becomes NaN. The clamp and the normalisation
    // are a separate, global pass — see `normalize`.
    for value in data.iter_mut() {
        *value = value.max(LOG_FLOOR).log10();
    }

    MelSpectrogram {
        bins: N_MELS,
        frames,
        data,
    }
}

/// Applies the dynamic-range clamp and the affine normalisation, in place.
///
/// `maximum(log_spec, log_spec.max() - 8.0)`, then `(x + 4) / 4`.
///
/// **Both steps are global**, which is worth knowing for two reasons. The clamp
/// is relative to the *loudest value anywhere in the clip*, not to each frame,
/// so a spectrogram cannot be normalised frame by frame — and a clip whose loud
/// part is at the end gets a lower floor throughout than the same clip with its
/// loud part at the start. That is Whisper's behaviour, not a quirk here, but it
/// means the features depend on the whole 30-second window.
pub fn normalize(spectrogram: &mut MelSpectrogram) {
    let peak = spectrogram
        .data
        .iter()
        .fold(f32::MIN, |highest, value| highest.max(*value));
    let floor = peak - DYNAMIC_RANGE;

    for value in spectrogram.data.iter_mut() {
        *value = (*value).max(floor);
        *value = (*value + NORM_OFFSET) / NORM_SCALE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- the mel scale ------------------------------------------------------

    #[test]
    fn the_scale_breaks_at_1000_hz() {
        // Both branches must agree at the knee, or the filterbank has a
        // discontinuity through the middle of the speech band.
        assert!((hz_to_mel(MIN_LOG_HZ) - MIN_LOG_MEL).abs() < 1e-9);
        assert!((mel_to_hz(MIN_LOG_MEL) - MIN_LOG_HZ).abs() < 1e-6);
        // And just either side.
        assert!((hz_to_mel(999.9) - 999.9 / F_SP).abs() < 1e-6);
        assert!((mel_to_hz(15.0) - 1000.0).abs() < 0.01);
    }

    #[test]
    fn mel_conversion_round_trips() {
        for hz in [0.0, 100.0, 440.0, 999.0, 1000.0, 2000.0, 8000.0] {
            let back = mel_to_hz(hz_to_mel(hz));
            assert!((back - hz).abs() < 0.01, "{hz} became {back}");
        }
    }

    #[test]
    fn the_scale_is_monotonic() {
        let mut previous = f64::MIN;
        for hz in (0..=8000).step_by(50) {
            let mel = hz_to_mel(hz as f64);
            assert!(mel > previous, "the scale went backwards at {hz} Hz");
            previous = mel;
        }
    }

    #[test]
    fn zero_hertz_is_zero_mel() {
        assert_eq!(hz_to_mel(0.0), 0.0);
        assert_eq!(mel_to_hz(0.0), 0.0);
    }

    // -- the filterbank -----------------------------------------------------

    #[test]
    fn the_filterbank_has_the_shape_the_encoder_expects() {
        let filters = MelFilters::new();
        assert_eq!(filters.data().len(), N_MELS * N_FREQ_BINS);
        assert_eq!(filters.filter(0).len(), N_FREQ_BINS);
        assert_eq!(filters.filter(N_MELS - 1).len(), N_FREQ_BINS);
    }

    #[test]
    fn every_weight_is_non_negative_and_finite() {
        let filters = MelFilters::new();
        for (index, weight) in filters.data().iter().enumerate() {
            assert!(weight.is_finite(), "weight {index} is not finite");
            assert!(*weight >= 0.0, "weight {index} is negative: {weight}");
        }
    }

    #[test]
    fn every_filter_covers_something() {
        // A filter with no weight anywhere is a dead mel bin, which would make
        // an entire row of the encoder's input constant.
        let filters = MelFilters::new();
        for index in 0..N_MELS {
            let total: f32 = filters.filter(index).iter().sum();
            assert!(total > 0.0, "filter {index} is entirely zero");
        }
    }

    #[test]
    fn filters_are_ordered_low_to_high() {
        // Each filter's centre of mass must sit above the previous one's, or
        // the bins do not correspond to ascending frequency.
        let filters = MelFilters::new();
        let centroid = |index: usize| -> f64 {
            let row = filters.filter(index);
            let weighted: f64 = row
                .iter()
                .enumerate()
                .map(|(bin, weight)| bin as f64 * *weight as f64)
                .sum();
            let total: f64 = row.iter().map(|w| *w as f64).sum();
            weighted / total
        };

        let mut previous = f64::MIN;
        for index in 0..N_MELS {
            let centre = centroid(index);
            assert!(
                centre > previous,
                "filter {index} sits at bin {centre}, not above {previous}"
            );
            previous = centre;
        }
    }

    #[test]
    fn the_extreme_bins_are_deliberately_not_covered() {
        // librosa's construction puts a triangle vertex at 0 Hz and another at
        // Nyquist, and a vertex is a *foot* — so DC and the top bin get zero
        // weight from every filter. This surprises people, so it is asserted
        // rather than left to be rediscovered: the encoder's first and last
        // input bins are always zero. (Whisper's `center=True` STFT means the
        // missing DC bin is not information the model needs.)
        let filters = MelFilters::new();
        for index in 0..N_MELS {
            let row = filters.filter(index);
            assert_eq!(row[0], 0.0, "filter {index} has weight at DC");
            assert_eq!(
                row[N_FREQ_BINS - 1],
                0.0,
                "filter {index} has weight at Nyquist"
            );
        }
    }

    #[test]
    fn the_filterbank_covers_the_usable_band() {
        // Every bin the filters are capable of covering should be covered by at
        // least one of them, or there is a hole in the spectrum the encoder
        // cannot see. Quantified rather than asserted per-bin, because a
        // triangle vertex landing exactly on a bin leaves that bin at zero from
        // both neighbours — a legitimate artefact of sampling a continuous
        // filterbank, not a bug.
        let filters = MelFilters::new();

        let covered = (0..N_FREQ_BINS)
            .filter(|bin| (0..N_MELS).any(|index| filters.filter(index)[*bin] > 0.0))
            .count();

        assert!(
            covered >= N_FREQ_BINS - 12,
            "only {covered} of {N_FREQ_BINS} bins are covered"
        );
        // And the coverage is spread across the band rather than bunched at one
        // end, which a wrong mel scale would cause.
        let lowest = (0..N_FREQ_BINS)
            .find(|bin| (0..N_MELS).any(|i| filters.filter(i)[*bin] > 0.0))
            .expect("some bin is covered");
        let highest = (0..N_FREQ_BINS)
            .rev()
            .find(|bin| (0..N_MELS).any(|i| filters.filter(i)[*bin] > 0.0))
            .expect("some bin is covered");
        assert!(lowest <= 2, "coverage starts at bin {lowest}");
        assert!(
            highest >= N_FREQ_BINS - 3,
            "coverage stops at bin {highest} of {N_FREQ_BINS}"
        );
    }

    #[test]
    fn the_mel_scale_matches_slaney_at_known_points() {
        // Hard values, so a switch to the HTK scale — which is logarithmic all
        // the way up instead of breaking at 1 kHz — fails here rather than as a
        // subtly wrong transcript.
        assert!((hz_to_mel(1000.0) - 15.0).abs() < 1e-9);
        // 15 + ln(8) / (ln(6.4)/27)
        let expected_8000 = 15.0 + 8f64.ln() / (6.4f64.ln() / 27.0);
        assert!((hz_to_mel(8000.0) - expected_8000).abs() < 1e-6);
        // Below the knee the scale is linear at 66.67 Hz per mel.
        assert!((hz_to_mel(666.666_666) - 10.0).abs() < 1e-6);
    }

    #[test]
    fn a_single_bin_applies_to_at_most_a_few_filters() {
        // Triangles overlap by design, but a bin should not contribute to
        // dozens of filters: that would mean the vertices are wrong.
        let filters = MelFilters::new();
        for bin in 0..N_FREQ_BINS {
            let touching = (0..N_MELS)
                .filter(|index| filters.filter(*index)[bin] > 0.0)
                .count();
            assert!(touching <= 4, "bin {bin} touches {touching} filters");
        }
    }

    #[test]
    fn applying_the_filterbank_against_a_known_spectrum() {
        let filters = MelFilters::new();

        // All the energy in one bin. The result must be that bin's column of
        // the filterbank, which is a value we can read directly.
        let mut power = vec![0.0f32; N_FREQ_BINS];
        power[100] = 1.0;

        let mut out = vec![0.0f32; N_MELS];
        filters.apply(&power, &mut out);

        for filter in 0..N_MELS {
            assert_eq!(out[filter], filters.filter(filter)[100]);
        }

        // Silence in gives silence out.
        let mut silent = vec![0.0f32; N_MELS];
        filters.apply(&vec![0.0f32; N_FREQ_BINS], &mut silent);
        assert!(silent.iter().all(|value| *value == 0.0));
    }

    // -- windowing ----------------------------------------------------------

    #[test]
    fn the_window_is_periodic_not_symmetric() {
        let window = hann_window(N_FFT);
        assert_eq!(window.len(), N_FFT);
        // Periodic: the first sample is exactly zero, the last is *nearly*
        // zero but not quite. A symmetric window would give exactly zero at
        // both ends.
        assert_eq!(window[0], 0.0);
        assert!(window[N_FFT - 1] > 0.0, "symmetric, not periodic");
        assert!(window[N_FFT - 1] < 0.001);
        // And it peaks in the middle.
        let peak = window.iter().cloned().fold(f32::MIN, f32::max);
        assert!((peak - 1.0).abs() < 1e-6);
        assert!((window[N_FFT / 2] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn the_window_is_symmetric_about_its_middle_sample() {
        // Periodic Hann is symmetric about N/2, not (N-1)/2.
        let window = hann_window(N_FFT);
        for offset in 1..N_FFT / 2 {
            let left = window[N_FFT / 2 - offset];
            let right = window[N_FFT / 2 + offset];
            assert!((left - right).abs() < 1e-6, "asymmetric at offset {offset}");
        }
    }

    // -- padding ------------------------------------------------------------

    #[test]
    fn padding_extends_a_short_clip_with_silence() {
        let padded = pad_or_trim(&[1.0, 2.0], 5);
        assert_eq!(padded, vec![1.0, 2.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn trimming_cuts_a_long_clip() {
        let padded = pad_or_trim(&[1.0, 2.0, 3.0, 4.0], 2);
        assert_eq!(padded, vec![1.0, 2.0]);
    }

    #[test]
    fn an_exact_length_clip_is_untouched() {
        let samples = vec![0.5; 10];
        assert_eq!(pad_or_trim(&samples, 10), samples);
    }

    #[test]
    fn reflect_padding_mirrors_without_repeating_the_edge() {
        let padded = reflect_pad(&[1.0, 2.0, 3.0, 4.0], 2);
        // [3, 2] then the signal then [3, 2] — never the edge value twice, which
        // is what 'replicate' would do and what would bias the edge frames.
        assert_eq!(padded, vec![3.0, 2.0, 1.0, 2.0, 3.0, 4.0, 3.0, 2.0]);
    }

    #[test]
    fn reflect_padding_adds_exactly_two_pads() {
        let samples = vec![0.1; 100];
        assert_eq!(reflect_pad(&samples, 20).len(), 140);
        assert_eq!(reflect_pad(&samples, 0).len(), 100);
    }

    #[test]
    fn reflect_padding_a_too_short_slice_returns_it_unchanged() {
        // Would index backwards otherwise. Cannot happen for a 30-second
        // window, which is the point of making it total.
        let samples = vec![1.0, 2.0];
        assert_eq!(reflect_pad(&samples, 5), samples);
        assert!(reflect_pad(&[], 5).is_empty());
    }

    // -- the whole spectrogram ---------------------------------------------

    #[test]
    fn the_spectrogram_has_the_shape_the_encoder_expects() {
        let mel = log_mel(&vec![0.0f32; TARGET_RATE as usize]);
        assert_eq!(mel.bins, N_MELS);
        assert_eq!(mel.frames, N_FRAMES);
        assert_eq!(mel.data.len(), N_MELS * N_FRAMES);
    }

    #[test]
    fn a_silent_clip_produces_finite_values() {
        // The reason LOG_FLOOR exists: log10(0) is -inf, and one -inf makes
        // every later arithmetic step NaN.
        let mel = log_mel(&vec![0.0f32; TARGET_RATE as usize]);
        assert!(
            mel.data.iter().all(|value| value.is_finite()),
            "silence produced a non-finite value"
        );
    }

    #[test]
    fn the_normalised_range_is_plausible() {
        let mut samples = vec![0.0f32; TARGET_RATE as usize];
        // A mid-band tone, which is where speech energy sits.
        for (index, sample) in samples.iter_mut().enumerate() {
            let t = index as f32 / TARGET_RATE as f32;
            *sample = 0.3 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
        }

        let mel = log_mel(&samples);
        let (low, high) = mel.range();
        // Whisper's normalisation is designed to land roughly in [-1, 1.5].
        // Anything wildly outside means the flooring or the affine step is
        // wrong, which is exactly the kind of error that produces a transcript
        // of plausible-looking nonsense.
        assert!(low >= -2.0 && high <= 3.0, "range {low} .. {high}");
        assert!(high > low, "the spectrogram is constant");
    }

    #[test]
    fn a_tone_shows_up_in_the_bin_covering_its_frequency() {
        // A 1000 Hz sine should light up the mel bin whose triangle covers
        // 1000 Hz, and the loudest bin should be near there rather than at DC
        // or at Nyquist.
        let mut samples = vec![0.0f32; TARGET_RATE as usize];
        for (index, sample) in samples.iter_mut().enumerate() {
            let t = index as f32 / TARGET_RATE as f32;
            *sample = 0.5 * (2.0 * std::f32::consts::PI * 1000.0 * t).sin();
        }

        let mel = log_mel(&samples);
        // Average each bin over time, since the whole clip carries the tone.
        let energy: Vec<f32> = (0..N_MELS)
            .map(|bin| mel.bin(bin).iter().sum::<f32>() / mel.frames as f32)
            .collect();

        let loudest = energy
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).expect("finite"))
            .map(|(index, _)| index)
            .expect("non-empty");

        // Find which filter covers 1000 Hz by reading the filterbank, rather
        // than asserting a hardcoded index that would be brittle.
        let filters = MelFilters::new();
        let bin_at_1000 = (1000.0 / (TARGET_RATE as f64 / N_FFT as f64)) as usize;
        let covering = (0..N_MELS)
            .max_by(|a, b| {
                filters.filter(*a)[bin_at_1000]
                    .partial_cmp(&filters.filter(*b)[bin_at_1000])
                    .expect("finite")
            })
            .expect("non-empty");

        let distance = loudest.abs_diff(covering);
        assert!(
            distance <= 2,
            "the 1 kHz tone peaked at bin {loudest}, but {covering} covers 1 kHz"
        );
    }

    #[test]
    fn scaling_the_input_shifts_the_spectrogram_by_a_constant() {
        // The mel step runs on the *power* spectrum, so ten times the amplitude
        // is a hundred times the power — two decades of log10, which is 0.5
        // after the normalisation divides by four. Getting this factor wrong is
        // the classic sign of a front-end that squares when it should not, or
        // vice versa.
        //
        // The claim worth testing is that the shift is *uniform*: the final
        // normalisation is affine and the dynamic-range clamp is relative to
        // each spectrogram's own peak, so scaling the input cannot change the
        // pattern, only its offset.
        let mut quiet = vec![0.0f32; TARGET_RATE as usize];
        for (index, sample) in quiet.iter_mut().enumerate() {
            let t = index as f32 / TARGET_RATE as f32;
            *sample = 0.01 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
        }
        let loud: Vec<f32> = quiet.iter().map(|s| s * 10.0).collect();

        let a = log_mel(&quiet);
        let b = log_mel(&loud);

        // The shift is exactly uniform, including on floored values: the floor
        // is `peak - 8`, and the peak moves by the same 2 decades, so a floored
        // value moves with it. That is why this compares every element rather
        // than filtering out the quiet ones.
        let expected = 0.5f32;
        let mut worst = 0.0f32;
        let mut worst_at = 0usize;

        for (index, (x, y)) in a.data.iter().zip(&b.data).enumerate() {
            let shift = y - x;
            let error = (shift - expected).abs();
            if error > worst {
                worst = error;
                worst_at = index;
            }
        }

        assert!(
            worst < 0.01,
            "the shift is not uniform: worst deviation {worst} at index {worst_at}, \
             where it is {} rather than {expected}",
            b.data[worst_at] - a.data[worst_at]
        );
    }

    #[test]
    fn a_long_clip_is_trimmed_rather_than_growing_the_frame_count() {
        let double = vec![0.1f32; N_SAMPLES * 2];
        let mel = log_mel(&double);
        assert_eq!(mel.frames, N_FRAMES);
    }

    #[test]
    fn the_two_normalisation_steps_are_global_not_per_frame() {
        // The clamp is `max(x, peak - 8)` where the peak spans the whole
        // spectrogram. A per-frame floor would give different results for a
        // clip whose loud part is in one frame and a clip whose energy is
        // spread evenly — and this asserts the global behaviour, because it is
        // the behaviour Whisper's weights were trained with.
        let mut quiet_then_loud = vec![0.0f32; TARGET_RATE as usize * 2];
        let half = TARGET_RATE as usize;
        for (index, sample) in quiet_then_loud.iter_mut().enumerate() {
            let t = index as f32 / TARGET_RATE as f32;
            let amplitude = if index < half { 0.01 } else { 1.0 };
            *sample = amplitude * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
        }

        let spectrogram = log_mel(&quiet_then_loud);
        let (low, _) = spectrogram.range();

        // The quiet half should be sitting on the floor, because the loud half
        // set it 8 decades lower than its own peak.
        let quiet_frames = half / HOP_LENGTH;
        let quiet_portion: Vec<f32> = (0..spectrogram.bins)
            .flat_map(|bin| spectrogram.bin(bin)[..quiet_frames].to_vec())
            .collect();
        let on_floor = quiet_portion.iter().filter(|value| **value == low).count();

        assert!(
            on_floor * 2 > quiet_portion.len(),
            "only {on_floor} of {} quiet values hit the floor, so the clamp is not global",
            quiet_portion.len()
        );
    }

    #[test]
    fn normalising_is_idempotent_in_its_input_but_not_its_output() {
        // Applying `normalize` twice is *not* the same as applying it once —
        // the second pass finds a different peak and shifts again. Worth
        // pinning, because it is the reason the two steps are separate
        // functions rather than folded into the computation.
        let mut samples = vec![0.0f32; TARGET_RATE as usize];
        for (index, sample) in samples.iter_mut().enumerate() {
            let t = index as f32 / TARGET_RATE as f32;
            *sample = 0.3 * (2.0 * std::f32::consts::PI * 500.0 * t).sin();
        }

        let filters = MelFilters::new();
        let mut once = log_mel_raw(&samples, &filters);
        normalize(&mut once);

        let mut twice = log_mel_raw(&samples, &filters);
        normalize(&mut twice);
        let after_once = twice.data.clone();
        normalize(&mut twice);

        assert_eq!(after_once, once.data, "the first pass must be deterministic");
        assert_ne!(
            twice.data, after_once,
            "a second pass should shift again, which is why it is called once"
        );
    }

    #[test]
    fn raw_and_normalised_differ_only_by_the_global_steps() {
        // The split exists so the structural part can be checked independently.
        // This asserts the two really are the same computation up to the clamp
        // and the affine step.
        let mut samples = vec![0.0f32; TARGET_RATE as usize];
        for (index, sample) in samples.iter_mut().enumerate() {
            let t = index as f32 / TARGET_RATE as f32;
            *sample = 0.3 * (2.0 * std::f32::consts::PI * 700.0 * t).sin();
        }

        let filters = MelFilters::new();
        let raw = log_mel_raw(&samples, &filters);
        let mut normalised = raw.clone();
        normalize(&mut normalised);

        // Structural: same shape, same frame count.
        assert_eq!(raw.bins, normalised.bins);
        assert_eq!(raw.frames, normalised.frames);

        // And the minimum legitimately *differs*: the clamp exists precisely to
        // raise everything quieter than `peak - 8` up to that floor. Raw values
        // go far lower — a silent frame sits at log10(1e-10) = -10.
        assert!(
            raw.range().0 < normalised.range().0,
            "the clamp should have raised the floor: raw {} vs normalised {}",
            raw.range().0,
            normalised.range().0
        );

        // And the transform is exactly the two steps, everywhere.
        let peak = raw.data.iter().cloned().fold(f32::MIN, f32::max);
        let floor = peak - DYNAMIC_RANGE;
        for (before, after) in raw.data.iter().zip(&normalised.data) {
            let expected = (before.max(floor) + NORM_OFFSET) / NORM_SCALE;
            assert!((after - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn the_log_floor_keeps_silence_finite_in_the_raw_output_too() {
        let filters = MelFilters::new();
        let raw = log_mel_raw(&vec![0.0f32; TARGET_RATE as usize], &filters);
        assert!(raw.data.iter().all(|value| value.is_finite()));
        // Pure silence is exactly the floor everywhere.
        assert!(
            raw.data
                .iter()
                .all(|value| (value - LOG_FLOOR.log10()).abs() < 1e-5),
            "silence should sit exactly on the log floor"
        );
    }

    #[test]
    fn building_the_filterbank_twice_gives_the_same_answer() {
        let a = MelFilters::new();
        let b = MelFilters::new();
        assert_eq!(a.data(), b.data());
    }
}
