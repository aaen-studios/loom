//! Transcribes a WAV file with Whisper, and prints what it heard.
//!
//! This is the speech-to-text loop end to end: a PCM WAV on disk, through the
//! mel front-end, the ONNX encoder, the greedy decoder, and the tokenizer. It
//! exists to answer one question that no unit test can — *does it produce the
//! words that were said?*
//!
//! ```text
//! cargo run -p loom-core --example transcribe
//! cargo run -p loom-core --example transcribe -- --wav path/to/audio.wav
//! cargo run -p loom-core --example transcribe -- --builtin
//! ```
//!
//! `--builtin` synthesises a sentence with Kokoro first, so the input is known
//! exactly: any word the transcriber gets wrong is its own fault rather than a
//! recording artefact.

use std::path::PathBuf;

use loom_core::voice::audio;
use loom_core::voice::{espeak, tts, whisper};

/// The sentence `--builtin` speaks. Ordinary words, a comma and a full stop, so
/// there is punctuation to lose and a sentence break to get wrong.
const SPOKEN: &str = "The quick brown fox jumps over the lazy dog. \
                      Loom runs every model on this machine, with no network at all.";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| -> Option<String> {
        args.iter()
            .position(|arg| arg == name)
            .and_then(|index| args.get(index + 1))
            .cloned()
    };

    let home = loom_core::paths::loom_home()?;
    let whisper_paths = whisper::Paths::from_home(&home);
    let tts_paths = tts::Paths::from_home(&home);
    let espeak_paths = espeak::Paths::from_home(&home);

    // The ONNX Runtime has to be initialised once before any session loads, and
    // either model can do it.
    if !tts_paths.runtime.exists() {
        eprintln!(
            "ONNX Runtime is missing at {}. Run: python scripts/setup-voice.py",
            tts_paths.runtime.display()
        );
        return Err("no runtime".into());
    }

    if !whisper_paths.complete() {
        eprintln!("The Whisper models are missing. Run: python scripts/fetch-whisper.py");
        return Err("no models".into());
    }

    // Where the audio comes from: an argument, the built-in synthesis, or the
    // WAV `hello_kokoro` leaves behind.
    let builtin = args.iter().any(|arg| arg == "--builtin");
    let wav: PathBuf = match flag("--wav") {
        Some(path) => PathBuf::from(path),
        // Synthesise into a path of our own, rather than overwriting the file
        // `hello_kokoro` writes — a caller may want to compare against it.
        None if builtin => std::env::temp_dir().join("loom-transcribe-builtin.wav"),
        None => std::env::temp_dir().join("loom-kokoro.wav"),
    };

    if builtin {
        println!("synthesising the test sentence with Kokoro...");
        let mut kokoro = tts::Kokoro::load(&tts_paths, &espeak_paths)?;
        let synthesis = kokoro.speak(SPOKEN, tts::Kokoro::DEFAULT_VOICE, 1.0)?;
        synthesis.audio.write_wav(&wav)?;
        println!(
            "  wrote {:.2} s to {}",
            synthesis.audio.duration(),
            wav.display()
        );
        println!("  expected: {SPOKEN}");
    }

    let bytes = std::fs::read(&wav).map_err(|e| format!("could not read {}: {e}", wav.display()))?;
    let (samples, rate, channels) =
        read_wav(&bytes).ok_or_else(|| format!("{} is not a 16-bit PCM WAV", wav.display()))?;

    println!();
    println!("file           : {}", wav.display());
    println!(
        "audio          : {:.2} s, {} Hz, {} channel(s)",
        samples.len() as f32 / rate as f32 / channels as f32,
        rate,
        channels
    );
    println!();

    println!("loading Whisper...");
    let started = std::time::Instant::now();
    let mut engine = whisper::Whisper::load(&whisper_paths)?;
    println!("  loaded in {:.2} s", started.elapsed().as_secs_f32());
    println!();

    println!("transcribing...");
    let transcription = engine.transcribe(&samples, rate, channels)?;

    println!();
    println!("transcript     : {}", transcription.text);
    println!();
    println!("tokens         : {}", transcription.text_tokens());
    println!("compute        : {:.2} s", transcription.compute_seconds);
    println!(
        "real time      : {}",
        if transcription.real_time_factor() < 1.0 {
            format!(
                "{:.1}x faster than real time",
                1.0 / transcription.real_time_factor().max(1e-6)
            )
        } else {
            format!("{:.2}x slower than real time", transcription.real_time_factor())
        }
    );

    // A word-level comparison when the input is known, because "it produced
    // text" and "it produced the right text" are different claims.
    if builtin {
        println!();
        let expected: Vec<String> = words(SPOKEN);
        let got: Vec<String> = words(&transcription.text);
        let matched = expected
            .iter()
            .filter(|word| got.contains(word))
            .count();
        let accuracy = matched as f32 / expected.len() as f32;

        println!(
            "word match     : {matched} of {} ({:.0}%)",
            expected.len(),
            accuracy * 100.0
        );

        let missing: Vec<&String> = expected
            .iter()
            .filter(|word| !got.contains(word))
            .collect();
        if missing.is_empty() {
            println!("                 every expected word was heard");
        } else {
            println!("                 missed: {missing:?}");
        }
    } else {
        println!();
        println!("No --builtin, so there is nothing to compare against. To check");
        println!("accuracy against known text:");
        println!("  cargo run -p loom-core --example transcribe -- --builtin");
    }

    Ok(())
}

/// Lowercased words with punctuation stripped, for comparison.
fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(|c| c.to_lowercase())
                .collect::<String>()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

/// Minimal PCM WAV reader, matching what `write_wav` produces.
///
/// `audio.rs` writes WAV and does not read one; a caller with a file from
/// elsewhere needs the reading half, so it lives here rather than in the
/// library.
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

    // The fmt chunk's declared length is not always 16, so walk the chunks
    // rather than assuming a fixed offset.
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
        // Chunks are word-aligned.
        offset += 8 + size + (size % 2);
    }
    None
}
