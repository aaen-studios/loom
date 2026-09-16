//! Voice mode: Loom speaking, and listening.
//!
//! The pipeline is a chain rather than one model, because Kokoro is
//! decoder-only and cannot hear. Spoken input becomes text, text becomes a
//! normal Loom turn, and the reply is spoken back sentence by sentence while it
//! is still being written.
//!
//! ```text
//!   capture   getUserMedia + AudioWorklet, 16 kHz mono
//!   VAD       Silero
//!   STT       whisper
//!   engine    the existing turn: tools, memory, personas
//!   chunk     streamed tokens -> speakable sentences
//!   clean     markdown stripped before synthesis
//!   TTS       Kokoro, one voice per persona
//!   playback  WebAudio queue, flushed on barge-in
//! ```
//!
//! Everything below the engine row already exists in this crate. The modules
//! here are the new edges, and they are ordered so that the audio-free parts
//! can be built and tested before a single sample is synthesised.
//!
//! # Why some modules are pure
//!
//! [`vocab`], [`phonemes`], [`chunk`] and [`clean`] have no dependencies beyond
//! `std`. They are the parts most likely to be silently wrong — an off-by-one in
//! the phoneme vocabulary produces noise rather than an error — so they are
//! written to be testable in isolation, and are, before anything else is built
//! on them.
//!
//! [`manifest`] is the asset list. The downloader, the ONNX session, the
//! phonemizer and the microphone are deliberately absent: each needs a
//! decision that has not been verified yet, and guessing at them would put
//! untested code behind a tested interface.

pub mod assets;
pub mod audio;
pub mod chunk;
pub mod clean;
pub mod config;
pub mod dictate;
pub mod espeak;
pub mod install;
pub mod listen;
pub mod manifest;
pub mod mel;
pub mod phonemes;
pub mod tokenizer;
pub mod tts;
pub mod vocab;
pub mod voices;
pub mod vad;
pub mod whisper;

/// Serialises tests that load ONNX models.
///
/// `ort` allows one environment per process, and sessions are hundreds of
/// megabytes. Rust's test harness runs everything in parallel by default, so
/// several tests creating large sessions at once contends badly enough that a
/// session creation fails — which looks like a code bug and is not.
///
/// Production never does this: `src-tauri/src/voice.rs` owns one worker thread
/// and loads each model once. So the lock is test-only, and it exists to make
/// the suite's parallelism match the runtime's actual concurrency rather than
/// to hide a real problem.
#[cfg(test)]
pub(crate) fn model_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub use manifest::{MAX_PHONEME_LENGTH, SAMPLE_RATE};

/// Reads a value out of the voice configuration.
///
/// Placeholder for the config surface added alongside the downloader; kept here
/// so the module's public shape is visible before the settings exist.
pub const ENABLED_BY_DEFAULT: bool = true;
