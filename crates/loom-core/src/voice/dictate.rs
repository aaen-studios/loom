//! Dictation: audio in, utterances out, transcripts back.
//!
//! This is the module that closes the loop for speech *input*. Every layer
//! beneath it already exists and is separately verified — [`audio`] for
//! resampling and framing, [`vad`] for the speech probabilities, [`listen`] for
//! the endpointing state machine, [`whisper`] for the recognition — and none of
//! them know about each other. [`Dictation`] is what puts them in a line.
//!
//! # Why this is in the core rather than in the command layer
//!
//! Because it is the part that is testable against real audio, and the test that
//! matters is end to end: feed the Kokoro sample in through the *streaming* path
//! and check that the words come back. A version of this living in the Tauri
//! layer could only be tested through a running app and a real microphone, which
//! means in practice it would not be tested at all.
//!
//! # Two entry points, deliberately
//!
//! * [`Dictation::push`] takes microphone blocks. It returns a transcript only
//!   when an utterance ends, because that is the only moment a transcript is
//!   possible — recognition needs the whole utterance.
//! * [`Dictation::transcribe`] takes a whole clip. It is what the `transcribe`
//!   example uses, and it exists so the file path and the live path share one
//!   implementation rather than two that could drift.
//!
//! # What it does not do
//!
//! Run recognition while more audio is arriving. Everything is synchronous, so a
//! caller feeding a live microphone must push from a thread that is allowed to
//! block for the length of a transcription — around 0.3× real time at the
//! measured rate. The Tauri layer does exactly that: one worker owns the model
//! and the microphone blocks queue behind it. Streaming *partial* results would
//! need a second thread and a policy for what to do with a half-finished
//! hypothesis, and it is not attempted here.

use std::path::Path;

use super::listen::{Listener, Listening};
use super::vad::Vad;
use super::whisper::{Paths, Transcription, Whisper};
use super::{audio, tts};
use crate::Result;

/// A finished piece of speech, recognised.
#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    /// What was said. Empty when recognition found nothing, which happens for a
    /// cough or a door.
    pub text: String,
    /// How much audio went in.
    pub audio_seconds: f32,
    /// How long recognition took.
    pub compute_seconds: f64,
    /// Whether the utterance hit [`MAX_UTTERANCE_SECONDS`] and was cut off, so
    /// the last word may be missing.
    pub truncated: bool,
}

impl Heard {
    /// Nothing was said, or nothing was understood.
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    /// Compute time divided by audio time. Below 1.0 is faster than real time.
    pub fn real_time_factor(&self) -> f32 {
        if self.audio_seconds <= 0.0 {
            return 0.0;
        }
        (self.compute_seconds as f32) / self.audio_seconds
    }
}

/// Text plus the numbers behind it, for the file path.
#[derive(Debug, Clone, PartialEq)]
pub struct Read {
    pub heard: Heard,
    /// The tokens the decoder chose, for diagnosing a bad transcript.
    pub tokens: Vec<i64>,
}

impl From<Transcription> for Read {
    fn from(value: Transcription) -> Self {
        Self {
            heard: Heard {
                text: value.text,
                audio_seconds: value.audio_seconds,
                compute_seconds: value.compute_seconds,
                truncated: false,
            },
            tokens: value.tokens,
        }
    }
}

/// A loaded dictation session: a detector and a recogniser, in a line.
///
/// Holds both models resident. That is deliberate for a live session — the
/// Whisper graphs are hundreds of megabytes and loading them per utterance would
/// put a second of latency in the gap between someone finishing a sentence and
/// seeing it — and it is why the caller should keep one for the lifetime of a
/// voice-mode session rather than constructing one per call.
pub struct Dictation {
    listener: Listener<Vad>,
    whisper: Whisper,
}

impl std::fmt::Debug for Dictation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dictation")
            .field("speaking", &self.listener.is_speaking())
            .field("pending_seconds", &self.listener.pending_seconds())
            .finish_non_exhaustive()
    }
}

impl Dictation {
    /// Loads the detector and the recogniser from the Loom home directory.
    pub fn load() -> Result<Self> {
        let home = crate::paths::loom_home()?;
        Self::load_from(&home)
    }

    /// Loads from an explicit home directory, for tests and for `LOOM_HOME`.
    pub fn load_from(home: &Path) -> Result<Self> {
        // The speech-to-text side has no runtime of its own: `ort` allows one
        // environment per process and the runtime path belongs to the TTS side.
        // Initialising it here means dictation works on its own, without a
        // synthesiser ever having been loaded first.
        let tts_paths = tts::Paths::from_home(home);
        ort::init_from(&tts_paths.runtime)
            .map_err(|e| {
                crate::Error::Http(format!(
                    "could not load ONNX Runtime from {}: {e}",
                    tts_paths.runtime.display()
                ))
            })?
            .commit();

        let paths = Paths::from_home(home);
        let vad = Vad::load(&paths.vad)?;
        let whisper = Whisper::load(&paths)?;

        Ok(Self {
            listener: Listener::new(vad),
            whisper,
        })
    }

    /// Whether an utterance is currently being collected.
    pub fn speaking(&self) -> bool {
        self.listener.is_speaking()
    }

    /// How much audio the open utterance holds.
    pub fn pending_seconds(&self) -> f32 {
        self.listener.pending_seconds()
    }

    /// Clears the detector and any partial utterance.
    ///
    /// The recogniser is untouched: it holds no per-utterance state.
    pub fn reset(&mut self) {
        self.listener.reset();
    }

    /// Feeds microphone audio and returns a transcript when one is ready.
    ///
    /// `Some` means an utterance ended *and* recognition found words in it. An
    /// utterance that recognises to nothing returns `None` rather than an empty
    /// `Heard`, so a caller cannot accidentally commit a blank message.
    ///
    /// Audio may be at any rate with any channel count; the conversion happens
    /// here so a caller cannot forget it.
    pub fn push(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        channels: usize,
    ) -> Result<Option<Heard>> {
        match self.listener.push_audio(samples, sample_rate, channels)? {
            Listening::Finished { samples } => Ok(self.recognise(&samples, false)?),
            Listening::Truncated { samples } => Ok(self.recognise(&samples, true)?),
            // A start carries no transcript — recognition needs the whole
            // utterance — and `Nothing` carries no news.
            Listening::Started { .. } | Listening::Nothing => Ok(None),
        }
    }

    /// Ends the stream, transcribing anything still open.
    ///
    /// Someone pressing stop mid-sentence gets that sentence. Losing it would be
    /// the worst possible behaviour, since it is exactly what they just said.
    pub fn finish(&mut self) -> Result<Option<Heard>> {
        match self.listener.finish()? {
            Some(report) => Ok(self.recognise(
                report.samples().unwrap_or(&[]),
                matches!(report, Listening::Truncated { .. }),
            )?),
            None => Ok(None),
        }
    }

    /// Transcribes a whole clip, with no endpointing.
    ///
    /// For a file, or for a caller that has already found the utterance. Shares
    /// the recogniser with the streaming path, so the two cannot diverge.
    pub fn transcribe(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        channels: usize,
    ) -> Result<Read> {
        Ok(self
            .whisper
            .transcribe(samples, sample_rate, channels)?
            .into())
    }

    /// Recognises one utterance, returning `None` when there were no words.
    fn recognise(&mut self, samples: &[f32], truncated: bool) -> Result<Option<Heard>> {
        if samples.is_empty() {
            return Ok(None);
        }

        // Already mono at 16 kHz by the time it leaves the listener, so the
        // rate is stated rather than converted twice.
        let read: Read = self
            .whisper
            .transcribe(samples, audio::TARGET_RATE, 1)?
            .into();

        if read.heard.is_empty() {
            return Ok(None);
        }

        Ok(Some(Heard {
            truncated,
            ..read.heard
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::listen::MAX_UTTERANCE_SECONDS;

    /// The words in the Kokoro sample, taken from the pipeline's own output
    /// rather than from an assumption about what the file says.
    ///
    /// The first version of this list was a guess, and wrong: the sample is a
    /// Loom demo line, not the pangram that seemed natural for a speech test. A
    /// test whose expectation is invented fails for the right reason — it says
    /// the transcript does not match — and the message is unhelpfully confident
    /// about a claim nobody checked. Everything here was read out of a real
    /// transcription.
    ///
    /// Chosen to be unambiguous rather than exhaustive: no short word that could
    /// appear inside another (`is` inside `this`, `any` inside `anywhere`).
    const EXPECTED: &[&str] = &[
        "hello", "this", "loom", "speaking", "small", "model", "sounds", "better", "runs",
        "entirely", "machine", "never", "sends", "anywhere",
    ];

    /// The Kokoro sample as mono `f32` at 16 kHz, or `None` when it is absent.
    fn sample() -> Option<Vec<f32>> {
        let wav = std::env::temp_dir().join("loom-kokoro.wav");
        let bytes = std::fs::read(&wav).ok()?;
        let (samples, rate, channels) = read_wav(&bytes)?;
        let mono = audio::downmix(&samples, channels);
        Some(audio::resample_to_16k(&mono, rate))
    }

    /// Loads a dictation session, or returns `None` when the models are not
    /// installed — the suite must pass on a machine that has never run the
    /// installer.
    fn dictation() -> Option<Dictation> {
        let home = dirs::home_dir()?.join(".loom");
        if !Paths::from_home(&home).complete() {
            return None;
        }
        if !tts::Paths::from_home(&home).runtime.exists() {
            return None;
        }
        Dictation::load_from(&home).ok()
    }

    fn words(text: &str) -> String {
        text.to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { ' ' })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    // -- the shape of a result, without a model ----------------------------

    #[test]
    fn an_empty_transcript_is_reported_as_empty() {
        let heard = Heard {
            text: "   ".to_string(),
            audio_seconds: 1.0,
            compute_seconds: 0.5,
            truncated: false,
        };
        assert!(heard.is_empty());

        let heard = Heard {
            text: "hello".to_string(),
            ..heard
        };
        assert!(!heard.is_empty());
    }

    #[test]
    fn the_real_time_factor_is_zero_rather_than_infinite_for_no_audio() {
        let heard = Heard {
            text: "x".to_string(),
            audio_seconds: 0.0,
            compute_seconds: 1.0,
            truncated: false,
        };
        // Dividing by zero seconds would be infinity, which a UI would render as
        // a nonsense number rather than as "not applicable".
        assert_eq!(heard.real_time_factor(), 0.0);
    }

    #[test]
    fn the_real_time_factor_is_compute_over_audio() {
        let heard = Heard {
            text: "x".to_string(),
            audio_seconds: 2.0,
            compute_seconds: 0.5,
            truncated: false,
        };
        assert!((heard.real_time_factor() - 0.25).abs() < 1e-6);
    }

    // -- end to end, on real speech ---------------------------------------

    #[test]
    fn a_whole_clip_transcribes() {
        let _guard = crate::voice::model_lock();

        let Some(mut dictation) = dictation() else {
            return;
        };
        let Some(audio) = sample() else {
            return;
        };

        let read = dictation
            .transcribe(&audio, audio::TARGET_RATE, 1)
            .expect("transcription should work");

        let text = words(&read.heard.text);
        let found = EXPECTED.iter().filter(|word| text.contains(*word)).count();
        assert!(
            found >= EXPECTED.len() - 1,
            "only {found} of {} expected words appeared in {:?}",
            EXPECTED.len(),
            read.heard.text
        );
        assert!(
            read.heard.real_time_factor() < 1.0,
            "transcription should be faster than real time, was {:.2}×",
            read.heard.real_time_factor()
        );
        // The file path never truncates: nothing decides an endpoint.
        assert!(!read.heard.truncated);
    }

    #[test]
    fn streaming_audio_produces_a_transcript() {
        let _guard = crate::voice::model_lock();

        let Some(mut dictation) = dictation() else {
            return;
        };
        let Some(audio) = sample() else {
            return;
        };

        // Fed in 100 ms blocks, exactly as the microphone layer will. This is
        // the assertion the module exists for: it runs the *live* path, not the
        // file path, so it covers endpointing and recognition together.
        let block = audio::TARGET_RATE as usize / 10;
        let mut heard: Vec<Heard> = Vec::new();

        for chunk in audio.chunks(block) {
            if let Some(result) = dictation
                .push(chunk, audio::TARGET_RATE, 1)
                .expect("the streaming path should work")
            {
                heard.push(result);
            }
        }
        if let Some(result) = dictation.finish().expect("finishing should work") {
            heard.push(result);
        }

        assert!(
            !heard.is_empty(),
            "eleven seconds of speech produced no transcript"
        );

        // Recognised words, across every utterance. The recording has pauses, so
        // it arrives as several utterances and the words are spread over them.
        let joined = words(
            &heard
                .iter()
                .map(|result| result.text.clone())
                .collect::<Vec<_>>()
                .join(" "),
        );
        let found = EXPECTED.iter().filter(|word| joined.contains(*word)).count();
        assert!(
            found >= EXPECTED.len() - 4,
            "only {found} of {} expected words survived the streaming path: {joined:?}",
            EXPECTED.len()
        );

        // And most of the audio has to have been captured, or the endpointing is
        // throwing speech away. The shortfall is trimmed trailing silence.
        let captured: f32 = heard.iter().map(|result| result.audio_seconds).sum();
        let total = audio.len() as f32 / audio::TARGET_RATE as f32;
        let ratio = captured / total;
        assert!(
            ratio > 0.7,
            "only {:.0}% of the audio reached the recogniser",
            ratio * 100.0
        );

        // Nothing may be invented.
        assert!(
            captured <= total + 0.01,
            "the transcripts claim {captured:.2} s of a {total:.2} s clip"
        );
    }

    #[test]
    fn silence_produces_no_transcript() {
        let _guard = crate::voice::model_lock();

        let Some(mut dictation) = dictation() else {
            return;
        };

        // Five seconds of digital silence in 100 ms blocks. A detector that
        // fires on silence would send this to Whisper, which is exactly how a
        // hallucinated sentence gets committed to a chat.
        let quiet = vec![0.0f32; audio::TARGET_RATE as usize * 5];
        let mut results = 0usize;
        for chunk in quiet.chunks(audio::TARGET_RATE as usize / 10) {
            if dictation
                .push(chunk, audio::TARGET_RATE, 1)
                .expect("should run")
                .is_some()
            {
                results += 1;
            }
        }
        assert_eq!(results, 0, "silence produced {results} transcripts");
        assert!(!dictation.speaking(), "silence opened an utterance");
    }

    #[test]
    fn reset_discards_a_partial_utterance() {
        let _guard = crate::voice::model_lock();

        let Some(mut dictation) = dictation() else {
            return;
        };
        let Some(audio) = sample() else {
            return;
        };

        // Half the sample, so an utterance is open.
        for chunk in audio[..audio.len() / 2].chunks(audio::TARGET_RATE as usize / 10) {
            dictation.push(chunk, audio::TARGET_RATE, 1).expect("should run");
        }
        assert!(dictation.speaking(), "the sample should have opened an utterance");
        assert!(dictation.pending_seconds() > 0.0);

        dictation.reset();

        assert!(!dictation.speaking());
        assert_eq!(dictation.pending_seconds(), 0.0);
        assert_eq!(
            dictation.finish().expect("should run"),
            None,
            "reset left an utterance behind"
        );
    }

    #[test]
    fn audio_at_another_rate_is_converted_rather_than_misread() {
        let _guard = crate::voice::model_lock();

        let Some(mut dictation) = dictation() else {
            return;
        };
        let Some(audio) = sample() else {
            return;
        };

        // Fed as 48 kHz data. Skipping the conversion would make it a
        // three-times-speed recording, which recognises as nothing useful — the
        // failure that looks like a bad model rather than a missing resample.
        let upsample = |input: &[f32]| -> Vec<f32> {
            let mut out = Vec::with_capacity(input.len() * 3);
            for pair in input.windows(2) {
                for step in 0..3 {
                    let t = step as f32 / 3.0;
                    out.push(pair[0] + (pair[1] - pair[0]) * t);
                }
            }
            out
        };

        let at_48k = upsample(&audio);
        let mut heard: Vec<Heard> = Vec::new();
        for chunk in at_48k.chunks(48_000 / 10) {
            if let Some(result) = dictation.push(chunk, 48_000, 1).expect("should run") {
                heard.push(result);
            }
        }
        if let Some(result) = dictation.finish().expect("should run") {
            heard.push(result);
        }

        let joined = words(
            &heard
                .iter()
                .map(|result| result.text.clone())
                .collect::<Vec<_>>()
                .join(" "),
        );
        let found = EXPECTED.iter().filter(|word| joined.contains(*word)).count();
        assert!(
            found >= EXPECTED.len() - 4,
            "48 kHz input lost the words: {found} of {} in {joined:?}",
            EXPECTED.len()
        );
    }

    #[test]
    fn a_very_long_utterance_is_marked_truncated() {
        let _guard = crate::voice::model_lock();

        let Some(mut dictation) = dictation() else {
            return;
        };

        // Speech held past the cap, so the listener closes the utterance itself.
        //
        // This has to be *continuous* speech, which is the part that is easy to
        // get wrong: the whole sample has half-second pauses in it, so replaying
        // it end to end closes an utterance at every pause and the cap never
        // fires. An earlier version of this test asserted on the first utterance
        // it saw and failed with "a 0.4 s utterance was reported as complete" —
        // which was true, and not the cap's doing.
        //
        // The sample's longest unbroken run of speech is 3.42–6.62 s, so that
        // slice is the one to repeat. A tone will not do: it does not open an
        // utterance reliably, and the cap only fires inside one.
        let Some(audio) = sample() else {
            return;
        };
        let start = (3.42 * audio::TARGET_RATE as f32) as usize;
        let end = (6.60 * audio::TARGET_RATE as f32) as usize;
        let slice = &audio[start..end.min(audio.len())];

        let mut continuous = Vec::with_capacity(slice.len() * 12);
        for _ in 0..12 {
            continuous.extend_from_slice(slice);
        }
        let total_seconds = continuous.len() as f32 / audio::TARGET_RATE as f32;
        assert!(
            total_seconds > MAX_UTTERANCE_SECONDS + 2.0,
            "the fixture is {total_seconds:.1} s, not past the {MAX_UTTERANCE_SECONDS:.0} s cap"
        );

        let mut result = None;
        'outer: for chunk in continuous.chunks(audio::TARGET_RATE as usize / 10) {
            if let Some(heard) = dictation.push(chunk, audio::TARGET_RATE, 1).expect("should run") {
                // A natural end means the repeated slice had a seam after all;
                // the cap is what this test is about, so keep feeding.
                if heard.truncated {
                    result = Some(heard);
                    break 'outer;
                }
            }
        }

        let heard = result.expect("the cap should have fired");
        assert!(
            heard.truncated,
            "a {:.1} s utterance was reported as complete",
            heard.audio_seconds
        );
        assert!(
            heard.audio_seconds <= MAX_UTTERANCE_SECONDS + 0.5,
            "the utterance ran to {:.1} s, past the cap",
            heard.audio_seconds
        );
        assert!(!heard.is_empty(), "a truncated utterance should still have words");
    }

    #[test]
    fn the_file_and_streaming_paths_agree() {
        let _guard = crate::voice::model_lock();

        let Some(mut dictation) = dictation() else {
            return;
        };
        let Some(audio) = sample() else {
            return;
        };

        // The two paths share a recogniser, so a disagreement would mean one of
        // the wrappers is losing audio. Streaming is allowed to be worse — it
        // splits at pauses and trims each utterance — but it must not be
        // *better*, which would mean the file path is dropping words.
        let whole = dictation
            .transcribe(&audio, audio::TARGET_RATE, 1)
            .expect("should work");
        let whole_words = EXPECTED
            .iter()
            .filter(|word| words(&whole.heard.text).contains(*word))
            .count();

        let mut streaming = Vec::new();
        for chunk in audio.chunks(audio::TARGET_RATE as usize / 10) {
            if let Some(heard) = dictation.push(chunk, audio::TARGET_RATE, 1).expect("should run") {
                streaming.push(heard.text);
            }
        }
        if let Some(heard) = dictation.finish().expect("should run") {
            streaming.push(heard.text);
        }
        let joined = words(&streaming.join(" "));
        let streaming_words = EXPECTED
            .iter()
            .filter(|word| joined.contains(*word))
            .count();

        assert!(
            streaming_words <= whole_words,
            "streaming found {streaming_words} words against the file path's {whole_words}"
        );
        assert!(
            streaming_words >= whole_words - 4,
            "streaming lost too much: {streaming_words} against {whole_words}"
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
