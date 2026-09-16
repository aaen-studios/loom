//! Live endpointing: deciding when an utterance began, and when it ended.
//!
//! Everything above this is batch. [`Vad::segments`](super::vad::Vad::segments)
//! takes a whole recording and returns every span in it, which is the right
//! shape for a file and the wrong one for a microphone: there is no "whole
//! recording" while someone is still speaking, and a caller holding audio until
//! they stop would have to keep the entire conversation in memory.
//!
//! So this is the streaming counterpart. Audio arrives in blocks, and a
//! [`Listener`] reports what changed: an utterance started, it is still going,
//! or it finished and here it is.
//!
//! # Why the state machine is here and not in the Tauri layer
//!
//! Endpointing is the part that is *wrong* rather than *broken* when it fails:
//! an off-by-one in the quiet counter gives an utterance that ends a window too
//! early, and the only symptom is a truncated word in a transcript. That is the
//! kind of bug that has to be testable against real audio, so it lives in
//! `loom-core` rather than in the command layer.
//!
//! # The detector is a trait
//!
//! [`Detector`] is what the listener calls to score a window. In production that
//! is [`Vad`], which is a 2 MB recurrent model; in tests it is a stub that reads
//! a script. Without that seam the state machine could only be tested through
//! the model, which means testing it against audio whose probability sequence
//! nobody knows — so the interesting cases (a brief click, a gap just short of
//! the threshold, an utterance cut off mid-word) would be unreachable.

use super::audio;
use super::vad::{
    Vad, MIN_SILENCE_MS, MIN_SPEECH_MS, SILENCE_THRESHOLD, SPEECH_THRESHOLD, WINDOW,
};
use crate::Result;

/// The longest utterance to hold before transcribing it anyway.
///
/// Whisper's receptive field is 30 seconds, so anything past that cannot be
/// transcribed in one pass. Rather than silently dropping audio or truncating a
/// sentence, the listener closes the utterance and reports it — the caller gets
/// a long transcript instead of no transcript.
pub const MAX_UTTERANCE_SECONDS: f32 = 28.0;

/// What a listener reports after being given audio.
#[derive(Debug, Clone, PartialEq)]
pub enum Listening {
    /// Nothing changed: no speech, or speech that is still going.
    Nothing,
    /// An utterance just began. `samples` is the audio so far, which is the
    /// window that triggered it — worth having so a caller can show a level
    /// before there is anything to transcribe.
    Started { samples: Vec<f32> },
    /// An utterance finished. This is what gets transcribed.
    Finished { samples: Vec<f32> },
    /// The utterance hit [`MAX_UTTERANCE_SECONDS`] and was cut short.
    ///
    /// Reported separately from [`Finished`](Self::Finished) because it means
    /// the audio may end mid-word, and a caller that wants a complete sentence
    /// should say so rather than presenting a truncation as a result.
    Truncated { samples: Vec<f32> },
}

impl Listening {
    /// The audio this report carries, if any.
    pub fn samples(&self) -> Option<&[f32]> {
        match self {
            Listening::Nothing => None,
            Listening::Started { samples }
            | Listening::Finished { samples }
            | Listening::Truncated { samples } => Some(samples),
        }
    }
}

/// Scores one window of audio.
///
/// Implemented by [`Vad`] in production and by a stub in tests. The name is
/// deliberately about the job rather than the model, so a different detector can
/// replace Silero without the listener knowing.
pub trait Detector {
    /// Speech probability for one [`WINDOW`]-sample block at 16 kHz.
    fn probability(&mut self, window: &[f32]) -> Result<f32>;

    /// Clears carried state. Called between utterances.
    fn reset(&mut self);
}

impl Detector for Vad {
    fn probability(&mut self, window: &[f32]) -> Result<f32> {
        Vad::probability(self, window)
    }

    fn reset(&mut self) {
        Vad::reset(self)
    }
}

/// Where the state machine is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Waiting for the first speech window.
    Idle,
    /// Inside an utterance.
    Speaking,
}

/// Turns a stream of audio blocks into utterances.
///
/// Generic over the detector so the state machine can be tested without a model.
pub struct Listener<D: Detector> {
    detector: D,
    framer: audio::Framer,
    phase: Phase,
    /// The utterance being built, including any trailing silence that has not
    /// yet closed it.
    utterance: Vec<f32>,
    /// Consecutive quiet windows, which is what closes an utterance.
    quiet_windows: usize,
    /// Windows counted towards the minimum speech length check.
    speech_windows: usize,
    /// A short burst that has not yet qualified.
    ///
    /// Held separately from `utterance` so a click that never becomes speech
    /// does not contribute to an utterance's length. Without this a 60 ms click
    /// followed by real speech would count as the start of the utterance, and
    /// endpointing would include the click's leading silence.
    pending: Vec<f32>,
    /// Windows of quiet needed to close, derived from the sample rate.
    min_quiet_windows: usize,
    /// Windows of speech needed to qualify, derived from the sample rate.
    min_speech_windows: usize,
    /// Windows in [`MAX_UTTERANCE_SECONDS`].
    max_windows: usize,
}

impl<D: Detector> Listener<D> {
    /// A listener using `detector`, with the reference's thresholds.
    pub fn new(detector: D) -> Self {
        // Milliseconds to windows. The division by 1000 is the whole point of
        // the function: without it `ms` is a sample count, and "100 ms of
        // silence" becomes 3125 windows — a listener that never closes an
        // utterance, with no error anywhere to point at it.
        let windows_per_ms = |ms: u32| {
            let samples = audio::TARGET_RATE as usize * ms as usize / 1000;
            // Ceiling division: a gap of exactly the minimum must close the
            // utterance, not fall one window short of it.
            samples.div_ceil(WINDOW).max(1)
        };

        Self {
            detector,
            framer: audio::Framer::for_vad(),
            phase: Phase::Idle,
            utterance: Vec::new(),
            quiet_windows: 0,
            speech_windows: 0,
            pending: Vec::new(),
            min_quiet_windows: windows_per_ms(MIN_SILENCE_MS),
            min_speech_windows: windows_per_ms(MIN_SPEECH_MS),
            max_windows: (MAX_UTTERANCE_SECONDS * audio::TARGET_RATE as f32 / WINDOW as f32)
                .ceil() as usize,
        }
    }

    /// Clears everything, including the detector's carried state.
    pub fn reset(&mut self) {
        self.detector.reset();
        self.framer = audio::Framer::for_vad();
        self.phase = Phase::Idle;
        self.utterance.clear();
        self.pending.clear();
        self.quiet_windows = 0;
        self.speech_windows = 0;
    }

    /// Whether an utterance is currently open.
    pub fn is_speaking(&self) -> bool {
        self.phase == Phase::Speaking
    }

    /// How much audio the open utterance holds, in seconds.
    pub fn pending_seconds(&self) -> f32 {
        (self.utterance.len() + self.pending.len()) as f32 / audio::TARGET_RATE as f32
    }

    /// Feeds audio that is already mono at 16 kHz.
    ///
    /// Returns at most one report per call even when several windows complete: a
    /// block large enough to contain both the start and the end of an utterance
    /// is rare from a microphone, and collapsing avoids a caller having to
    /// handle a start and an end in the same return value.
    pub fn push(&mut self, samples: &[f32]) -> Result<Listening> {
        let frames = self.framer.push(samples);
        let mut report = Listening::Nothing;

        for frame in frames {
            let probability = self.detector.probability(&frame)?;
            let next = self.advance(&frame, probability)?;
            // Later reports win: a `Finished` in the same block as a `Started`
            // is the more useful of the two.
            if !matches!(next, Listening::Nothing) {
                report = next;
            }
        }

        Ok(report)
    }

    /// Feeds audio at any rate and channel count.
    ///
    /// Resampling here rather than at the call site means a caller cannot forget
    /// it, and forgetting it is silent: 48 kHz audio treated as 16 kHz is a
    /// three-times-speed recording that the VAD still scores.
    pub fn push_audio(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        channels: usize,
    ) -> Result<Listening> {
        let mono = audio::downmix(samples, channels);
        let resampled = audio::resample_to_16k(&mono, sample_rate);
        self.push(&resampled)
    }

    /// Closes any open utterance, for a stream that has ended.
    ///
    /// A caller stopping the microphone mid-sentence gets that sentence rather
    /// than losing it, which is what a user pressing "stop" expects.
    pub fn finish(&mut self) -> Result<Option<Listening>> {
        // A partial window is padded: at the end of a stream that is the only
        // option, and a padded tail cannot close an utterance that was not
        // already closing.
        if let Some(frame) = self.framer.flush_padded() {
            let probability = self.detector.probability(&frame)?;
            self.advance(&frame, probability)?;
        }

        match self.phase {
            Phase::Idle => Ok(None),
            Phase::Speaking => {
                let samples = self.take_utterance();
                if samples.len() < self.min_speech_samples() {
                    return Ok(None);
                }
                Ok(Some(Listening::Finished { samples }))
            }
        }
    }

    /// Samples needed for an utterance to be worth keeping.
    fn min_speech_samples(&self) -> usize {
        self.min_speech_windows * WINDOW
    }

    /// The state machine for one window. Returns a report when one is due.
    fn advance(&mut self, frame: &[f32], probability: f32) -> Result<Listening> {
        match self.phase {
            Phase::Idle => {
                if probability >= SPEECH_THRESHOLD {
                    self.pending.extend_from_slice(frame);
                    self.speech_windows += 1;
                    self.quiet_windows = 0;

                    if self.speech_windows >= self.min_speech_windows {
                        // Qualified: what was pending becomes the utterance.
                        self.utterance.append(&mut self.pending);
                        self.phase = Phase::Speaking;
                        return Ok(Listening::Started {
                            samples: self.utterance.clone(),
                        });
                    }
                } else if !self.pending.is_empty() {
                    // A burst is only abandoned after the *full* minimum
                    // silence, not on the first sub-threshold window. One
                    // window below the threshold in the middle of a word is
                    // ordinary — the detector dips — and discarding the pending
                    // audio on it means an utterance that never qualifies no
                    // matter how long someone talks. Measured on real speech,
                    // the strict version never opened an utterance at all.
                    self.pending.extend_from_slice(frame);

                    if probability < SILENCE_THRESHOLD {
                        self.quiet_windows += 1;
                        if self.quiet_windows >= self.min_quiet_windows {
                            self.pending.clear();
                            self.speech_windows = 0;
                            self.quiet_windows = 0;
                        }
                    }
                    // In the hysteresis band: keep waiting without counting
                    // quiet, exactly as the Speaking phase does.
                }
                Ok(Listening::Nothing)
            }

            Phase::Speaking => {
                self.utterance.extend_from_slice(frame);

                if probability >= SPEECH_THRESHOLD {
                    // Speech resumed: the quiet run was too short to count.
                    self.quiet_windows = 0;
                } else if probability < SILENCE_THRESHOLD {
                    self.quiet_windows += 1;
                }
                // Between the thresholds: neither speech nor quiet enough to
                // close. That gap is the hysteresis.

                if self.utterance.len() / WINDOW >= self.max_windows {
                    let samples = self.take_utterance();
                    return Ok(Listening::Truncated { samples });
                }

                if self.quiet_windows >= self.min_quiet_windows {
                    let samples = self.take_utterance();
                    return Ok(Listening::Finished { samples });
                }

                Ok(Listening::Nothing)
            }
        }
    }

    /// Removes the open utterance, trailing silence trimmed, and resets.
    fn take_utterance(&mut self) -> Vec<f32> {
        let raw = std::mem::take(&mut self.utterance);
        self.phase = Phase::Idle;
        self.pending.clear();
        self.quiet_windows = 0;
        self.speech_windows = 0;
        trim_trailing_silence(&raw, SILENCE_THRESHOLD)
    }
}

/// Drops the quiet tail an utterance accumulated while it was closing.
///
/// The quiet windows are part of the utterance because the hysteresis needs
/// them, but sending them to a recogniser adds nothing and Whisper will happily
/// hallucinate a word into a long silence. Only the tail is trimmed: leading
/// silence is already excluded because the utterance starts at the first
/// qualifying window.
fn trim_trailing_silence(samples: &[f32], threshold: f32) -> Vec<f32> {
    // Silence here means quiet *and* low amplitude — a window can be scored as
    // silence by the detector while still holding a fade-out worth keeping, so
    // the amplitude check is what decides.
    let amplitude = threshold * 0.1;
    let mut end = samples.len();
    while end > 0 && samples[end - 1].abs() < amplitude {
        end -= 1;
    }
    // Do not trim into nothing.
    if end < WINDOW {
        return samples.to_vec();
    }
    samples[..end].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A detector that reads a script instead of scoring audio.
    ///
    /// This is the seam that makes the state machine testable. A real VAD cannot
    /// be told "return 0.9 for two windows then 0.1 forever" — it scores audio,
    /// and the audio that produces a given probability sequence is not something
    /// a test can construct.
    struct Scripted {
        /// Probabilities to return, consumed in order. The last one repeats.
        script: Vec<f32>,
        position: usize,
        resets: usize,
    }

    impl Scripted {
        fn new(script: Vec<f32>) -> Self {
            assert!(!script.is_empty());
            Self {
                script,
                position: 0,
                resets: 0,
            }
        }
    }

    impl Detector for Scripted {
        fn probability(&mut self, _window: &[f32]) -> Result<f32> {
            let value = self
                .script
                .get(self.position)
                .copied()
                .unwrap_or_else(|| *self.script.last().expect("non-empty"));
            self.position += 1;
            Ok(value)
        }

        fn reset(&mut self) {
            self.position = 0;
            self.resets += 1;
        }
    }

    /// One window of audio with a given amplitude, so the trailing-silence
    /// trim has something to work with.
    fn window(amplitude: f32) -> Vec<f32> {
        vec![amplitude; WINDOW]
    }

    /// Pushes `count` windows, collecting whatever reports come back.
    fn feed<D: Detector>(listener: &mut Listener<D>, blocks: &[Vec<f32>]) -> Vec<Listening> {
        let mut reports = Vec::new();
        for block in blocks {
            let report = listener.push(block).expect("the stub cannot fail");
            if !matches!(report, Listening::Nothing) {
                reports.push(report);
            }
        }
        reports
    }

    // -- constants ----------------------------------------------------------

    #[test]
    fn the_window_budget_matches_the_other_modules() {
        // Derived from the shared constants rather than restated, so the
        // listener and `vad::segments` cannot disagree about what "100 ms of
        // silence" means.
        let listener = Listener::new(Scripted::new(vec![0.0]));
        assert_eq!(listener.min_quiet_windows, 4, "100 ms is 3.125 windows");
        assert_eq!(listener.min_speech_windows, 8, "250 ms is 7.8 windows");
        assert_eq!(listener.max_windows, 875, "28 s is 875 windows");

        // The conversion that the rest of this module rests on: a millisecond
        // count is not a sample count. Stated as its own arithmetic so a
        // regression here fails with the number rather than as a mysterious
        // never-ending utterance.
        assert_eq!(audio::TARGET_RATE as usize * 100 / 1000, 1_600);
        assert_eq!(1_600usize.div_ceil(WINDOW), 4);
    }

    #[test]
    fn max_utterance_stays_inside_whispers_receptive_field() {
        // Whisper cannot see past 30 seconds, so the cap has to be under it with
        // room for the trailing silence.
        assert!(MAX_UTTERANCE_SECONDS < 30.0);
        assert!(MAX_UTTERANCE_SECONDS > 20.0);
    }

    // -- the state machine --------------------------------------------------

    #[test]
    fn a_qualifying_utterance_reports_started_then_finished() {
        // Eight speech windows to qualify (min_speech_windows), then enough
        // quiet to close.
        let mut script = vec![0.9f32; 12];
        script.extend(vec![0.05f32; 10]);
        let mut listener = Listener::new(Scripted::new(script));

        let blocks: Vec<Vec<f32>> = (0..22)
            .map(|index| window(if index < 12 { 0.5 } else { 0.0 }))
            .collect();
        let reports = feed(&mut listener, &blocks);

        assert_eq!(reports.len(), 2, "{reports:?}");
        assert!(matches!(reports[0], Listening::Started { .. }));
        match &reports[1] {
            Listening::Finished { samples } => {
                // Twelve windows of speech, and the four quiet ones that closed
                // it are the trailing-silence trim's business.
                assert!(
                    samples.len() >= 12 * WINDOW,
                    "only {} samples survived",
                    samples.len()
                );
            }
            other => panic!("expected Finished, got {other:?}"),
        }
    }

    #[test]
    fn a_brief_click_never_opens_an_utterance() {
        // Two speech windows is 64 ms, well under the 250 ms minimum. The
        // reference discards these, and so must the listener: a door closing
        // must not become a dictation.
        let mut script = vec![0.9f32; 2];
        script.extend(vec![0.05f32; 20]);
        let mut listener = Listener::new(Scripted::new(script));

        let blocks: Vec<Vec<f32>> = (0..22)
            .map(|index| window(if index < 2 { 0.5 } else { 0.0 }))
            .collect();
        let reports = feed(&mut listener, &blocks);

        assert!(reports.is_empty(), "a 64 ms click produced {reports:?}");
        assert!(!listener.is_speaking());
    }

    #[test]
    fn a_gap_shorter_than_the_minimum_does_not_close_an_utterance() {
        // Eight windows to qualify, three quiet (96 ms, under the 100 ms
        // minimum), then speech resumes. One utterance, not two.
        let mut script = vec![0.9f32; 8];
        script.extend(vec![0.05f32; 3]);
        script.extend(vec![0.9f32; 10]);
        script.extend(vec![0.05f32; 10]);

        let mut listener = Listener::new(Scripted::new(script));
        let blocks: Vec<Vec<f32>> = (0..31)
            .map(|index| window(if (8..11).contains(&index) || index >= 21 { 0.0 } else { 0.5 }))
            .collect();
        let reports = feed(&mut listener, &blocks);

        let finished = reports
            .iter()
            .filter(|report| matches!(report, Listening::Finished { .. }))
            .count();
        assert_eq!(finished, 1, "a 96 ms gap split the utterance: {reports:?}");
    }

    #[test]
    fn a_span_between_the_thresholds_does_not_close_an_utterance() {
        // 0.4 is below SPEECH_THRESHOLD but above SILENCE_THRESHOLD, so it is
        // neither speech nor quiet enough to end — the hysteresis band.
        let mut script = vec![0.9f32; 8];
        script.extend(vec![0.4f32; 10]);
        script.extend(vec![0.9f32; 4]);
        script.extend(vec![0.05f32; 10]);

        let mut listener = Listener::new(Scripted::new(script));
        let blocks: Vec<Vec<f32>> = (0..32).map(|_| window(0.5)).collect();
        let reports = feed(&mut listener, &blocks);

        let finished = reports
            .iter()
            .filter(|report| matches!(report, Listening::Finished { .. }))
            .count();
        assert_eq!(finished, 1, "the hysteresis band split it: {reports:?}");
    }

    #[test]
    fn an_utterance_that_hits_the_cap_is_reported_as_truncated() {
        // Only ever speech, so nothing closes it and the cap is what ends it.
        let mut listener = Listener::new(Scripted::new(vec![0.9]));

        let mut truncated = None;
        // Enough windows to exceed the cap.
        for _ in 0..900 {
            if let Ok(report) = listener.push(&window(0.5)) {
                if let Listening::Truncated { samples } = report {
                    truncated = Some(samples);
                    break;
                }
            }
        }

        let samples = truncated.expect("the cap should have fired");
        let seconds = samples.len() as f32 / audio::TARGET_RATE as f32;
        assert!(
            (seconds - MAX_UTTERANCE_SECONDS).abs() < 0.5,
            "truncated at {seconds:.1}s, expected about {MAX_UTTERANCE_SECONDS:.0}s"
        );
        // And it is reported as truncated rather than finished, so a caller can
        // tell a complete sentence from a cut-off one.
        assert!(!listener.is_speaking());
    }

    #[test]
    fn finishing_a_stream_keeps_the_open_utterance() {
        // Someone stops the microphone mid-sentence. Losing that audio would be
        // the worst possible behaviour — it is the sentence they just spoke.
        let mut listener = Listener::new(Scripted::new(vec![0.9]));
        for _ in 0..10 {
            listener.push(&window(0.5)).expect("the stub cannot fail");
        }
        assert!(listener.is_speaking());

        let report = listener.finish().expect("the stub cannot fail");
        match report {
            Some(Listening::Finished { samples }) => {
                assert!(samples.len() >= 8 * WINDOW, "only {} samples", samples.len());
            }
            other => panic!("expected a Finished utterance, got {other:?}"),
        }
    }

    #[test]
    fn finishing_a_stream_with_only_a_click_reports_nothing() {
        let mut script = vec![0.9f32; 2];
        script.extend(vec![0.05f32; 50]);
        let mut listener = Listener::new(Scripted::new(script));

        for index in 0..10 {
            listener
                .push(&window(if index < 2 { 0.5 } else { 0.0 }))
                .expect("the stub cannot fail");
        }

        assert_eq!(
            listener.finish().expect("the stub cannot fail"),
            None,
            "a click became an utterance"
        );
    }

    #[test]
    fn finishing_an_idle_stream_reports_nothing() {
        let mut listener = Listener::new(Scripted::new(vec![0.0]));
        assert_eq!(listener.finish().expect("the stub cannot fail"), None);
    }

    #[test]
    fn reset_clears_the_detector_and_the_utterance() {
        let mut listener = Listener::new(Scripted::new(vec![0.9]));
        for _ in 0..10 {
            listener.push(&window(0.5)).expect("the stub cannot fail");
        }
        assert!(listener.is_speaking());
        assert!(listener.pending_seconds() > 0.0);

        listener.reset();

        assert!(!listener.is_speaking());
        assert_eq!(listener.pending_seconds(), 0.0);
        assert_eq!(listener.finish().expect("the stub cannot fail"), None);
    }

    #[test]
    fn pending_seconds_tracks_the_open_utterance() {
        let mut listener = Listener::new(Scripted::new(vec![0.9]));
        assert_eq!(listener.pending_seconds(), 0.0);

        for _ in 0..32 {
            listener.push(&window(0.5)).expect("the stub cannot fail");
        }

        // 32 windows is one second of audio.
        let seconds = listener.pending_seconds();
        assert!(
            (seconds - 1.0).abs() < 0.05,
            "pending_seconds said {seconds:.3}, expected about 1.0"
        );
    }

    #[test]
    fn a_report_carries_its_audio_and_nothing_carries_nothing() {
        let started = Listening::Started {
            samples: vec![0.1],
        };
        assert_eq!(started.samples(), Some([0.1].as_slice()));
        assert_eq!(Listening::Nothing.samples(), None);
    }

    #[test]
    fn a_block_larger_than_one_window_is_handled() {
        // A microphone callback can deliver more than a window at once, and
        // everything in the block must still be scored in order.
        //
        // The script has to line up with the blocks: five windows of speech do
        // not qualify, so the first push reports nothing and the second one
        // carries both the start and the end of the utterance.
        let mut script = vec![0.9f32; 8];
        script.extend(vec![0.05f32; 20]);
        let mut listener = Listener::new(Scripted::new(script));

        // Five windows of speech in one push: not enough to qualify (the
        // minimum is eight).
        let report = listener
            .push(&vec![0.5f32; WINDOW * 5])
            .expect("the stub cannot fail");
        assert!(matches!(report, Listening::Nothing), "{report:?}");

        // Twelve more windows of audio: three of speech to qualify, then nine of
        // silence to close. Both happen inside one push, and the `Finished` is
        // the report that survives — the more useful of the two.
        let mut block = vec![0.5f32; WINDOW * 3];
        block.extend(vec![0.0f32; WINDOW * 9]);
        let report = listener.push(&block).expect("the stub cannot fail");

        match report {
            Listening::Finished { samples } => {
                // Eight windows of speech; the trim drops the quiet tail.
                assert!(
                    samples.len() >= 8 * WINDOW,
                    "only {} samples survived",
                    samples.len()
                );
                assert!(
                    samples.len() < block.len(),
                    "the quiet tail was not trimmed"
                );
            }
            other => panic!("expected Finished, got {other:?}"),
        }
    }

    #[test]
    fn a_partial_block_is_carried_rather_than_dropped() {
        let mut listener = Listener::new(Scripted::new(vec![0.9]));
        // Half a window at a time: no window completes on the first push.
        let half = vec![0.5f32; WINDOW / 2];
        listener.push(&half).expect("the stub cannot fail");
        listener.push(&half).expect("the stub cannot fail");

        // The two halves made one window, so the utterance has begun counting.
        let third = vec![0.5f32; WINDOW / 2];
        listener.push(&third).expect("the stub cannot fail");
        assert!(listener.pending_seconds() > 0.0);
    }

    #[test]
    fn a_brief_dip_in_the_middle_of_speech_does_not_reset_the_pending_audio() {
        // The bug this pins: a strict "one sub-threshold window clears the
        // burst" rule meant real speech never qualified, because a detector
        // dips mid-word. Measured against the real model, that version reported
        // no utterance at all on eleven seconds of clear speech.
        let mut script = vec![0.9f32; 4];
        script.push(0.1); // one dip
        script.extend(vec![0.9f32; 5]); // then enough to qualify
        script.extend(vec![0.05f32; 20]);
        let mut listener = Listener::new(Scripted::new(script));

        // Nine speech windows in total with a dip four windows in: still one
        // utterance, because the quiet never reached the minimum.
        let blocks: Vec<Vec<f32>> = (0..30).map(|_| window(0.5)).collect();
        let reports = feed(&mut listener, &blocks);

        let started = reports
            .iter()
            .filter(|report| matches!(report, Listening::Started { .. }))
            .count();
        assert_eq!(started, 1, "a single dip lost the utterance: {reports:?}");
    }

    #[test]
    fn a_gap_long_enough_does_abandon_a_burst_that_never_qualified() {
        // The other side of the same rule: a click followed by real silence is
        // abandoned, so it cannot become the opening of a later utterance.
        let mut script = vec![0.9f32; 4]; // never reaches the 8-window minimum
        script.extend(vec![0.0f32; 20]); // long quiet clears it
        script.extend(vec![0.9f32; 10]); // then real speech arrives
        script.extend(vec![0.05f32; 20]);
        let mut listener = Listener::new(Scripted::new(script));

        let blocks: Vec<Vec<f32>> = (0..54).map(|_| window(0.5)).collect();
        let reports = feed(&mut listener, &blocks);

        let started = reports
            .iter()
            .filter(|report| matches!(report, Listening::Started { .. }))
            .count();
        assert_eq!(started, 1, "the abandoned burst was kept: {reports:?}");
    }

    // -- resampling ---------------------------------------------------------

    #[test]
    fn audio_at_another_rate_is_resampled_rather_than_misread() {
        // 48 kHz stereo, two seconds. Treated as 16 kHz it would be six seconds
        // of the wrong-speed audio, and the VAD would still score it — which is
        // exactly why the conversion belongs inside the listener.
        let mut interleaved = Vec::new();
        for _ in 0..(48_000 * 2) {
            interleaved.push(0.3f32);
            interleaved.push(0.3f32);
        }

        let mut listener = Listener::new(Scripted::new(vec![0.9]));
        listener
            .push_audio(&interleaved, 48_000, 2)
            .expect("the stub cannot fail");

        // Two seconds of 16 kHz audio is 62 windows, so the 8-window minimum is
        // comfortably met and the utterance has opened.
        assert!(listener.is_speaking());
        let seconds = listener.pending_seconds();
        assert!(
            (seconds - 2.0).abs() < 0.1,
            "two seconds of audio became {seconds:.2} s"
        );
    }

    #[test]
    fn audio_already_at_the_target_rate_passes_through() {
        let mut listener = Listener::new(Scripted::new(vec![0.9]));
        let samples = vec![0.5f32; audio::TARGET_RATE as usize];
        listener
            .push_audio(&samples, audio::TARGET_RATE, 1)
            .expect("the stub cannot fail");
        assert!((listener.pending_seconds() - 1.0).abs() < 0.05);
    }

    // -- trailing silence ---------------------------------------------------

    #[test]
    fn trailing_silence_is_trimmed_from_a_finished_utterance() {
        // Whisper will hallucinate into a long quiet tail, so the audio handed
        // over should not include it.
        let samples = {
            let mut s = vec![0.5f32; WINDOW * 10];
            s.extend(vec![0.0001f32; WINDOW * 6]);
            s
        };
        let trimmed = trim_trailing_silence(&samples, SILENCE_THRESHOLD);
        assert!(
            trimmed.len() <= WINDOW * 10,
            "the quiet tail survived: {} samples",
            trimmed.len()
        );
    }

    #[test]
    fn trimming_never_empties_the_audio() {
        // All quiet: better to hand over something than nothing.
        let samples = vec![0.0f32; WINDOW * 3];
        assert_eq!(trim_trailing_silence(&samples, SILENCE_THRESHOLD).len(), samples.len());
    }

    #[test]
    fn a_loud_tail_is_left_alone() {
        let samples = vec![0.5f32; WINDOW * 4];
        assert_eq!(trim_trailing_silence(&samples, SILENCE_THRESHOLD).len(), samples.len());
    }

    // -- against the real detector ------------------------------------------

    #[test]
    fn the_real_vad_finds_the_kokoro_utterance() {
        let _guard = crate::voice::model_lock();

        let Some(model) = dirs::home_dir()
            .map(|home| home.join(".loom/voice/silero_vad.onnx"))
            .filter(|path| path.exists())
        else {
            return;
        };
        let Some(runtime) = dirs::home_dir()
            .map(|home| home.join(".loom/ort/onnxruntime.dll"))
            .filter(|path| path.exists())
        else {
            return;
        };
        let _ = ort::init_from(&runtime).map(|builder| builder.commit());

        // The Kokoro sample: 11 seconds of continuous speech with a little
        // silence at each end, which is the exact shape a dictation has.
        let wav = std::env::temp_dir().join("loom-kokoro.wav");
        let Ok(bytes) = std::fs::read(&wav) else {
            return;
        };
        let Some((samples, rate, channels)) = read_wav(&bytes) else {
            return;
        };
        let mono = audio::downmix(&samples, channels);
        let audio = audio::resample_to_16k(&mono, rate);

        let vad = Vad::load(&model).expect("the VAD should load");
        let mut listener = Listener::new(vad);

        // Collect every utterance, not just the first. This recording is two
        // sentences with pauses between clauses, so a listener that returns one
        // span has swallowed the pauses rather than found the speech.
        let mut utterances: Vec<Vec<f32>> = Vec::new();
        let mut started = 0usize;

        // Feed in 100 ms blocks, the way a microphone would.
        for block in audio.chunks(audio::TARGET_RATE as usize / 10) {
            match listener.push(block).expect("the VAD should run") {
                Listening::Nothing => {}
                Listening::Started { .. } => started += 1,
                Listening::Finished { samples } => utterances.push(samples),
                Listening::Truncated { samples } => utterances.push(samples),
            }
        }

        if let Some(report) = listener.finish().expect("the VAD should run") {
            if let Some(samples) = report.samples() {
                utterances.push(samples.to_vec());
            }
        }

        assert!(started > 0, "the listener never saw speech start");
        assert!(
            !utterances.is_empty(),
            "eleven seconds of speech produced no utterances"
        );

        // Each utterance has to be at least the minimum speech length, or the
        // listener is emitting fragments.
        for (index, samples) in utterances.iter().enumerate() {
            let seconds = samples.len() as f32 / audio::TARGET_RATE as f32;
            assert!(
                seconds >= 0.2,
                "utterance {index} was only {seconds:.2} s, below the minimum"
            );
        }

        // And collectively they have to account for most of the recording. The
        // shortfall is trailing silence at each pause, which is trimmed on
        // purpose — Whisper hallucinates into a quiet tail.
        let captured: usize = utterances.iter().map(|samples| samples.len()).sum();
        let total = audio.len();
        let ratio = captured as f32 / total as f32;

        assert!(
            ratio > 0.7,
            "only {:.0}% of the recording was captured across {} utterances \
             ({:.2} s of {:.2} s)",
            ratio * 100.0,
            utterances.len(),
            captured as f32 / audio::TARGET_RATE as f32,
            total as f32 / audio::TARGET_RATE as f32
        );

        // Nothing may be invented: the utterances cannot exceed the audio.
        assert!(
            captured <= total,
            "the listener produced more audio than it was given"
        );
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
