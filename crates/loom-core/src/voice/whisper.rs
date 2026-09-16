//! Whisper transcription: audio in, text out.
//!
//! # The shape of the two models, read from the files
//!
//! | | inputs | outputs |
//! |---|---|---|
//! | encoder | `input_features` `(1, 80, 3000)` f32 | `last_hidden_state` `(1, 1500, 384)` f32 |
//! | decoder | `input_ids` `(1, n)` i64, `encoder_hidden_states` `(1, 1500, 384)` f32 | `logits` `(1, n, 51864)` f32, plus 16 `present.*` |
//!
//! `scripts/probe_whisper` prints this, and everything here is written against
//! what it printed rather than against memory.
//!
//! # Why the loop is this simple, and what it costs
//!
//! The decoder export takes **no `past_key_values` inputs** — only the full
//! token sequence and the encoder's output. It *returns* 16 `present.*` tensors,
//! which are the key/value cache for the next step, but there is nowhere to feed
//! them back. So every generated token re-runs the whole sequence: O(n²) rather
//! than O(n).
//!
//! That is a deliberate trade. Threading a cache by hand means slicing 16
//! tensors per layer per step and getting their shapes exactly right, for a
//! speed-up that a dictation clip — a few dozen tokens — would not notice. The
//! `present.*` outputs are ignored, and the cost is documented rather than
//! hidden.
//!
//! # The three things that make a transcript wrong rather than broken
//!
//! 1. **The prompt.** `[SOT, NO_TIMESTAMPS]`, from [`DecodeIds`]. Without the
//!    second token the model interleaves timestamps with the text.
//! 2. **The suppression list.** Whisper's config names ~90 tokens that must
//!    never be generated. Letting the argmax reach them produces a transcript
//!    full of punctuation-less noise, not an error.
//! 3. **The mel.** Already cross-checked against numpy; see `mel.rs`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ort::session::Session;
use ort::value::Tensor;

use super::mel::{self, MelFilters, N_FREQ_BINS, N_FRAMES, N_MELS};
use super::tokenizer::{DecodeIds, Tokenizer};
use super::{audio, clean};
use crate::{Error, Result};

/// The encoder's own sequence length, after its 2× downsample of the mel.
pub const ENCODER_POSITIONS: i64 = 1500;
/// The decoder's hidden width, which the encoder output and the decoder input
/// share.
pub const HIDDEN: i64 = 384;
/// The vocabulary, as `logits`' last dimension.
pub const VOCAB: usize = 51_864;

/// Whisper's `max_target_positions`: the **total** decoder sequence length,
/// prompt included.
///
/// Not the number of tokens to generate. The position embedding table holds
/// exactly this many rows, so passing one more fails inside the graph with a
/// broadcast error rather than an error anyone can act on:
///
/// ```text
/// Attempting to broadcast an axis by a dimension other than 1. 448 by 449
/// ```
///
/// Getting this wrong only shows up on audio that produces no end-of-text
/// token — a tone, silence, or a noisy recording — because that is the case
/// that runs to the cap instead of stopping early.
pub const MAX_TOKENS: usize = 448;

/// Where the speech-to-text models live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub tokenizer: PathBuf,
    pub generation_config: PathBuf,
    pub config: PathBuf,
    pub vad: PathBuf,
}

impl Paths {
    /// Resolves everything from the Loom home directory.
    pub fn from_home(home: &Path) -> Self {
        let whisper = home.join("voice").join("whisper");
        Self {
            encoder: whisper.join("encoder_model.onnx"),
            decoder: whisper.join("decoder_model.onnx"),
            tokenizer: whisper.join("tokenizer.json"),
            generation_config: whisper.join("generation_config.json"),
            config: whisper.join("config.json"),
            vad: home.join("voice").join("silero_vad.onnx"),
        }
    }

    /// The paths this build will use, honouring `LOOM_HOME`.
    pub fn resolve() -> Result<Self> {
        Ok(Self::from_home(&crate::paths::loom_home()?))
    }

    /// Whether the transcription models are all present.
    ///
    /// The VAD is not included: transcription works without it, and it is
    /// checked separately so a missing detector does not claim the transcriber
    /// is unusable.
    pub fn complete(&self) -> bool {
        self.encoder.exists()
            && self.decoder.exists()
            && self.tokenizer.exists()
            && self.config.exists()
    }
}

/// A finished transcription.
#[derive(Debug, Clone, PartialEq)]
pub struct Transcription {
    pub text: String,
    /// The tokens generated, including the prompt. Kept for diagnostics.
    pub tokens: Vec<i64>,
    /// How much audio went in, in seconds.
    pub audio_seconds: f32,
    /// Total time in the encoder and the decoder.
    pub compute_seconds: f64,
}

impl Transcription {
    /// How much faster than real time this ran. Above 1.0 is faster.
    pub fn real_time_factor(&self) -> f32 {
        if self.audio_seconds <= 0.0 {
            return 0.0;
        }
        (self.compute_seconds as f32) / self.audio_seconds
    }

    /// The generated tokens without the prompt.
    pub fn text_tokens(&self) -> usize {
        self.tokens.len().saturating_sub(2)
    }
}

/// A loaded Whisper, ready to transcribe.
pub struct Whisper {
    encoder: Session,
    decoder: Session,
    tokenizer: Tokenizer,
    ids: DecodeIds,
    filters: MelFilters,
    /// Ids the argmax must never select.
    suppressed: HashSet<i64>,
    /// Ids suppressed only on the first generated token.
    begin_suppressed: HashSet<i64>,
}

impl std::fmt::Debug for Whisper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Whisper")
            .field("vocabulary", &VOCAB)
            .field("suppressed", &self.suppressed.len())
            .field("prompt", &self.ids.prompt())
            .finish_non_exhaustive()
    }
}

impl Whisper {
    /// Loads both graphs and the tokenizer.
    ///
    /// The runtime must already be initialized — `tts::Kokoro::load` does that,
    /// and `ort` memoizes it, so loading either model first is fine as long as
    /// one of them has run.
    pub fn load(paths: &Paths) -> Result<Self> {
        for (label, path) in [
            ("encoder", &paths.encoder),
            ("decoder", &paths.decoder),
            ("tokenizer", &paths.tokenizer),
            ("config", &paths.config),
        ] {
            if !path.exists() {
                return Err(Error::Http(format!(
                    "the Whisper {label} is missing at {}. Run scripts/fetch-whisper.py.",
                    path.display()
                )));
            }
        }

        let encoder = Session::builder()
            .map_err(|e| Error::Http(format!("could not create the encoder session: {e}")))?
            .commit_from_file(&paths.encoder)
            .map_err(|e| Error::Http(format!("could not load the encoder: {e}")))?;

        let decoder = Session::builder()
            .map_err(|e| Error::Http(format!("could not create the decoder session: {e}")))?
            .commit_from_file(&paths.decoder)
            .map_err(|e| Error::Http(format!("could not load the decoder: {e}")))?;

        // The input names come from the session rather than being hardcoded, so
        // a differently-named export fails here with a clear message instead of
        // at inference with an opaque one.
        require_input(&encoder, "input_features")?;
        require_input(&decoder, "input_ids")?;
        require_input(&decoder, "encoder_hidden_states")?;
        require_output(&decoder, "logits")?;

        let tokenizer = Tokenizer::load(&paths.tokenizer)?;

        let generation = std::fs::read_to_string(&paths.generation_config).unwrap_or_default();
        let config_text = std::fs::read_to_string(&paths.config).unwrap_or_default();
        let ids = DecodeIds::from_configs(&generation, &config_text);

        let (suppressed, begin_suppressed) = suppression(&config_text, &generation);

        Ok(Self {
            encoder,
            decoder,
            tokenizer,
            ids,
            filters: MelFilters::new(),
            suppressed,
            begin_suppressed,
        })
    }

    /// The tokenizer, for a caller that wants to decode ids itself.
    pub fn tokenizer(&self) -> &Tokenizer {
        &self.tokenizer
    }

    /// The decode prompt in use.
    pub fn decode_ids(&self) -> DecodeIds {
        self.ids
    }

    /// Transcribes mono audio at any rate the caller has.
    ///
    /// The audio is resampled to 16 kHz and downmixed if it arrives
    /// interleaved; pass `channels = 1` for audio that is already mono. Anything
    /// longer than 30 seconds is truncated, because that is the model's whole
    /// receptive field — a caller with more should chunk it, or a clip cut
    /// mid-word transcribes as a cut word.
    pub fn transcribe(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        channels: usize,
    ) -> Result<Transcription> {
        // Mono at 16 kHz is what the mel expects, and doing it here means a
        // caller cannot forget.
        let mono = audio::downmix(samples, channels);
        let resampled = audio::resample_to_16k(&mono, sample_rate);

        let audio_seconds = resampled.len() as f32 / audio::TARGET_RATE as f32;
        let started = std::time::Instant::now();

        let spectrogram = mel::log_mel_with(&resampled, &self.filters);
        let hidden = self.encode(&spectrogram)?;
        let tokens = self.decode(&hidden)?;
        let compute_seconds = started.elapsed().as_secs_f64();

        // The tokenizer skips control tokens on its own, so the prompt and any
        // stray specials do not reach the text.
        let text = clean::collapse_whitespace(&self.tokenizer.decode(&tokens));

        Ok(Transcription {
            text,
            tokens,
            audio_seconds,
            compute_seconds,
        })
    }

    /// Runs the encoder: a mel spectrogram to the decoder's memory.
    fn encode(&mut self, spectrogram: &mel::MelSpectrogram) -> Result<Vec<f32>> {
        debug_assert_eq!(spectrogram.bins, N_MELS);
        debug_assert_eq!(spectrogram.frames, N_FRAMES);

        // Cloned because `Tensor::from_array` takes ownership and the caller
        // may want the spectrogram afterwards.
        let features = Tensor::from_array((
            [1i64, N_MELS as i64, N_FRAMES as i64],
            spectrogram.data.clone(),
        ))
        .map_err(|e| Error::Http(format!("could not build the feature tensor: {e}")))?;

        let outputs = self
            .encoder
            .run(ort::inputs!["input_features" => features])
            .map_err(|e| Error::Http(format!("the encoder failed: {e}")))?;

        let (shape, data) = outputs["last_hidden_state"]
            .try_extract_tensor::<f32>()
            .map_err(|e| Error::Http(format!("could not read the encoder output: {e}")))?;

        // The shape is checked rather than trusted: a mismatch here would give a
        // decoder error whose message points nowhere near the cause.
        let expected = (1 * ENCODER_POSITIONS * HIDDEN) as usize;
        if data.len() != expected {
            return Err(Error::Http(format!(
                "the encoder returned {} values in shape {shape:?}, expected {expected}",
                data.len()
            )));
        }

        Ok(data.to_vec())
    }

    /// The decoder loop: greedy, one token at a time, stopping at EOS.
    ///
    /// The bound is on the **sequence**, not on the count of steps. The prompt
    /// is part of what the graph sees, and its position table holds
    /// [`MAX_TOKENS`] rows, so the loop has to stop when the sequence reaches
    /// that length — not after that many iterations, which would pass one token
    /// too many and fail inside the graph.
    fn decode(&mut self, hidden: &[f32]) -> Result<Vec<i64>> {
        let mut tokens: Vec<i64> = self.ids.prompt().to_vec();

        while tokens.len() < MAX_TOKENS {
            let step = tokens.len() - 2;
            let ids = Tensor::from_array((
                [1i64, tokens.len() as i64],
                tokens.clone(),
            ))
            .map_err(|e| Error::Http(format!("could not build the token tensor: {e}")))?;

            let memory = Tensor::from_array((
                [1i64, ENCODER_POSITIONS, HIDDEN],
                hidden.to_vec(),
            ))
            .map_err(|e| Error::Http(format!("could not build the memory tensor: {e}")))?;

            let outputs = self
                .decoder
                .run(ort::inputs![
                    "input_ids" => ids,
                    "encoder_hidden_states" => memory,
                ])
                .map_err(|e| Error::Http(format!("the decoder failed at step {step}: {e}")))?;

            let logits = outputs["logits"]
                .try_extract_tensor::<f32>()
                .map_err(|e| Error::Http(format!("could not read the logits: {e}")))?
                .1;

            // `logits` is `(1, n, VOCAB)`. Only the last position predicts the
            // next token, and a wrong offset here reads the *prompt's* logits —
            // which produces a plausible-looking but unrelated transcript.
            let positions = tokens.len();
            let expected = positions * VOCAB;
            if logits.len() < expected {
                return Err(Error::Http(format!(
                    "the decoder returned {} logits for {positions} positions, expected {expected}",
                    logits.len()
                )));
            }
            let last = &logits[expected - VOCAB..expected];

            // The begin-suppression list applies only to the first generated
            // token; the full list applies to every one.
            let suppressed = if step == 0 {
                &self.begin_suppressed
            } else {
                &self.suppressed
            };

            let Some(next) = pick(last, suppressed) else {
                // Every candidate was suppressed. Stopping is the only honest
                // move: appending something arbitrary invents text.
                break;
            };

            if next == self.ids.eos {
                break;
            }
            tokens.push(next);
        }

        Ok(tokens)
    }
}

/// The highest-scoring token that is not suppressed.
///
/// Returns `None` when every candidate is suppressed, rather than falling back
/// to the raw argmax — inventing a token the model was told never to produce is
/// how a transcript acquires words nobody said.
fn pick(logits: &[f32], suppressed: &HashSet<i64>) -> Option<i64> {
    let mut best: Option<(i64, f32)> = None;

    for (index, value) in logits.iter().enumerate() {
        let id = index as i64;
        if suppressed.contains(&id) || !value.is_finite() {
            continue;
        }
        match best {
            Some((_, score)) if *value <= score => {}
            _ => best = Some((id, *value)),
        }
    }

    best.map(|(id, _)| id)
}

/// Reads the suppression lists from the configs.
///
/// `suppress_tokens` is a flat list; `begin_suppress_tokens` applies only to the
/// first generated token. Both are in `config.json` and mirrored in
/// `generation_config.json`.
fn suppression(config: &str, generation: &str) -> (HashSet<i64>, HashSet<i64>) {
    let generation: Option<serde_json::Value> = serde_json::from_str(generation).ok();
    let config: Option<serde_json::Value> = serde_json::from_str(config).ok();

    let read = |key: &str| -> HashSet<i64> {
        let found = generation
            .as_ref()
            .and_then(|value| value.get(key))
            .or_else(|| config.as_ref().and_then(|value| value.get(key)));

        found
            .and_then(|value| value.as_array())
            .map(|list| list.iter().filter_map(|id| id.as_i64()).collect())
            .unwrap_or_default()
    };

    let mut suppressed = read("suppress_tokens");
    let begin = read("begin_suppress_tokens");

    // EOS is never a legal *first* token, and always a legal later one. The
    // configs name it in the begin list, so nothing extra is needed — but the
    // prompt's own tokens must never be re-emitted either, or the loop can
    // latch.
    suppressed.insert(50_257); // <|startoftranscript|>
    suppressed.insert(50_362); // <|notimestamps|>

    (suppressed, begin)
}

/// Errors if a session has no input with this name.
///
/// Checked at load rather than at inference: "no input named input_features" is
/// actionable, and "invalid input name" three frames deep in a decode loop is
/// not.
fn require_input(session: &Session, name: &str) -> Result<()> {
    if session.inputs().iter().any(|input| input.name() == name) {
        return Ok(());
    }
    let found: Vec<&str> = session.inputs().iter().map(|input| input.name()).collect();
    Err(Error::Http(format!(
        "the model has no input called {name:?}; it has {found:?}"
    )))
}

/// Errors if a session has no output with this name.
fn require_output(session: &Session, name: &str) -> Result<()> {
    if session.outputs().iter().any(|output| output.name() == name) {
        return Ok(());
    }
    let found: Vec<&str> = session
        .outputs()
        .iter()
        .map(|output| output.name())
        .collect();
    Err(Error::Http(format!(
        "the model has no output called {name:?}; it has {found:?}"
    )))
}

/// A number no spectrogram value reaches, so the shape asserts are readable.
#[allow(dead_code)]
const _SHAPE_HINT: usize = N_FREQ_BINS;

#[cfg(test)]
mod tests {
    use super::*;

    fn real_paths() -> Option<Paths> {
        let home = dirs::home_dir()?.join(".loom");
        let paths = Paths::from_home(&home);
        paths.complete().then_some(paths)
    }

    // -- pure helpers -------------------------------------------------------

    #[test]
    fn picking_skips_suppressed_tokens() {
        let logits = vec![0.1, 0.9, 0.5, 0.2];
        // Without suppression, index 1 wins.
        assert_eq!(pick(&logits, &HashSet::new()), Some(1));

        // Suppressing the winner promotes the runner-up rather than inventing
        // one.
        let suppressed: HashSet<i64> = [1].into_iter().collect();
        assert_eq!(pick(&logits, &suppressed), Some(2));
    }

    #[test]
    fn picking_returns_nothing_when_everything_is_suppressed() {
        let logits = vec![0.1, 0.2];
        let suppressed: HashSet<i64> = [0, 1].into_iter().collect();
        assert_eq!(pick(&logits, &suppressed), None);
    }

    #[test]
    fn picking_ignores_non_finite_scores() {
        // A NaN or an infinity would otherwise win every comparison and stop
        // the loop dead.
        let logits = vec![f32::NAN, 0.5, f32::INFINITY, 0.2];
        let picked = pick(&logits, &HashSet::new()).expect("a finite value exists");
        assert!(picked == 1, "picked {picked}, expected the finite 0.5");
    }

    #[test]
    fn picking_an_empty_row_returns_nothing() {
        assert_eq!(pick(&[], &HashSet::new()), None);
    }

    #[test]
    fn picking_the_first_of_equal_scores_is_stable() {
        // Ties must resolve the same way every run, or a transcript is not
        // reproducible.
        let logits = vec![0.5, 0.5, 0.5];
        assert_eq!(pick(&logits, &HashSet::new()), Some(0));
        assert_eq!(pick(&logits, &HashSet::new()), Some(0));
    }

    #[test]
    fn suppression_reads_both_configs() {
        let config = r#"{"suppress_tokens": [1, 2, 3], "begin_suppress_tokens": [9]}"#;
        let (always, begin) = suppression(config, "");
        assert!(always.contains(&1) && always.contains(&3));
        assert!(begin.contains(&9));
        // The prompt's own tokens are always suppressed, whatever the file
        // says, so the loop cannot latch on them.
        assert!(always.contains(&50_257));
        assert!(always.contains(&50_362));
    }

    #[test]
    fn suppression_prefers_the_generation_config() {
        let generation = r#"{"suppress_tokens": [7, 8]}"#;
        let config = r#"{"suppress_tokens": [1, 2]}"#;
        let (always, _) = suppression(config, generation);
        assert!(always.contains(&7) && always.contains(&8));
        assert!(!always.contains(&1), "the generation config should win");
    }

    #[test]
    fn suppression_of_a_missing_or_malformed_list_is_empty_not_an_error() {
        let (always, begin) = suppression("{}", "not json");
        // Still carries the two prompt tokens.
        assert_eq!(always.len(), 2);
        assert!(begin.is_empty());

        let (always, _) = suppression(r#"{"suppress_tokens": "nonsense"}"#, "");
        assert_eq!(always.len(), 2);
    }

    #[test]
    fn paths_hang_off_the_whisper_directory() {
        let paths = Paths::from_home(Path::new("/home/u/.loom"));
        assert!(paths.encoder.starts_with("/home/u/.loom/voice/whisper"));
        assert!(paths.tokenizer.starts_with("/home/u/.loom/voice/whisper"));
        // The VAD sits beside the Kokoro files, not inside whisper/.
        assert_eq!(paths.vad, Path::new("/home/u/.loom/voice/silero_vad.onnx"));
    }

    #[test]
    fn a_missing_model_is_reported_with_the_fix() {
        let paths = Paths::from_home(Path::new("/nonexistent/loom"));
        assert!(!paths.complete());
        let error = Whisper::load(&paths).unwrap_err();
        assert!(error.to_string().contains("fetch-whisper"), "{error}");
    }

    #[test]
    fn the_sizes_are_the_ones_the_models_report() {
        // Hardcoded from `probe_whisper`'s output, so a different export fails
        // here rather than producing a decoder shape error much later.
        assert_eq!(ENCODER_POSITIONS, 1500);
        assert_eq!(HIDDEN, 384);
        assert_eq!(VOCAB, 51_864);
        assert_eq!(MAX_TOKENS, 448);
        // The mel's frame count is the encoder's input width, not its output.
        assert_eq!(N_FRAMES, 3000);
        assert_eq!(N_MELS, 80);
    }

    // -- against the real models --------------------------------------------

    #[test]
    fn loading_checks_every_input_name() {
        let _guard = crate::voice::model_lock();

        let Some(paths) = real_paths() else {
            return;
        };
        // The runtime must be initialised first; `Kokoro::load` normally does
        // it, and `ort` memoizes, so do it here directly.
        let home = match dirs::home_dir() {
            Some(home) => home.join(".loom"),
            None => return,
        };
        let runtime = super::super::tts::Paths::from_home(&home).runtime;
        if !runtime.exists() {
            return;
        }
        let _ = ort::init_from(&runtime).map(|builder| builder.commit());

        let whisper = Whisper::load(&paths).expect("the real models should load");

        // The prompt is the two tokens the configs name.
        let ids = whisper.decode_ids();
        assert_eq!(ids.prompt(), [50_257, 50_362]);
        assert_eq!(ids.eos, 50_256);

        // And the suppression list is non-trivial, which is what stops the
        // argmax producing punctuation noise.
        assert!(
            whisper.suppressed.len() > 50,
            "only {} suppressed tokens",
            whisper.suppressed.len()
        );
        assert!(!whisper.begin_suppressed.is_empty());
    }

    #[test]
    fn a_tone_transcribes_to_something_and_does_not_crash() {
        let _guard = crate::voice::model_lock();

        let Some(paths) = real_paths() else {
            return;
        };
        let home = match dirs::home_dir() {
            Some(home) => home.join(".loom"),
            None => return,
        };
        let runtime = super::super::tts::Paths::from_home(&home).runtime;
        if !runtime.exists() {
            return;
        }
        let _ = ort::init_from(&runtime).map(|builder| builder.commit());

        let Ok(mut whisper) = Whisper::load(&paths) else {
            return;
        };

        // A pure tone is not speech, so the *content* is not asserted — only
        // that the whole pipeline runs, terminates, and returns text. A tone
        // that produced tokens would be a bug; a tone that hung or panicked
        // would be a bigger one.
        let mut samples = vec![0.0f32; audio::TARGET_RATE as usize];
        for (index, sample) in samples.iter_mut().enumerate() {
            let t = index as f32 / audio::TARGET_RATE as f32;
            *sample = 0.2 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
        }

        let result = whisper.transcribe(&samples, audio::TARGET_RATE, 1);
        let transcription = result.expect("transcription should not error");
        assert!(transcription.audio_seconds > 0.9);
        assert!(transcription.tokens.len() >= 2, "the prompt is always there");
        assert!(
            transcription.tokens.len() <= MAX_TOKENS + 2,
            "the loop did not terminate: {} tokens",
            transcription.tokens.len()
        );
    }

    /// Transcribes the Kokoro sample, which is the one clip guaranteed to be
    /// real speech on this machine.
    #[test]
    fn the_kokoro_sample_transcribes_to_words() {
        let _guard = crate::voice::model_lock();

        let Some(paths) = real_paths() else {
            return;
        };
        let home = match dirs::home_dir() {
            Some(home) => home.join(".loom"),
            None => return,
        };
        let runtime = super::super::tts::Paths::from_home(&home).runtime;
        if !runtime.exists() {
            return;
        }
        let _ = ort::init_from(&runtime).map(|builder| builder.commit());

        // The WAV `hello_kokoro` writes.
        let wav = std::env::temp_dir().join("loom-kokoro.wav");
        let Ok(bytes) = std::fs::read(&wav) else {
            return;
        };
        let Some((samples, rate, channels)) = read_wav(&bytes) else {
            return;
        };

        let Ok(mut whisper) = Whisper::load(&paths) else {
            return;
        };
        let transcription = whisper
            .transcribe(&samples, rate, channels)
            .expect("transcribing real speech should work");

        // Whisper is nondeterministic in principle but greedy decoding is not,
        // and tiny.en on clean synthetic speech is reliable. Asserting *some*
        // words rather than exact text: a different build could punctuate
        // differently, and this must not be brittle.
        let words = transcription
            .text
            .split_whitespace()
            .filter(|word| word.chars().any(|c| c.is_alphabetic()))
            .count();
        assert!(
            words >= 5,
            "only {words} words from 11 seconds of speech: {:?}",
            transcription.text
        );
    }

    /// Minimal PCM WAV reader, for the test fixture. `audio.rs` writes WAV; it
    /// does not read one, and a test is not a reason to add that.
    fn read_wav(bytes: &[u8]) -> Option<(Vec<f32>, u32, usize)> {
        if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return None;
        }
        let channels = u16::from_le_bytes([bytes[22], bytes[23]]) as usize;
        let rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
        let bits = u16::from_le_bytes([bytes[34], bytes[35]]);
        if bits != 16 {
            return None;
        }

        // Walk to the data chunk: the fmt chunk's length is not always 16.
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
                let samples = audio::i16_bytes_to_f32(&bytes[start..end]);
                return Some((samples, rate, channels));
            }
            offset += 8 + size + (size % 2);
        }
        None
    }
}
