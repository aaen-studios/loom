//! Audio plumbing: resampling, framing, and sample-format conversion.
//!
//! Everything here is pure and allocation-light, because it runs on every
//! microphone callback. That is also why it is its own module: it is the layer
//! most likely to be subtly wrong in ways that only show up as *bad
//! transcripts* rather than as errors, so it is written to be testable without
//! a microphone, a model, or a sound card.
//!
//! # Why 16 kHz
//!
//! Speech-to-text models expect 16 kHz mono. A microphone hands over whatever
//! the device is set to — 44.1 or 48 kHz in practice — so resampling is not
//! optional, and doing it badly is one of the classic causes of a recogniser
//! that "almost works".

/// What speech processing expects.
pub const TARGET_RATE: u32 = 16_000;

/// Silero VAD's window, in samples at [`TARGET_RATE`]. Fixed by the model.
pub const VAD_WINDOW: usize = 512;

/// Converts `f32` in `[-1, 1]` to little-endian `i16` bytes.
///
/// Clamped before scaling: a value outside the range wraps to a large value of
/// the opposite sign, which is a loud click rather than a quiet distortion.
pub fn f32_to_i16_bytes(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        out.extend_from_slice(&((clamped * i16::MAX as f32).round() as i16).to_le_bytes());
    }
    out
}

/// Converts little-endian `i16` bytes back to `f32` in `[-1, 1]`.
///
/// A trailing odd byte is ignored rather than treated as an error: a stream can
/// be cut mid-sample, and dropping one byte is better than dropping the frame.
pub fn i16_bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / i16::MAX as f32)
        .collect()
}

/// Resamples mono audio to [`TARGET_RATE`].
///
/// Two paths, chosen by whether the ratio is a whole number:
///
/// * **Integer decimation** averages each group: a box filter, so it attenuates
///   content above the output's Nyquist limit by roughly the decimation factor
///   rather than removing it. Picking every Nth sample instead is the tempting
///   one-liner and it does not attenuate at all — 48 kHz → 16 kHz would fold
///   everything from 8–24 kHz straight down into the speech band. Averaging is
///   the weaker of the two filters that would be correct here and the cheapest
///   of them; for speech, whose energy above 8 kHz is low, and for capture
///   hardware that filters on its own, it is enough.
/// * **Linear interpolation** otherwise, which is imperfect but adequate for
///   speech and cheap enough for a callback.
///
/// Audio already at the target rate is returned as-is, so the common case costs
/// nothing.
pub fn resample_to_16k(samples: &[f32], from_rate: u32) -> Vec<f32> {
    if from_rate == TARGET_RATE {
        return samples.to_vec();
    }
    if samples.is_empty() || from_rate == 0 {
        return Vec::new();
    }

    if from_rate % TARGET_RATE == 0 {
        let factor = (from_rate / TARGET_RATE) as usize;
        let out_len = samples.len() / factor;
        let mut out = Vec::with_capacity(out_len);
        for index in 0..out_len {
            let start = index * factor;
            let group = &samples[start..start + factor];
            out.push(group.iter().sum::<f32>() / group.len() as f32);
        }
        return out;
    }

    // Linear interpolation. The last output sample lands one step before the
    // end, so it never reads past the input.
    let ratio = TARGET_RATE as f64 / from_rate as f64;
    let out_len = ((samples.len() as f64) * ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);

    for index in 0..out_len {
        let position = index as f64 / ratio;
        let left = position.floor() as usize;
        let frac = (position - left as f64) as f32;
        let a = samples.get(left).copied().unwrap_or(0.0);
        let b = samples.get(left + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
}

/// Averages every channel down to one.
///
/// Averaging rather than taking the first channel: a device whose channels are
/// out of phase would otherwise lose most of its signal.
pub fn downmix(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// A fixed-capacity buffer of samples, oldest first.
///
/// Bounded on purpose. Microphone audio arrives faster than it is consumed, and
/// an unbounded `Vec` would grow until the process died. Overflowing drops the
/// *oldest* samples: a recogniser that misses the start of a long pause is
/// better off than one that stops working entirely.
#[derive(Debug, Clone)]
pub struct RingBuffer {
    data: Vec<f32>,
    capacity: usize,
    /// Index of the oldest sample.
    start: usize,
    len: usize,
}

impl RingBuffer {
    /// A buffer holding `capacity` samples.
    pub fn new(capacity: usize) -> Self {
        Self {
            data: vec![0.0; capacity.max(1)],
            capacity: capacity.max(1),
            start: 0,
            len: 0,
        }
    }

    /// A buffer holding `seconds` at [`TARGET_RATE`].
    pub fn seconds(seconds: f32) -> Self {
        Self::new((seconds * TARGET_RATE as f32).round() as usize)
    }

    /// How many samples are buffered.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// How much room is left before the oldest samples get dropped.
    pub fn free(&self) -> usize {
        self.capacity - self.len
    }

    /// Discards everything buffered.
    pub fn clear(&mut self) {
        self.start = 0;
        self.len = 0;
    }

    /// Appends samples, dropping the oldest if the buffer fills.
    pub fn push(&mut self, samples: &[f32]) {
        for &sample in samples {
            let index = (self.start + self.len) % self.capacity;
            if self.len == self.capacity {
                // Full: overwrite the oldest and advance past it.
                self.data[index] = sample;
                self.start = (self.start + 1) % self.capacity;
            } else {
                self.data[index] = sample;
                self.len += 1;
            }
        }
    }

    /// A copy of everything buffered, oldest first.
    pub fn snapshot(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.len);
        for offset in 0..self.len {
            out.push(self.data[(self.start + offset) % self.capacity]);
        }
        out
    }

    /// Copies out the oldest `count` samples and removes them.
    ///
    /// Returns fewer than `count` when the buffer holds less, rather than
    /// padding with silence — the caller sees exactly how much was available.
    pub fn take(&mut self, count: usize) -> Vec<f32> {
        let take = count.min(self.len);
        let mut out = Vec::with_capacity(take);
        for offset in 0..take {
            out.push(self.data[(self.start + offset) % self.capacity]);
        }
        self.start = (self.start + take) % self.capacity;
        self.len -= take;
        out
    }
}

/// Cuts a stream into fixed-size frames, keeping the remainder for next time.
///
/// VAD models take a fixed window, and a microphone callback does not arrive in
/// those sizes, so the leftover has to be carried forward rather than padded
/// into the last frame — padding every callback would insert silence that the
/// detector reads as the end of speech.
#[derive(Debug)]
pub struct Framer {
    pending: Vec<f32>,
    size: usize,
}

impl Framer {
    /// A framer producing windows of `size` samples.
    pub fn new(size: usize) -> Self {
        Self {
            pending: Vec::new(),
            size: size.max(1),
        }
    }

    /// The VAD framer: [`VAD_WINDOW`] at 16 kHz.
    pub fn for_vad() -> Self {
        Self::new(VAD_WINDOW)
    }

    /// How many samples are waiting for a full window.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Adds samples, returning every complete window they completed.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Vec<f32>> {
        self.pending.extend_from_slice(samples);

        let mut frames = Vec::new();
        while self.pending.len() >= self.size {
            frames.push(self.pending.drain(..self.size).collect());
        }
        frames
    }

    /// Returns whatever is left, padded with silence to a whole window.
    ///
    /// Only for the end of a recording, where the alternative is losing the last
    /// fraction of a word. Mid-stream it would be wrong — see this type's note.
    pub fn flush_padded(&mut self) -> Option<Vec<f32>> {
        if self.pending.is_empty() {
            return None;
        }
        let mut frame = std::mem::take(&mut self.pending);
        frame.resize(self.size, 0.0);
        Some(frame)
    }
}

/// Root-mean-square level of a frame, for a meter or a silence test.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- conversion ---------------------------------------------------------

    #[test]
    fn samples_round_trip_through_i16() {
        let original = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];
        let bytes = f32_to_i16_bytes(&original);
        let back = i16_bytes_to_f32(&bytes);

        assert_eq!(back.len(), original.len());
        for (a, b) in original.iter().zip(&back) {
            // Quantisation to 16 bits is the only loss.
            assert!((a - b).abs() < 0.001, "{a} became {b}");
        }
    }

    #[test]
    fn out_of_range_samples_are_clamped_not_wrapped() {
        // Full scale either way: wrapping would flip the sign and click.
        let bytes = f32_to_i16_bytes(&[2.0, -2.0]);
        let back = i16_bytes_to_f32(&bytes);
        assert!((back[0] - 1.0).abs() < 0.001, "positive overflow: {}", back[0]);
        assert!((back[1] + 1.0).abs() < 0.001, "negative overflow: {}", back[1]);
    }

    #[test]
    fn a_trailing_odd_byte_is_ignored() {
        let mut bytes = f32_to_i16_bytes(&[0.25]);
        bytes.push(0x7f);
        assert_eq!(i16_bytes_to_f32(&bytes).len(), 1);
    }

    #[test]
    fn empty_input_converts_to_empty_output() {
        assert!(f32_to_i16_bytes(&[]).is_empty());
        assert!(i16_bytes_to_f32(&[]).is_empty());
    }

    // -- resampling ---------------------------------------------------------

    #[test]
    fn audio_at_the_target_rate_is_untouched() {
        let samples = vec![0.1, 0.2, 0.3];
        assert_eq!(resample_to_16k(&samples, TARGET_RATE), samples);
    }

    #[test]
    fn decimation_divides_the_length_exactly() {
        let samples = vec![0.5f32; 4800];
        let out = resample_to_16k(&samples, 48_000);
        assert_eq!(out.len(), 1600, "4800 samples at 48k is 1600 at 16k");
        // A constant signal stays constant.
        assert!(out.iter().all(|s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn decimation_attenuates_aliasable_content_rather_than_preserving_it() {
        // Alternating +1/-1 at 48k is a 24 kHz signal — beyond the 16 kHz
        // output's 8 kHz limit, so it must not survive at full amplitude.
        //
        // A box average is a *weak* low-pass: it attenuates by roughly the
        // decimation factor, not to zero. For a factor of 3 that is 1/3
        // (-9.5 dB), which is enough for speech — whose energy above 8 kHz is
        // low, and whose capture hardware filters anyway — and is why this is
        // not a sinc filter. The claim worth testing is the comparison: naive
        // decimation keeps the tone at full amplitude, averaging cuts it.
        let samples: Vec<f32> = (0..4800)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();

        let averaged = resample_to_16k(&samples, 48_000);
        let averaged_peak = averaged.iter().fold(0.0f32, |acc, s| acc.max(s.abs()));

        // What the tempting one-liner would have produced.
        let naive: Vec<f32> = samples.iter().step_by(3).copied().collect();
        let naive_peak = naive.iter().fold(0.0f32, |acc, s| acc.max(s.abs()));

        assert!(
            (naive_peak - 1.0).abs() < 1e-6,
            "the naive path should preserve the tone, not attenuate it: {naive_peak}"
        );
        assert!(
            averaged_peak < naive_peak / 2.0,
            "averaging should cut the tone well below naive decimation: \
             {averaged_peak} against {naive_peak}"
        );
    }

    #[test]
    fn a_non_integer_ratio_interpolates() {
        // 44.1k → 16k is not a whole ratio.
        let samples: Vec<f32> = (0..4410).map(|i| i as f32 / 4410.0).collect();
        let out = resample_to_16k(&samples, 44_100);
        assert_eq!(out.len(), 1600, "4410 at 44.1k is 1600 at 16k");

        // A ramp stays monotonic, which it would not if indices were wrong.
        for pair in out.windows(2) {
            assert!(pair[1] >= pair[0], "the ramp went backwards");
        }
    }

    #[test]
    fn resampling_never_reads_past_the_input() {
        // One sample at a non-integer ratio is the case that would overrun.
        let out = resample_to_16k(&[0.7], 44_100);
        assert!(out.len() <= 1);
        assert!(out.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn resampling_an_empty_slice_is_harmless() {
        assert!(resample_to_16k(&[], 48_000).is_empty());
        // A zero rate must not divide by zero.
        assert!(resample_to_16k(&[0.1, 0.2], 0).is_empty());
    }

    // -- downmix ------------------------------------------------------------

    #[test]
    fn stereo_is_averaged_not_truncated() {
        // Out-of-phase channels would cancel entirely if summed; a device that
        // is simply louder on one side would lose half its signal if truncated.
        let interleaved = vec![1.0, 0.0, 0.0, 1.0];
        let mono = downmix(&interleaved, 2);
        assert_eq!(mono, vec![0.5, 0.5]);
    }

    #[test]
    fn mono_passes_through_unchanged() {
        let samples = vec![0.1, 0.2];
        assert_eq!(downmix(&samples, 1), samples);
        assert_eq!(downmix(&samples, 0), samples);
    }

    #[test]
    fn a_partial_frame_is_dropped_by_the_downmix() {
        // One stray sample cannot make a stereo frame.
        assert_eq!(downmix(&[1.0, 1.0, 0.5], 2).len(), 1);
    }

    // -- ring buffer --------------------------------------------------------

    #[test]
    fn a_ring_buffer_returns_what_was_pushed_in_order() {
        let mut ring = RingBuffer::new(8);
        ring.push(&[1.0, 2.0, 3.0]);
        assert_eq!(ring.len(), 3);
        assert_eq!(ring.free(), 5);
        assert_eq!(ring.snapshot(), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn a_ring_buffer_wraps_without_losing_order() {
        let mut ring = RingBuffer::new(4);
        ring.push(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        // Six samples into four slots: the oldest two are gone, order kept.
        assert_eq!(ring.len(), 4);
        assert_eq!(ring.snapshot(), vec![3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn taking_more_than_is_buffered_returns_only_what_exists() {
        let mut ring = RingBuffer::new(8);
        ring.push(&[1.0, 2.0]);
        let taken = ring.take(100);
        assert_eq!(taken, vec![1.0, 2.0], "must not pad with silence");
        assert!(ring.is_empty());
    }

    #[test]
    fn taking_leaves_the_rest_in_place() {
        let mut ring = RingBuffer::new(4);
        ring.push(&[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(ring.take(2), vec![1.0, 2.0]);
        assert_eq!(ring.snapshot(), vec![3.0, 4.0]);
        // And more can be pushed into the freed room, wrapping correctly.
        ring.push(&[5.0, 6.0]);
        assert_eq!(ring.snapshot(), vec![3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn clearing_resets_both_the_length_and_the_order() {
        let mut ring = RingBuffer::new(4);
        ring.push(&[1.0, 2.0, 3.0]);
        ring.clear();
        assert!(ring.is_empty());
        ring.push(&[9.0]);
        assert_eq!(ring.snapshot(), vec![9.0]);
    }

    #[test]
    fn a_zero_capacity_buffer_is_clamped_rather_than_panicking() {
        let mut ring = RingBuffer::new(0);
        ring.push(&[1.0, 2.0]);
        assert_eq!(ring.len(), 1);
        assert_eq!(ring.snapshot(), vec![2.0]);
    }

    #[test]
    fn a_seconds_buffer_is_named_in_samples() {
        let ring = RingBuffer::seconds(0.5);
        assert_eq!(ring.free(), 8_000);
    }

    // -- framing ------------------------------------------------------------

    #[test]
    fn a_framer_emits_exactly_the_windows_it_is_given() {
        let mut framer = Framer::new(4);
        let frames = framer.push(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        assert_eq!(frames, vec![vec![1.0, 2.0, 3.0, 4.0], vec![5.0, 6.0, 7.0, 8.0]]);
        assert_eq!(framer.pending(), 0);
    }

    #[test]
    fn a_framer_carries_the_remainder_forward() {
        // The important case: a callback smaller than a window must not be
        // padded, or every callback would look like a pause.
        let mut framer = Framer::new(4);
        assert!(framer.push(&[1.0, 2.0]).is_empty());
        assert_eq!(framer.pending(), 2);

        let frames = framer.push(&[3.0, 4.0, 5.0]);
        assert_eq!(frames, vec![vec![1.0, 2.0, 3.0, 4.0]]);
        assert_eq!(framer.pending(), 1);
    }

    #[test]
    fn a_framer_reassembles_across_many_small_pushes() {
        let mut framer = Framer::for_vad();
        let mut frames = Vec::new();
        // 1000 samples in chunks of 100 is one 512-sample window plus a
        // remainder, never two.
        for _ in 0..10 {
            frames.extend(framer.push(&vec![0.25f32; 100]));
        }
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].len(), VAD_WINDOW);
        assert_eq!(framer.pending(), 1000 - VAD_WINDOW);
    }

    #[test]
    fn flushing_pads_only_at_the_end() {
        let mut framer = Framer::new(4);
        framer.push(&[1.0, 2.0]);
        assert_eq!(framer.flush_padded(), Some(vec![1.0, 2.0, 0.0, 0.0]));
        // A second flush has nothing left to give.
        assert_eq!(framer.flush_padded(), None);
    }

    // -- level --------------------------------------------------------------

    #[test]
    fn rms_measures_loudness() {
        assert!((rms(&[1.0, -1.0, 1.0, -1.0]) - 1.0).abs() < 1e-6);
        assert_eq!(rms(&[0.0; 10]), 0.0);
        assert_eq!(rms(&[]), 0.0);
    }

    #[test]
    fn rms_ignores_sign() {
        assert!((rms(&[0.5, 0.5]) - rms(&[-0.5, -0.5])).abs() < 1e-6);
    }

    // -- the whole path -----------------------------------------------------

    #[test]
    fn a_microphone_chunk_becomes_a_vad_window() {
        // The real path: interleaved stereo at 48k, downmixed then resampled
        // then framed. 1536 interleaved samples is 768 frames, which is
        // 256 samples at 16k — less than one VAD window, so nothing is emitted
        // yet and the remainder is held.
        let mut interleaved = Vec::new();
        for _ in 0..768 {
            interleaved.push(0.3);
            interleaved.push(0.3);
        }

        let mono = downmix(&interleaved, 2);
        assert_eq!(mono.len(), 768);

        let resampled = resample_to_16k(&mono, 48_000);
        assert_eq!(resampled.len(), 256);

        let mut framer = Framer::for_vad();
        let frames = framer.push(&resampled);
        assert!(frames.is_empty(), "256 samples cannot fill a 512 window");
        assert_eq!(framer.pending(), 256);

        // A second chunk of the same size completes exactly one window.
        let frames = framer.push(&resampled);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].len(), VAD_WINDOW);
        assert!(frames[0].iter().all(|s| (s - 0.3).abs() < 0.01));
    }
}
