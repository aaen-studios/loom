//! Kokoro inference: phonemes in, samples out.
//!
//! # The shape of a request
//!
//! The ONNX graph takes three inputs and returns one, plus durations when the
//! export reports them:
//!
//! | input | shape | meaning |
//! |---|---|---|
//! | `input_ids` (or `tokens`) | `(1, n + 2)` | phoneme ids, padded with 0 **both** sides |
//! | `style` | `(1, 256)` | one row of the voice's style matrix |
//! | `speed` | `(1,)` | playback rate, 0.5–2.0 |
//!
//! | output | meaning |
//! |---|---|
//! | 0 | audio, `f32`, at 24 kHz |
//! | 1 | per-phoneme frame durations, when present |
//!
//! The token input's *name* varies between exports — `input_ids` on newer ones,
//! `tokens` on older — so it is read from the session rather than hardcoded.
//! Getting that wrong is not a compile error and not a runtime error either; it
//! is a "failed to find input" that only shows up against a real model.
//!
//! # Why synthesis takes `&mut self`
//!
//! `ort::Session::run` requires exclusive access, so this type cannot be shared
//! behind an `&`. That is not a limitation worth working around: voice mode runs
//! synthesis on one dedicated blocking thread behind a queue, precisely so that
//! a slow sentence cannot stall token generation. One owner, one thread, no
//! interior mutability needed.
//!
//! # Why the runtime is loaded by path
//!
//! `ort` is configured with `load-dynamic`, so ONNX Runtime is loaded from a
//! file at run time instead of being linked at build time. That is what keeps
//! the installer small, and it makes a missing runtime a reportable error rather
//! than a binary that will not start.

use std::path::{Path, PathBuf};

use ort::session::Session;
use ort::value::Tensor;

use super::chunk::{self, StreamChunker};
use super::{clean, espeak, manifest, phonemes, voices::Voices};
use crate::{Error, Result};

/// The name newer exports give the token input.
const TOKENS_NEW: &str = "input_ids";
/// The name older exports give it.
const TOKENS_OLD: &str = "tokens";

/// A block of mono audio.
#[derive(Debug, Clone, PartialEq)]
pub struct Audio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl Audio {
    /// A block at the model's fixed sample rate.
    pub fn silent(sample_rate: u32) -> Self {
        Self {
            samples: Vec::new(),
            sample_rate,
        }
    }

    /// Seconds of audio.
    pub fn duration(&self) -> f32 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.samples.len() as f32 / self.sample_rate as f32
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// The largest absolute sample, which is how "did this actually produce
    /// sound" gets answered without listening.
    pub fn peak(&self) -> f32 {
        self.samples.iter().fold(0.0f32, |acc, s| acc.max(s.abs()))
    }

    /// Appends `seconds` of silence.
    fn push_silence(&mut self, seconds: f32) {
        let count = (seconds * self.sample_rate as f32).round() as usize;
        self.samples.extend(std::iter::repeat_n(0.0, count));
    }

    /// Writes 16-bit mono PCM as a WAV file.
    ///
    /// Hand-rolled rather than pulled in: a WAV header is 44 bytes, and the
    /// alternative is a dependency for those 44 bytes. The graph emits `f32`,
    /// but 16-bit is what every player opens without complaint.
    pub fn write_wav(&self, path: &Path) -> Result<()> {
        let bytes = self.to_wav_bytes();
        std::fs::write(path, bytes).map_err(|e| Error::io(path, e))
    }

    /// The same bytes `write_wav` writes, without touching the filesystem.
    ///
    /// The UI plays audio from memory — a data URL per sentence — so the
    /// encoding has to be available without a temporary file per chunk.
    pub fn to_wav_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(44 + self.samples.len() * 2);

        let data_len = (self.samples.len() * 2) as u32;
        let channels: u16 = 1;
        let bits: u16 = 16;
        let byte_rate = self.sample_rate * channels as u32 * (bits / 8) as u32;
        let block_align = channels * (bits / 8);

        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&self.sample_rate.to_le_bytes());
        bytes.extend_from_slice(&byte_rate.to_le_bytes());
        bytes.extend_from_slice(&block_align.to_le_bytes());
        bytes.extend_from_slice(&bits.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());

        for sample in &self.samples {
            // Clamp before scaling: a value outside [-1, 1] would wrap to a
            // large value of the opposite sign, which is an audible click.
            let clamped = sample.clamp(-1.0, 1.0);
            let scaled = (clamped * i16::MAX as f32).round() as i16;
            bytes.extend_from_slice(&scaled.to_le_bytes());
        }

        bytes
    }
}

/// Where the runtime and models live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub runtime: PathBuf,
    pub model: PathBuf,
    pub voices: PathBuf,
}

impl Paths {
    /// Resolves everything from the Loom home directory.
    ///
    /// ONNX Runtime is not in `voice/` with the models: it has its own
    /// directory because its correct build depends on the host CPU, so it may
    /// be supplied by an installer or by the user rather than downloaded.
    pub fn from_home(home: &Path) -> Self {
        let voice = home.join("voice");
        Self {
            runtime: runtime_library(&home.join("ort")),
            model: voice.join("kokoro-v1.0.onnx"),
            voices: voice.join("voices-v1.0.bin"),
        }
    }

    /// The paths this build will actually use, honouring `LOOM_HOME`.
    pub fn resolve() -> Result<Self> {
        Ok(Self::from_home(&crate::paths::loom_home()?))
    }

    /// The runtime's containing directory, which the loader may need.
    pub fn runtime_dir(&self) -> Option<PathBuf> {
        self.runtime.parent().map(Path::to_path_buf)
    }
}

/// The first ONNX Runtime library name that exists under `root`.
///
/// The name is platform-specific, and on Windows `onnxruntime.dll` may sit
/// either directly in the directory or in a versioned `lib/` subdirectory —
/// which is how the official release archive is laid out, so both are checked.
fn runtime_library(root: &Path) -> PathBuf {
    let names: &[&str] = if cfg!(windows) {
        &["onnxruntime.dll"]
    } else if cfg!(target_os = "macos") {
        &["libonnxruntime.dylib"]
    } else {
        &["libonnxruntime.so"]
    };

    for name in names {
        let direct = root.join(name);
        if direct.exists() {
            return direct;
        }
        // The release archive extracts to onnxruntime-<platform>-<version>/lib/.
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let nested = entry.path().join("lib").join(name);
                if nested.exists() {
                    return nested;
                }
            }
        }
    }

    root.join(names[0])
}

/// What is and is not present, for the settings screen to report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Availability {
    pub runtime: bool,
    pub model: bool,
    pub voices: bool,
    pub phonemizer: bool,
}

impl Availability {
    /// Whether synthesis can actually run.
    pub fn ready(&self) -> bool {
        self.runtime && self.model && self.voices && self.phonemizer
    }

    /// The first missing piece, in the order it has to be fixed.
    pub fn blocking(&self) -> Option<&'static str> {
        if !self.phonemizer {
            return Some("espeak-ng is not installed");
        }
        if !self.runtime {
            return Some("ONNX Runtime is not installed");
        }
        if !self.model {
            return Some("the voice model is not downloaded");
        }
        if !self.voices {
            return Some("the voices file is not downloaded");
        }
        None
    }

    /// Checks the filesystem without loading anything.
    pub fn probe(paths: &Paths, espeak_paths: &espeak::Paths) -> Self {
        Self {
            runtime: paths.runtime.exists(),
            model: paths.model.exists(),
            voices: paths.voices.exists(),
            phonemizer: espeak_paths.present(),
        }
    }
}

/// How a single phrase was produced, for logging and for benchmarks.
#[derive(Debug, Clone, PartialEq)]
pub struct Synthesis {
    pub audio: Audio,
    /// Wall-clock seconds spent inside the graph.
    pub compute_seconds: f64,
    /// Batches the text was split into.
    pub batches: usize,
}

impl Synthesis {
    /// Real-time factor: below 1.0 is faster than playback.
    pub fn real_time_factor(&self) -> f32 {
        let duration = self.audio.duration();
        if duration <= 0.0 {
            return 0.0;
        }
        (self.compute_seconds as f32) / duration
    }
}

/// A loaded Kokoro model.
///
/// Loading is expensive — the graph is 325 MB — so it happens once and the
/// session stays resident. Synthesis takes `&mut self` because `ort` requires
/// exclusive access to a session; see the module note.
pub struct Kokoro {
    session: Session,
    voices: Voices,
    token_input: &'static str,
    espeak: espeak::Paths,
}

impl std::fmt::Debug for Kokoro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kokoro")
            .field("token_input", &self.token_input)
            .field("voices", &self.voices.len())
            .field("espeak", &self.espeak.library)
            .finish_non_exhaustive()
    }
}

impl Kokoro {
    /// The default voice: the one the model card names first and grades highest.
    pub const DEFAULT_VOICE: &'static str = "af_heart";

    /// Points ONNX Runtime at a specific library and loads the model.
    ///
    /// `ort`'s runtime must be initialized before any other `ort` use in the
    /// process. Initializing twice is harmless here because `ort` memoizes it,
    /// but a *different* path the second time is not honoured — which is why the
    /// path is taken explicitly rather than read from a global.
    pub fn load(paths: &Paths, espeak_paths: &espeak::Paths) -> Result<Self> {
        if !paths.runtime.exists() {
            return Err(Error::Http(format!(
                "ONNX Runtime not found at {}. Run scripts/setup-voice.py.",
                paths.runtime.display()
            )));
        }
        if !paths.model.exists() {
            return Err(Error::Http(format!(
                "voice model not found at {}",
                paths.model.display()
            )));
        }
        if !paths.voices.exists() {
            return Err(Error::Http(format!(
                "voices file not found at {}",
                paths.voices.display()
            )));
        }
        if !espeak_paths.present() {
            return Err(Error::Http(format!(
                "espeak-ng not found under {}. Run scripts/setup-voice.py.",
                espeak_paths.data_root.display()
            )));
        }

        // `load-dynamic` means the library is loaded from this path rather than
        // linked, so a wrong path is a clean error instead of a load failure.
        //
        // `init_from` returns a builder; `commit` on it returns `bool`, not a
        // `Result` — it reports whether *this* call performed the initialization,
        // since `ort` memoizes it and a second call is a no-op rather than a
        // failure. A missing or incompatible library surfaces as the `Err` from
        // `init_from` itself.
        ort::init_from(&paths.runtime)
            .map_err(|e| {
                Error::Http(format!(
                    "could not load ONNX Runtime from {}: {e}",
                    paths.runtime.display()
                ))
            })?
            .commit();

        let session = Session::builder()
            .map_err(|e| Error::Http(format!("could not create a session: {e}")))?
            .commit_from_file(&paths.model)
            .map_err(|e| {
                Error::Http(format!("could not load {}: {e}", paths.model.display()))
            })?;

        // The token input's name varies by export. Reading it from the session
        // rather than hardcoding it means an older export still loads.
        let token_input = if session.inputs().iter().any(|i| i.name() == TOKENS_NEW) {
            TOKENS_NEW
        } else if session.inputs().iter().any(|i| i.name() == TOKENS_OLD) {
            TOKENS_OLD
        } else {
            let found: Vec<String> = session.inputs().iter().map(|i| i.name().to_string()).collect();
            return Err(Error::Http(format!(
                "the model has no token input; expected {TOKENS_NEW:?} or {TOKENS_OLD:?}, found {found:?}"
            )));
        };

        let voices = Voices::load(&paths.voices)?;

        Ok(Self {
            session,
            voices,
            token_input,
            espeak: espeak_paths.clone(),
        })
    }

    /// The token input name this model actually uses.
    pub fn token_input(&self) -> &str {
        self.token_input
    }

    /// The language a voice speaks, which the settings UI groups by.
    pub fn language_of(voice: &str) -> Option<espeak::Lang> {
        espeak::Lang::from_voice_id(voice)
    }

    /// How many voices are available.
    pub fn voice_count(&self) -> usize {
        self.voices.len()
    }

    /// Whether a voice exists.
    pub fn has_voice(&self, voice: &str) -> bool {
        self.voices.contains(voice)
    }

    /// Synthesizes `text` in `voice`.
    pub fn speak(&mut self, text: &str, voice: &str, speed: f32) -> Result<Synthesis> {
        if !(0.5..=2.0).contains(&speed) {
            return Err(Error::Http(format!(
                "speed must be between 0.5 and 2.0, got {speed}"
            )));
        }
        if !self.voices.contains(voice) {
            return Err(Error::Http(format!(
                "voice {voice:?} is not in the voices file"
            )));
        }

        let empty = Synthesis {
            audio: Audio::silent(manifest::SAMPLE_RATE),
            compute_seconds: 0.0,
            batches: 0,
        };

        // Markdown first: a reply is written for the eye, and the model would
        // otherwise read a URL out one character at a time.
        let prose = clean::for_speech(text);
        if prose.trim().is_empty() {
            return Ok(empty);
        }

        let lang = espeak::Lang::from_voice_id(voice).ok_or_else(|| {
            Error::Http(format!("no phonemizer language is known for voice {voice:?}"))
        })?;

        let phonemes = espeak::phonemize(&prose, lang, &self.espeak)
            .map_err(|e| Error::Http(e.to_string()))?;
        if phonemes.trim().is_empty() {
            return Ok(empty);
        }

        let batches = chunk::split_phonemes(&phonemes, manifest::MAX_PHONEME_LENGTH);
        if batches.is_empty() {
            return Ok(empty);
        }

        let style = self
            .voices
            .get(voice)
            .ok_or_else(|| Error::Http(format!("voice {voice:?} is missing")))?
            .clone();

        let mut audio = Audio::silent(manifest::SAMPLE_RATE);        let mut compute = 0.0f64;
        let count = batches.len();

        for (index, batch) in batches.iter().enumerate() {
            let tokens = phonemes::tokenize(batch);
            if tokens.is_empty() {
                continue;
            }

            let start = std::time::Instant::now();
            let samples = self.run(&tokens, style.row(tokens.len()), speed)?;
            compute += start.elapsed().as_secs_f64();

            // Trim the silence the graph leaves at a batch boundary, then add
            // back the pause the punctuation calls for — otherwise every
            // sentence runs into the next with no gap at all.
            audio
                .samples
                .extend_from_slice(trim_silence(&samples, TRIM_THRESHOLD));

            if index + 1 < count {
                audio.push_silence(chunk::pause_after(batch, SENTENCE_PAUSE, CLAUSE_PAUSE));
            }
        }

        Ok(Synthesis {
            audio,
            compute_seconds: compute,
            batches: count,
        })
    }

    /// Runs one batch through the graph.
    fn run(&mut self, tokens: &[i64], style: &[f32], speed: f32) -> Result<Vec<f32>> {
        let padded = phonemes::padded_row(tokens);
        let width = padded.len();

        let ids = Tensor::from_array(([1usize, width], padded))
            .map_err(|e| Error::Http(format!("could not build the token tensor: {e}")))?;
        let style_tensor = Tensor::from_array(([1usize, style.len()], style.to_vec()))
            .map_err(|e| Error::Http(format!("could not build the style tensor: {e}")))?;
        let speed_tensor = Tensor::from_array(([1usize], vec![speed]))
            .map_err(|e| Error::Http(format!("could not build the speed tensor: {e}")))?;

        let outputs = self
            .session
            .run(ort::inputs![
                self.token_input => ids,
                "style" => style_tensor,
                "speed" => speed_tensor,
            ])
            .map_err(|e| Error::Http(format!("inference failed: {e}")))?;

        let (_, audio) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| Error::Http(format!("could not read the audio output: {e}")))?;

        Ok(audio.to_vec())
    }

    /// Synthesizes a whole reply, emitting audio as each sentence completes.
    ///
    /// This is the latency lever: the first sentence is spoken while the model
    /// is still writing the third, so time to first audio depends on the
    /// chunker rather than on how fast the reply finishes. The callback returns
    /// `false` to stop, which is how barge-in cancels a reply mid-sentence.
    pub fn speak_stream<'a, F>(&'a mut self, voice: &str, on_audio: F) -> Result<SpeakStream<'a>>
    where
        F: FnMut(Audio) -> bool + 'a,
    {
        if !self.voices.contains(voice) {
            return Err(Error::Http(format!(
                "voice {voice:?} is not in the voices file"
            )));
        }
        let lang = espeak::Lang::from_voice_id(voice)
            .ok_or_else(|| Error::Http(format!("no phonemizer language for {voice:?}")))?;
        let style = self
            .voices
            .get(voice)
            .ok_or_else(|| Error::Http(format!("voice {voice:?} is missing")))?
            .clone();

        Ok(SpeakStream {
            engine: self,
            style,
            lang,
            chunker: StreamChunker::new(),
            sink: Box::new(on_audio),
        })
    }
}

/// Push-based streaming synthesis.
///
/// Text is fed in as the model writes it, and complete sentences are
/// synthesised as soon as they are available. Fenced code is dropped by the
/// chunker, so a code block is never read aloud.
pub struct SpeakStream<'a> {
    engine: &'a mut Kokoro,
    style: super::voices::Style,
    lang: espeak::Lang,
    chunker: StreamChunker,
    sink: Box<dyn FnMut(Audio) -> bool + 'a>,
}

impl SpeakStream<'_> {
    /// Feeds a delta of reply text. Returns how many chunks were spoken.
    ///
    /// Stops early — returning fewer than the number of complete sentences —
    /// when the sink asks to stop, which is barge-in.
    pub fn push(&mut self, delta: &str) -> Result<usize> {
        let chunks = self.chunker.push(delta);
        let mut spoken = 0;
        for chunk in chunks {
            if self.speak_chunk(&chunk)? {
                spoken += 1;
            } else {
                break;
            }
        }
        Ok(spoken)
    }

    /// Ends the stream, speaking whatever prose is left.
    pub fn finish(&mut self) -> Result<usize> {
        match self.chunker.finish() {
            Some(tail) => Ok(usize::from(self.speak_chunk(&tail)?)),
            None => Ok(0),
        }
    }

    /// Synthesizes one chunk and hands it to the sink. `false` means stop.
    fn speak_chunk(&mut self, text: &str) -> Result<bool> {
        let phonemes = match espeak::phonemize(text, self.lang, &self.engine.espeak) {
            Ok(phonemes) => phonemes,
            // A phonemizer that cannot handle one fragment should not kill the
            // reply; the rest of it is still worth speaking.
            Err(error) => {
                eprintln!("[loom] voice: phonemization failed for a chunk: {error}");
                return Ok(true);
            }
        };
        if phonemes.trim().is_empty() {
            return Ok(true);
        }

        let batches = chunk::split_phonemes(&phonemes, manifest::MAX_PHONEME_LENGTH);
        if batches.is_empty() {
            return Ok(true);
        }

        let mut audio = Audio::silent(manifest::SAMPLE_RATE);
        let count = batches.len();

        for (index, batch) in batches.iter().enumerate() {
            let tokens = phonemes::tokenize(batch);
            if tokens.is_empty() {
                continue;
            }
            let samples =
                self.engine
                    .run(&tokens, self.style.row(tokens.len()), 1.0)?;
            audio
                .samples
                .extend_from_slice(trim_silence(&samples, TRIM_THRESHOLD));
            if index + 1 < count {
                audio.push_silence(chunk::pause_after(batch, SENTENCE_PAUSE, CLAUSE_PAUSE));
            }
        }

        if audio.is_empty() {
            return Ok(true);
        }
        Ok((self.sink)(audio))
    }
}

/// Peak below which a sample is treated as silence.
///
/// Deliberately simple. The reference implementation ships a spectral trimmer;
/// this uses a peak threshold, which is enough to remove the flat silence the
/// graph emits at a batch boundary and cheap enough to run per sentence. If
/// seams turn out to be audible, this is the function to improve.
const TRIM_THRESHOLD: f32 = 0.005;

/// Pause inserted after a sentence, in seconds.
const SENTENCE_PAUSE: f32 = 0.25;
/// Pause inserted after a clause.
const CLAUSE_PAUSE: f32 = 0.1;

/// Removes leading and trailing near-silence.
///
/// Returns the whole slice when nothing crosses the threshold, rather than an
/// empty one: a batch that is genuinely quiet is better heard than dropped.
fn trim_silence(samples: &[f32], threshold: f32) -> &[f32] {
    let Some(first) = samples.iter().position(|s| s.abs() > threshold) else {
        return samples;
    };
    let last = samples
        .iter()
        .rposition(|s| s.abs() > threshold)
        .unwrap_or(first);
    &samples[first..=last]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::create_dir_all(&path);
        path
    }

    // -- paths --------------------------------------------------------------

    #[test]
    fn paths_hang_off_the_home_directory() {
        let paths = Paths::from_home(Path::new("/home/u/.loom"));
        assert!(paths.model.starts_with("/home/u/.loom/voice"));
        assert!(paths.voices.starts_with("/home/u/.loom/voice"));
        // The runtime gets its own directory: its build depends on the CPU, so
        // it may be supplied rather than downloaded.
        assert!(paths.runtime.starts_with("/home/u/.loom/ort"));
    }

    #[test]
    fn the_runtime_name_matches_the_platform() {
        let paths = Paths::from_home(Path::new("/nonexistent"));
        let name = paths.runtime.file_name().unwrap().to_string_lossy().to_string();
        if cfg!(windows) {
            assert_eq!(name, "onnxruntime.dll");
        } else if cfg!(target_os = "macos") {
            assert_eq!(name, "libonnxruntime.dylib");
        } else {
            assert_eq!(name, "libonnxruntime.so");
        }
    }

    #[test]
    fn a_nested_runtime_directory_is_found() {
        // The official release archive extracts to onnxruntime-<platform>-<v>/lib/,
        // not flat, so a flat-only lookup would miss it.
        let root = dir("loom-voice-nested-runtime");
        let nested = root.join("onnxruntime-win-x64-1.22.0").join("lib");
        let _ = std::fs::create_dir_all(&nested);
        let name = if cfg!(windows) {
            "onnxruntime.dll"
        } else if cfg!(target_os = "macos") {
            "libonnxruntime.dylib"
        } else {
            "libonnxruntime.so"
        };
        let _ = std::fs::write(nested.join(name), b"stub");

        let found = runtime_library(&root);
        assert!(found.exists(), "nested runtime was not found at {found:?}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_flat_runtime_directory_is_found() {
        let root = dir("loom-voice-flat-runtime");
        let name = if cfg!(windows) {
            "onnxruntime.dll"
        } else if cfg!(target_os = "macos") {
            "libonnxruntime.dylib"
        } else {
            "libonnxruntime.so"
        };
        let _ = std::fs::write(root.join(name), b"stub");
        assert!(runtime_library(&root).exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    // -- availability -------------------------------------------------------

    #[test]
    fn availability_reports_the_first_blocker_in_fix_order() {
        let missing = Availability {
            runtime: false,
            model: false,
            voices: false,
            phonemizer: false,
        };
        assert!(!missing.ready());
        // The phonemizer comes first: without it nothing else can be used.
        assert!(missing.blocking().unwrap().contains("espeak-ng"));

        let no_runtime = Availability {
            phonemizer: true,
            ..missing.clone()
        };
        assert!(no_runtime.blocking().unwrap().contains("ONNX Runtime"));

        let only_voices_missing = Availability {
            runtime: true,
            model: true,
            voices: false,
            phonemizer: true,
        };
        assert!(only_voices_missing.blocking().unwrap().contains("voices"));

        let complete = Availability {
            runtime: true,
            model: true,
            voices: true,
            phonemizer: true,
        };
        assert!(complete.ready());
        assert!(complete.blocking().is_none());
    }

    #[test]
    fn probing_an_empty_home_reports_nothing_present() {
        let root = dir("loom-voice-probe-absent");
        let availability = Availability::probe(
            &Paths::from_home(&root),
            &espeak::Paths::from_home(&root),
        );
        assert!(!availability.runtime);
        assert!(!availability.model);
        assert!(!availability.voices);
        assert!(!availability.phonemizer);
        assert!(!availability.ready());
        let _ = std::fs::remove_dir_all(&root);
    }

    // -- trimming -----------------------------------------------------------

    #[test]
    fn trimming_removes_silence_at_both_ends() {
        let mut samples = vec![0.0f32; 10];
        samples.extend_from_slice(&[0.5, -0.5, 0.5]);
        samples.extend_from_slice(&[0.0f32; 10]);

        let trimmed = trim_silence(&samples, 0.005);
        assert_eq!(trimmed.len(), 3);
        assert_eq!(trimmed[0], 0.5);
    }

    #[test]
    fn trimming_keeps_a_quiet_clip_rather_than_emptying_it() {
        // Nothing crosses the threshold. Returning an empty slice would drop
        // audio that is merely soft.
        let samples = vec![0.0f32; 100];
        assert_eq!(trim_silence(&samples, 0.005).len(), 100);
    }

    #[test]
    fn trimming_an_empty_slice_is_harmless() {
        assert!(trim_silence(&[], 0.005).is_empty());
    }

    #[test]
    fn trimming_keeps_a_single_sample_that_crosses() {
        let samples = vec![0.0, 0.0, 0.9, 0.0];
        assert_eq!(trim_silence(&samples, 0.005), &[0.9]);
    }

    // -- audio --------------------------------------------------------------

    #[test]
    fn duration_follows_the_sample_rate() {
        let audio = Audio {
            samples: vec![0.0; 24_000],
            sample_rate: 24_000,
        };
        assert_eq!(audio.duration(), 1.0);
        assert!(!audio.is_empty());
    }

    #[test]
    fn a_zero_sample_rate_does_not_divide_by_zero() {
        let audio = Audio {
            samples: vec![0.0; 10],
            sample_rate: 0,
        };
        assert_eq!(audio.duration(), 0.0);
    }

    #[test]
    fn peak_finds_the_loudest_sample() {
        let audio = Audio {
            samples: vec![0.1, -0.8, 0.3],
            sample_rate: 24_000,
        };
        assert!((audio.peak() - 0.8).abs() < 1e-6);
        assert_eq!(Audio::silent(24_000).peak(), 0.0);
    }

    #[test]
    fn silence_is_pushed_at_the_right_length() {
        let mut audio = Audio::silent(24_000);
        audio.push_silence(0.25);
        assert_eq!(audio.samples.len(), 6_000);
        assert!(audio.samples.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn a_wav_header_describes_what_follows_it() {
        let root = dir("loom-voice-wav-test");
        let path = root.join("out.wav");

        let audio = Audio {
            samples: vec![0.0, 0.5, -0.5, 1.0],
            sample_rate: 24_000,
        };
        audio.write_wav(&path).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(&bytes[36..40], b"data");

        // 16-bit mono: four samples, two bytes each.
        assert_eq!(bytes.len(), 44 + 8);
        let declared = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]);
        assert_eq!(declared as usize, 8);

        let rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
        assert_eq!(rate, 24_000);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn out_of_range_samples_are_clamped_rather_than_wrapped() {
        let root = dir("loom-voice-wav-clamp");
        let path = root.join("loud.wav");

        let audio = Audio {
            // A value past full scale would wrap to a large value of the
            // opposite sign, which is an audible click.
            samples: vec![5.0, -5.0],
            sample_rate: 24_000,
        };
        audio.write_wav(&path).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let first = i16::from_le_bytes([bytes[44], bytes[45]]);
        let second = i16::from_le_bytes([bytes[46], bytes[47]]);
        assert_eq!(first, i16::MAX);
        assert_eq!(second, -i16::MAX);

        let _ = std::fs::remove_dir_all(&root);
    }

    // -- synthesis accounting ----------------------------------------------

    #[test]
    fn the_real_time_factor_is_compute_over_duration() {
        let synthesis = Synthesis {
            audio: Audio {
                samples: vec![0.0; 96_000], // four seconds
                sample_rate: 24_000,
            },
            compute_seconds: 0.5,
            batches: 1,
        };
        // Half a second of work for four seconds of speech.
        assert!((synthesis.real_time_factor() - 0.125).abs() < 1e-6);
    }

    #[test]
    fn real_time_factor_of_nothing_is_zero_rather_than_infinite() {
        let synthesis = Synthesis {
            audio: Audio::silent(24_000),
            compute_seconds: 1.0,
            batches: 0,
        };
        assert_eq!(synthesis.real_time_factor(), 0.0);
    }

    #[test]
    fn the_default_voice_is_a_documented_one() {
        assert_eq!(Kokoro::DEFAULT_VOICE, "af_heart");
    }

    // -- against the real files --------------------------------------------

    /// Everything needed for a real synthesis, when it is on disk.
    ///
    /// Deliberately **not** `Paths::resolve()`: that reads the process-global
    /// `LOOM_HOME`, which other tests in this crate point at temporary
    /// directories, so these tests would resolve to `/nonexistent/loom/...` and
    /// fail for reasons that have nothing to do with voice mode. The real
    /// installation is always under the user's home directory.
    fn assets() -> Option<(Paths, espeak::Paths)> {        let home = dirs::home_dir()?.join(".loom");
        let paths = Paths::from_home(&home);
        let espeak_paths = espeak::Paths::from_home(&home);
        let availability = Availability::probe(&paths, &espeak_paths);
        availability.ready().then_some((paths, espeak_paths))
    }

    #[test]
    fn the_runtime_and_models_are_found_when_present() {
        let Some(home) = dirs::home_dir().map(|home| home.join(".loom")) else {
            return;
        };
        let paths = Paths::from_home(&home);
        if !paths.model.exists() {
            return;
        }
        // The nested layout is what the release archive produces, so if the
        // model is here the runtime should be findable too.
        assert!(paths.runtime.exists(), "runtime not located at {:?}", paths.runtime);
        assert!(paths.runtime_dir().is_some());
    }

    /// Loads the model and speaks. Silently returns when the assets are absent,
    /// since none of that is a code fault — CI has no 325 MB model.
    #[test]
    fn speaks_when_everything_is_available() {
        // Serialised against the other model-loading tests: `ort` allows one
        // environment per process and sessions are hundreds of megabytes, so
        // several loading at once contends badly enough to fail. Production
        // has one worker thread and never does this.
        let _guard = crate::voice::model_lock();

        let Some((paths, espeak_paths)) = assets() else {
            return;
        };
        let Ok(mut engine) = Kokoro::load(&paths, &espeak_paths) else {
            return;
        };

        assert!(engine.has_voice(Kokoro::DEFAULT_VOICE));
        assert!(!engine.has_voice("af_nonexistent"));
        assert_eq!(engine.voice_count(), 54);
        assert!(
            engine.token_input() == TOKENS_NEW || engine.token_input() == TOKENS_OLD,
            "unexpected token input {:?}",
            engine.token_input()
        );

        let out = engine
            .speak("Hello there.", Kokoro::DEFAULT_VOICE, 1.0)
            .expect("synthesis failed");

        assert!(!out.audio.is_empty(), "no audio was produced");
        assert_eq!(out.audio.sample_rate, 24_000);
        // Speech, not noise: the samples must actually move, and must not clip.
        assert!(out.audio.peak() > 0.01, "the output is silent");
        assert!(out.audio.peak() <= 1.0, "the output clips");
        assert!(out.batches >= 1);

        // One short sentence should be far faster than real time.
        assert!(
            out.real_time_factor() < 5.0,
            "suspiciously slow: RTF {}",
            out.real_time_factor()
        );
    }

    #[test]
    fn an_unknown_voice_is_rejected_clearly() {
        let Some((paths, espeak_paths)) = assets() else {
            return;
        };
        let Ok(mut engine) = Kokoro::load(&paths, &espeak_paths) else {
            return;
        };
        let error = engine
            .speak("Hello.", "zz_not_a_voice", 1.0)
            .expect_err("an unknown voice must not synthesise");
        assert!(error.to_string().contains("not in the voices file"));
    }

    #[test]
    fn an_out_of_range_speed_is_rejected() {
        let Some((paths, espeak_paths)) = assets() else {
            return;
        };
        let Ok(mut engine) = Kokoro::load(&paths, &espeak_paths) else {
            return;
        };
        assert!(engine.speak("Hello.", Kokoro::DEFAULT_VOICE, 0.1).is_err());
        assert!(engine.speak("Hello.", Kokoro::DEFAULT_VOICE, 3.0).is_err());
    }

    #[test]
    fn empty_text_produces_no_audio_rather_than_failing() {
        let Some((paths, espeak_paths)) = assets() else {
            return;
        };
        let Ok(mut engine) = Kokoro::load(&paths, &espeak_paths) else {
            return;
        };
        let out = engine.speak("   ", Kokoro::DEFAULT_VOICE, 1.0).unwrap();
        assert!(out.audio.is_empty());
        assert_eq!(out.batches, 0);
    }

    #[test]
    fn streaming_stops_when_the_sink_says_stop() {
        let _guard = crate::voice::model_lock();

        let Some((paths, espeak_paths)) = assets() else {
            return;
        };
        let Ok(mut engine) = Kokoro::load(&paths, &espeak_paths) else {
            return;
        };

        // Refuse the first chunk, which is what barge-in does. The stream must
        // stop rather than synthesizing the rest of the reply.
        let mut seen = 0usize;
        {
            let mut stream = engine
                .speak_stream(Kokoro::DEFAULT_VOICE, |_audio| {
                    seen += 1;
                    false
                })
                .unwrap();
            stream.push("One. Two. Three. ").unwrap();
            stream.finish().unwrap();
        }
        assert_eq!(seen, 1, "the sink refused audio but synthesis continued");

        // And the streaming path produces the same kind of audio as `speak`.
        let mut collected = 0usize;
        {
            let mut stream = engine
                .speak_stream(Kokoro::DEFAULT_VOICE, |audio| {
                    assert!(!audio.is_empty());
                    assert_eq!(audio.sample_rate, 24_000);
                    collected += audio.samples.len();
                    true
                })
                .unwrap();
            stream.push("Hello there. This is a second sentence. ").unwrap();
            stream.finish().unwrap();
        }
        assert!(collected > 0, "streaming produced no audio at all");
    }
}
