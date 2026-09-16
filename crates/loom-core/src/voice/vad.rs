//! Silero VAD: where the speech is, in a stream of audio.
//!
//! # Why the input is 576 samples and not 512
//!
//! This is the one thing about the model that cannot be guessed, and getting it
//! wrong produces a detector that *runs* and returns plausible numbers that mean
//! nothing. From Silero's own `OnnxWrapper`:
//!
//! ```python
//! context_size = 64 if sr == 16000 else 32
//! x = torch.cat([self._context, x], dim=1)   # 64 + 512 = 576
//! ...
//! self._context = x[..., -context_size:]     # carried into the next call
//! ```
//!
//! Every window is prefixed with the **tail of the previous one**, so the model
//! sees 576 samples and consecutive frames overlap by 64. Passing 512 alone —
//! which the graph accepts, because its input is declared `[?, ?]` — reads as a
//! detector that returns 0.003 on obvious speech and 0.0005 on silence: it is
//! working, on the wrong input.
//!
//! So a [`Vad`] owns two pieces of state: the 256-float recurrent `state`, and a
//! 64-float `context`. Both must be threaded, and both must be reset between
//! utterances.
//!
//! # Endpointing
//!
//! [`segments`] turns a probability sequence into speech spans, following the
//! reference's own thresholds and hysteresis: [`SPEECH_THRESHOLD`] to start,
//! [`SILENCE_THRESHOLD`] to stop, [`MIN_SILENCE_MS`] of quiet before a span
//! closes, and [`MIN_SPEECH_MS`] before a span is worth keeping. The two
//! thresholds are what stop a span flickering at a single value.

use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;

use super::audio;
use crate::{Error, Result};

/// Samples per call at 16 kHz. Fixed by the model.
pub const WINDOW: usize = 512;
/// Samples carried from the end of one window into the start of the next.
pub const CONTEXT: usize = 64;
/// The recurrent state: two layers, one batch, 128 wide.
pub const STATE_SHAPE: [i64; 3] = [2, 1, 128];

/// Probability at or above which speech starts, or continues.
pub const SPEECH_THRESHOLD: f32 = 0.5;
/// Probability below which speech may end.
///
/// Deliberately lower than [`SPEECH_THRESHOLD`] — the reference uses
/// `threshold - 0.15`. A single threshold makes a span flap on and off as the
/// probability wobbles either side of it, which reads as many short utterances
/// rather than one.
pub const SILENCE_THRESHOLD: f32 = 0.35;
/// Quiet for this long closes a span.
pub const MIN_SILENCE_MS: u32 = 100;
/// Spans shorter than this are discarded as noise.
pub const MIN_SPEECH_MS: u32 = 250;
/// Padding added to each end of a span.
pub const PAD_MS: u32 = 30;

/// One stretch of speech, in samples at 16 kHz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub start: usize,
    pub end: usize,
}

impl Segment {
    /// Length in samples.
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    /// Length in seconds.
    pub fn seconds(&self) -> f32 {
        self.len() as f32 / audio::TARGET_RATE as f32
    }
}

/// A loaded voice-activity detector.
///
/// Holds state, so it is not `Sync` and should be owned by whatever is reading
/// the microphone. [`reset`](Self::reset) between utterances is required, not
/// optional: the recurrent state carries the spectral history of the previous
/// audio, and a detector started mid-thought reports the first window as speech.
pub struct Vad {
    session: Session,
    state: Vec<f32>,
    context: Vec<f32>,
}

impl std::fmt::Debug for Vad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vad")
            .field("window", &WINDOW)
            .field("context", &CONTEXT)
            .finish_non_exhaustive()
    }
}

impl Vad {
    /// Loads the detector.
    ///
    /// The ONNX runtime must already be initialized — `Kokoro::load` or
    /// `Whisper::load` does it, and `ort` memoizes, so any of them first is
    /// fine.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(Error::Http(format!(
                "the voice-activity model is missing at {}. Run scripts/fetch-whisper.py.",
                path.display()
            )));
        }

        let session = Session::builder()
            .map_err(|e| Error::Http(format!("could not create a VAD session: {e}")))?
            .commit_from_file(path)
            .map_err(|e| Error::Http(format!("could not load the VAD: {e}")))?;

        // The names are the ones the export declares; checked at load so a
        // different export fails here rather than mid-stream.
        for name in ["input", "state", "sr"] {
            if !session.inputs().iter().any(|slot| slot.name() == name) {
                let found: Vec<&str> = session
                    .inputs()
                    .iter()
                    .map(|slot| slot.name())
                    .collect();
                return Err(Error::Http(format!(
                    "the VAD has no input called {name:?}; it has {found:?}"
                )));
            }
        }

        let mut vad = Self {
            session,
            state: vec![0.0; STATE_SHAPE.iter().product::<i64>() as usize],
            context: vec![0.0; CONTEXT],
        };
        vad.reset();
        Ok(vad)
    }

    /// Clears the carried state. Call before a new utterance.
    pub fn reset(&mut self) {
        self.state.fill(0.0);
        self.context.fill(0.0);
    }

    /// The speech probability for one 512-sample window at 16 kHz.
    ///
    /// The window must be exactly [`WINDOW`] samples. A shorter one is rejected
    /// rather than padded: padding silently would make a truncated stream look
    /// like it had trailing silence, which is the one thing that closes a span.
    pub fn probability(&mut self, window: &[f32]) -> Result<f32> {
        if window.len() != WINDOW {
            return Err(Error::Http(format!(
                "the VAD takes {WINDOW}-sample windows, got {}",
                window.len()
            )));
        }

        // The concatenation the reference performs: the carried tail, then the
        // new window.
        let mut input = Vec::with_capacity(CONTEXT + WINDOW);
        input.extend_from_slice(&self.context);
        input.extend_from_slice(window);

        let features = Tensor::from_array(([1i64, input.len() as i64], input.clone()))
            .map_err(|e| Error::Http(format!("could not build the VAD input: {e}")))?;
        let state = Tensor::from_array((STATE_SHAPE.to_vec(), self.state.clone()))
            .map_err(|e| Error::Http(format!("could not build the VAD state: {e}")))?;

        // The graph declares `sr` as a scalar, and the reference passes a 0-d
        // array. An empty shape here is what matches.
        let sample_rate = Tensor::from_array((Vec::<i64>::new(), vec![audio::TARGET_RATE as i64]))
            .map_err(|e| Error::Http(format!("could not build the sample rate: {e}")))?;

        let outputs = self
            .session
            .run(ort::inputs![
                "input" => features,
                "state" => state,
                "sr" => sample_rate,
            ])
            .map_err(|e| Error::Http(format!("the VAD failed: {e}")))?;

        let probability = outputs["output"]
            .try_extract_tensor::<f32>()
            .map_err(|e| Error::Http(format!("could not read the VAD output: {e}")))?
            .1
            .first()
            .copied()
            .unwrap_or(0.0);

        // Carry the state and the tail of the *concatenated* input — which for a
        // 512-sample window is the last 64 samples of the window itself.
        if let Ok((_, next)) = outputs["stateN"].try_extract_tensor::<f32>() {
            if next.len() == self.state.len() {
                self.state.copy_from_slice(next);
            }
        }
        let tail = &input[input.len() - CONTEXT..];
        self.context.copy_from_slice(tail);

        Ok(probability)
    }

    /// The probability for every full window of `audio`.
    ///
    /// Audio must already be mono at 16 kHz; anything after the last whole
    /// window is dropped, matching how the reference pads only the final partial
    /// chunk of a whole file rather than mid-stream.
    pub fn probabilities(&mut self, audio: &[f32]) -> Result<Vec<f32>> {
        let mut framer = audio::Framer::for_vad();
        let frames = framer.push(audio);

        let mut out = Vec::with_capacity(frames.len());
        for frame in &frames {
            out.push(self.probability(frame)?);
        }
        Ok(out)
    }

    /// Transcribes a whole clip into speech spans.
    ///
    /// Resets first, so a caller can call it repeatedly without the previous
    /// clip's history leaking in.
    pub fn segments(&mut self, audio: &[f32]) -> Result<Vec<Segment>> {
        self.reset();
        let probabilities = self.probabilities(audio)?;
        Ok(segments(&probabilities, audio::TARGET_RATE))
    }
}

/// Turns a probability sequence into speech spans.
///
/// Pure, so it can be tested against hand-written sequences where the expected
/// answer is obvious. `sample_rate` is only used to convert the millisecond
/// constants into samples.
pub fn segments(probabilities: &[f32], sample_rate: u32) -> Vec<Segment> {
    let per_window = |ms: u32| (sample_rate as u64 * ms as u64 / 1000) as usize;

    let min_speech = per_window(MIN_SPEECH_MS);
    let min_silence = per_window(MIN_SILENCE_MS);
    let pad = per_window(PAD_MS);

    let mut segments: Vec<Segment> = Vec::new();
    let mut start: Option<usize> = None;
    // Where the current quiet run began, if the detector is inside a span.
    let mut quiet_since: Option<usize> = None;

    for (index, probability) in probabilities.iter().enumerate() {
        let at = index * WINDOW;

        if *probability >= SPEECH_THRESHOLD {
            // Speech resumes: any quiet run was too short to end the span.
            quiet_since = None;
            if start.is_none() {
                start = Some(at);
            }
        } else {
            if start.is_none() {
                continue;
            }
            if *probability < SILENCE_THRESHOLD {
                let since = *quiet_since.get_or_insert(at);
                if at.saturating_sub(since) >= min_silence {
                    let end = since;
                    if end.saturating_sub(start.unwrap_or(end)) >= min_speech {
                        segments.push(Segment {
                            start: start.unwrap_or(end),
                            end,
                        });
                    }
                    start = None;
                    quiet_since = None;
                }
            }
            // Between the two thresholds while inside a span: neither speech nor
            // quiet enough to close it. That gap is the hysteresis.
        }
    }

    // A span still open at the end runs to the last window. The reference does
    // the same, and it is the right behaviour for a stream that was still being
    // spoken when the audio ran out.
    if let Some(from) = start {
        let end = probabilities.len() * WINDOW;
        if end.saturating_sub(from) >= min_speech {
            segments.push(Segment { start: from, end });
        }
    }

    // Padding, clamped to the audio and then re-checked: a span that padding
    // merges with its neighbour is left as two, which is a limitation rather
    // than a bug — merging would need a second pass.
    let total = probabilities.len() * WINDOW;
    segments
        .into_iter()
        .map(|segment| Segment {
            start: segment.start.saturating_sub(pad),
            end: (segment.end + pad).min(total),
        })
        .filter(|segment| !segment.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A probability sequence of `windows` values.
    fn probs(windows: usize, value: f32) -> Vec<f32> {
        vec![value; windows]
    }

    /// Builds a sequence from run-length pairs: `runs(10, 0.9, 5, 0.1)` is ten
    /// speech windows then five silent ones.
    fn runs(spec: &[(usize, f32)]) -> Vec<f32> {
        spec.iter()
            .flat_map(|(count, value)| vec![*value; *count])
            .collect()
    }

    // -- constants ----------------------------------------------------------

    #[test]
    fn the_window_and_context_match_the_model() {
        // From Silero's own wrapper: 512 samples at 16 kHz, with a 64-sample
        // context prepended. Getting either wrong gives a detector that runs and
        // means nothing.
        assert_eq!(WINDOW, 512);
        assert_eq!(CONTEXT, 64);
        assert_eq!(STATE_SHAPE, [2, 1, 128]);
        // The concatenated input is what the graph actually sees.
        assert_eq!(CONTEXT + WINDOW, 576);
    }

    #[test]
    fn the_thresholds_have_the_hysteresis_the_reference_uses() {
        assert_eq!(SPEECH_THRESHOLD, 0.5);
        // threshold - 0.15, which is what stops a span flickering.
        assert!((SPEECH_THRESHOLD - SILENCE_THRESHOLD - 0.15).abs() < 1e-6);
    }

    // -- the segmenter ------------------------------------------------------

    #[test]
    fn a_single_run_of_speech_is_one_segment() {
        // Two seconds of speech at 16 kHz is about 62 windows.
        let probabilities = runs(&[(10, 0.1), (62, 0.9), (30, 0.05)]);
        let found = segments(&probabilities, audio::TARGET_RATE);
        assert_eq!(found.len(), 1, "{found:?}");

        // The start sits near window 10, plus or minus the padding.
        let expected_start = 10 * WINDOW;
        assert!(
            found[0].start <= expected_start,
            "the padded start {} should not be after the speech's {}",
            found[0].start,
            expected_start
        );
        assert!(found[0].seconds() > 1.5, "only {:.2} s", found[0].seconds());
    }

    #[test]
    fn two_runs_separated_by_silence_are_two_segments() {
        // The gap must exceed MIN_SILENCE_MS to split: 100 ms is 3 windows at
        // 16 kHz, so 10 windows is comfortably enough.
        let probabilities = runs(&[
            (5, 0.1),
            (30, 0.9),
            (10, 0.05),
            (30, 0.9),
            (20, 0.05),
        ]);
        let found = segments(&probabilities, audio::TARGET_RATE);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[1].start > found[0].end || found[1].start >= found[0].start);
    }

    #[test]
    fn a_gap_shorter_than_the_minimum_silence_does_not_split() {
        // One quiet window is 32 ms, well under the 100 ms needed. A detector
        // without hysteresis reports two utterances here.
        let probabilities = runs(&[(30, 0.9), (1, 0.2), (30, 0.9), (20, 0.05)]);
        let found = segments(&probabilities, audio::TARGET_RATE);
        assert_eq!(found.len(), 1, "a 32 ms gap split the span: {found:?}");
    }

    #[test]
    fn a_span_between_the_thresholds_does_not_close() {
        // Values in [SILENCE_THRESHOLD, SPEECH_THRESHOLD) are neither speech nor
        // quiet enough to end — the hysteresis band.
        let probabilities = runs(&[(30, 0.9), (10, 0.4), (30, 0.9), (20, 0.05)]);
        let found = segments(&probabilities, audio::TARGET_RATE);
        assert_eq!(found.len(), 1, "the hysteresis band split the span: {found:?}");
    }

    #[test]
    fn a_very_short_burst_is_discarded() {
        // Under MIN_SPEECH_MS, so a click rather than an utterance.
        let probabilities = runs(&[(20, 0.05), (2, 0.9), (30, 0.05)]);
        let found = segments(&probabilities, audio::TARGET_RATE);
        assert!(found.is_empty(), "a 64 ms burst became {found:?}");
    }

    #[test]
    fn silence_alone_produces_nothing() {
        assert!(segments(&probs(100, 0.0), audio::TARGET_RATE).is_empty());
        assert!(segments(&probs(100, 0.2), audio::TARGET_RATE).is_empty());
        assert!(segments(&[], audio::TARGET_RATE).is_empty());
    }

    #[test]
    fn an_utterance_still_open_at_the_end_is_kept() {
        // A stream cut off mid-sentence: the span runs to the last window rather
        // than being dropped, which matters because that is exactly the audio a
        // live microphone delivers.
        let probabilities = runs(&[(10, 0.05), (60, 0.9)]);
        let found = segments(&probabilities, audio::TARGET_RATE);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].end, probabilities.len() * WINDOW);
    }

    #[test]
    fn segments_are_padded_and_never_exceed_the_audio() {
        let probabilities = runs(&[(2, 0.9), (100, 0.05)]);
        let found = segments(&probabilities, audio::TARGET_RATE);
        let total = probabilities.len() * WINDOW;
        for segment in &found {
            assert!(segment.end <= total, "{segment:?} runs past {total}");
            assert!(segment.start < segment.end);
        }
    }

    #[test]
    fn a_segment_never_runs_backwards() {
        // Randomly-shaped input, to catch an arithmetic slip in the padding or
        // the clamping that a tidy fixture would miss.
        let mut state = 0x1234_5678u32;
        let mut next = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 16) as f32 / 65535.0
        };
        let probabilities: Vec<f32> = (0..500).map(|_| next()).collect();

        for segment in segments(&probabilities, audio::TARGET_RATE) {
            assert!(segment.start < segment.end, "{segment:?}");
            assert!(segment.end <= 500 * WINDOW);
        }
    }

    #[test]
    fn segment_length_helpers_agree() {
        let segment = Segment {
            start: 100,
            end: 16_100,
        };
        assert_eq!(segment.len(), 16_000);
        assert!(!segment.is_empty());
        assert!((segment.seconds() - 1.0).abs() < 1e-6);

        let empty = Segment { start: 5, end: 5 };
        assert!(empty.is_empty());
        assert_eq!(empty.seconds(), 0.0);
    }

    // -- against the real model --------------------------------------------

    fn model_path() -> Option<std::path::PathBuf> {
        let path = dirs::home_dir()?.join(".loom/voice/silero_vad.onnx");
        path.exists().then_some(path)
    }

    fn runtime_path() -> Option<std::path::PathBuf> {
        let path = dirs::home_dir()?.join(".loom/ort/onnxruntime.dll");
        path.exists().then_some(path)
    }

    fn kokoro_audio() -> Option<Vec<f32>> {
        let wav = std::env::temp_dir().join("loom-kokoro.wav");
        let bytes = std::fs::read(&wav).ok()?;
        let (samples, rate, channels) = read_wav(&bytes)?;
        let mono = audio::downmix(&samples, channels);
        Some(audio::resample_to_16k(&mono, rate))
    }

    #[test]
    fn real_speech_reads_as_speech() {
        let _guard = crate::voice::model_lock();

        let (Some(model), Some(runtime)) = (model_path(), runtime_path()) else {
            return;
        };
        if kokoro_audio().is_none() {
            return;
        }
        let _ = ort::init_from(&runtime).map(|builder| builder.commit());

        let mut vad = Vad::load(&model).expect("the VAD should load");
        let audio = kokoro_audio().expect("checked above");

        let probabilities = vad.probabilities(&audio).expect("the VAD should run");
        assert!(!probabilities.is_empty(), "no windows were produced");

        let peak = probabilities.iter().cloned().fold(0.0f32, f32::max);
        let speech = probabilities.iter().filter(|p| **p >= SPEECH_THRESHOLD).count();
        let ratio = speech as f32 / probabilities.len() as f32;

        // This is the assertion the whole module exists for. Before the context
        // was threaded, this same audio peaked at 0.003 and read as 0% speech —
        // while Whisper transcribed it at 100% word accuracy. So a threshold
        // here is not arbitrary: it is the difference between a working detector
        // and a plausible-looking broken one.
        assert!(
            ratio > 0.5,
            "only {:.0}% of {}-11 s of continuous speech read as speech (peak {peak:.4})",
            ratio * 100.0,
            probabilities.len()
        );
        assert!(peak > 0.9, "peak probability was only {peak:.4}");
    }

    #[test]
    fn silence_reads_as_silence() {
        let _guard = crate::voice::model_lock();

        let (Some(model), Some(runtime)) = (model_path(), runtime_path()) else {
            return;
        };
        let _ = ort::init_from(&runtime).map(|builder| builder.commit());

        let mut vad = Vad::load(&model).expect("the VAD should load");

        // Five seconds of digital silence.
        let quiet = vec![0.0f32; audio::TARGET_RATE as usize * 5];
        let probabilities = vad.probabilities(&quiet).expect("should run");

        let speech = probabilities.iter().filter(|p| **p >= SPEECH_THRESHOLD).count();
        assert_eq!(speech, 0, "{speech} silent windows read as speech");

        let peak = probabilities.iter().cloned().fold(0.0f32, f32::max);
        assert!(peak < 0.2, "silence peaked at {peak:.4}");
    }

    #[test]
    fn the_state_carries_history_across_calls() {
        let _guard = crate::voice::model_lock();

        let (Some(model), Some(runtime)) = (model_path(), runtime_path()) else {
            return;
        };
        let _ = ort::init_from(&runtime).map(|builder| builder.commit());

        let mut vad = Vad::load(&model).expect("the VAD should load");

        // A tone, so each window is identical in content but not in history.
        let mut window = vec![0.0f32; WINDOW];
        for (index, sample) in window.iter_mut().enumerate() {
            let t = index as f32 / audio::TARGET_RATE as f32;
            *sample = 0.1 * (2.0 * std::f32::consts::PI * 300.0 * t).sin();
        }

        let first = vad.probability(&window).expect("should run");
        for _ in 0..4 {
            vad.probability(&window).expect("should run");
        }
        let fifth = vad.probability(&window).expect("should run");

        // Identical input through a stateful model must *not* give identical
        // output. If it does, the state is not being threaded and the detector
        // has no memory — which is the failure that looks like it works.
        assert!(
            (first - fifth).abs() > 1e-6,
            "five identical windows all gave {first:.6}, so the state is not threaded"
        );

        // And a reset clears that history.
        vad.reset();
        let after_reset = vad.probability(&window).expect("should run");
        assert!(
            (after_reset - first).abs() < 1e-6,
            "reset did not restore the initial condition: {after_reset:.6} vs {first:.6}"
        );
    }

    #[test]
    fn a_window_of_the_wrong_size_is_rejected() {
        let _guard = crate::voice::model_lock();

        let (Some(model), Some(runtime)) = (model_path(), runtime_path()) else {
            return;
        };
        let _ = ort::init_from(&runtime).map(|builder| builder.commit());

        let mut vad = Vad::load(&model).expect("the VAD should load");
        let error = vad.probability(&[0.0; 256]).unwrap_err();
        assert!(error.to_string().contains("512"), "{error}");
    }

    /// Minimal PCM WAV reader, for the fixture.
    fn read_wav(bytes: &[u8]) -> Option<(Vec<f32>, u32, usize)> {
        if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return None;
        }
        let channels = u16::from_le_bytes([bytes[22], bytes[23]]) as usize;
        let rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
        let bits = u16::from_le_bytes([bytes[34], bytes[35]]);
        if bits != 16 || channels == 0 {
            return None;
        }
        let mut offset = 12usize;
        while offset + 8 <= bytes.len() {
            let id = &bytes[offset..offset + 4];
            let size = u32::from_le_bytes([
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ]) as usize;
            if id == b"data" {
                let start = offset + 8;
                let end = (start + size).min(bytes.len());
                return Some((audio::i16_bytes_to_f32(&bytes[start..end]), rate, channels));
            }
            offset += 8 + size + (size % 2);
        }
        None
    }
}
