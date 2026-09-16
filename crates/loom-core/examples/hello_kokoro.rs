//! Synthesizes a phrase and writes it to a WAV file.
//!
//! This is the Phase 0 gate made runnable: it either produces speech or it
//! produces noise, and either way it says so with numbers. Everything upstream
//! — the vocabulary, the phonemizer, the style matrix, the graph — is proven or
//! disproven by listening to the file this writes.
//!
//! ```text
//! python scripts/setup-voice.py
//! cargo run -p loom-core --example hello_kokoro
//! cargo run -p loom-core --example hello_kokoro -- --voice bf_emma --text "Good evening."
//! cargo run -p loom-core --example hello_kokoro -- --list
//! ```
//!
//! It also reports the real-time factor, which is the number that decides
//! whether a conversational pipeline is possible at all.

use std::path::PathBuf;

use loom_core::voice::espeak;
use loom_core::voice::tts::{Availability, Kokoro, Paths};

/// Long enough to hear prosody and a comma, short enough to iterate on.
const DEFAULT_TEXT: &str = "Hello. This is Loom speaking, with Kokoro — a small \
model that sounds far better than it has any right to. It runs entirely on your \
machine, and it never sends your text anywhere.";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    let voice = flag(&args, "--voice").unwrap_or_else(|| Kokoro::DEFAULT_VOICE.to_string());
    let text = flag(&args, "--text").unwrap_or_else(|| DEFAULT_TEXT.to_string());
    let out = flag(&args, "--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("loom-kokoro.wav"));
    let speed: f32 = match flag(&args, "--speed") {
        Some(value) => value.parse().unwrap_or(1.0),
        None => 1.0,
    };

    let home = loom_core::paths::loom_home()?;
    let paths = Paths::from_home(&home);
    let espeak_paths = espeak::Paths::from_home(&home);

    let availability = Availability::probe(&paths, &espeak_paths);

    if args.iter().any(|a| a == "--list") {
        println!("Loom home      : {}", home.display());
        println!();
        println!();
        println!("{:16} {}", "onnxruntime", mark(availability.runtime));
        println!("{:16} {}", "kokoro model", mark(availability.model));
        println!("{:16} {}", "kokoro voices", mark(availability.voices));
        println!("{:16} {}", "espeak-ng", mark(availability.phonemizer));
        println!();

        if !availability.model || !availability.voices {
            println!("The model is not installed. Run: python scripts/setup-voice.py");
            return Ok(());
        }

        // Listing voices only needs the voices file, so it works even when the
        // runtime is missing — which is a useful thing to be able to do.
        if let Ok(voices) = loom_core::voice::voices::Voices::load(&paths.voices) {
            println!("{} voices:", voices.len());
            for name in voices.names() {
                let lang = Kokoro::language_of(name)
                    .map(|l| l.espeak_name().to_string())
                    .unwrap_or_else(|| "—".to_string());
                println!("  {name:<18} {lang}");
            }
        }
        return Ok(());
    }

    println!("Loom home      : {}", home.display());
    println!("voice          : {voice}");
    println!("text           : {} characters", text.len());
    println!("speed          : {speed}");
    println!("{:16} {}", "onnxruntime", mark(availability.runtime));
    println!("{:16} {}", "kokoro model", mark(availability.model));
    println!("{:16} {}", "kokoro voices", mark(availability.voices));
    println!("{:16} {}", "espeak-ng", mark(availability.phonemizer));
    println!();

    if let Some(blocker) = availability.blocking() {
        eprintln!("cannot synthesize: {blocker}");
        eprintln!();
        eprintln!("Install everything with:");
        eprintln!("  python scripts/setup-voice.py");
        return Err(blocker.into());
    }

    println!("loading the model (325 MB, first run takes a moment)...");
    let load_start = std::time::Instant::now();
    let mut engine = Kokoro::load(&paths, &espeak_paths)?;
    println!(
        "loaded in {:.2}s — {} voices, token input {:?}",
        load_start.elapsed().as_secs_f32(),
        engine.voice_count(),
        engine.token_input()
    );
    println!();

    println!("speaking {} characters...", text.len());
    let start = std::time::Instant::now();
    let synthesis = engine.speak(&text, &voice, speed)?;
    let elapsed = start.elapsed().as_secs_f64();

    if synthesis.audio.is_empty() {
        eprintln!("produced no audio — every phoneme was dropped");
        eprintln!("this means the phonemizer and the model disagree about the vocabulary");
        return Err("no audio".into());
    }

    synthesis.audio.write_wav(&out)?;

    let audio = &synthesis.audio;
    println!();
    println!("wrote          : {}", out.display());
    println!("audio          : {:.2}s at {} Hz", audio.duration(), audio.sample_rate);
    println!("batches        : {}", synthesis.batches);
    println!("compute        : {:.3}s", synthesis.compute_seconds);
    println!("wall           : {:.3}s", elapsed);
    println!("real-time      : {:.2}x faster than playback", 1.0 / synthesis.real_time_factor().max(1e-6));
    println!("peak           : {:.3}", audio.peak());
    println!();

    // A one-line diagnosis, because the interesting failure is not "it broke"
    // but "it wrote a file that is silence or noise".
    let peak = audio.peak();
    if peak < 0.01 {
        println!("WARNING: the output is essentially silent.");
        println!("         The phonemizer probably produced phonemes the model");
        println!("         cannot speak, so every token was dropped.");
    } else if peak > 0.999 {
        println!("WARNING: the output is clipping. Reduce the speed or check the");
        println!("         style matrix — values should be small floats.");
    } else {
        println!("Play it and listen. Speech means the pipeline is correct end to end;");
        println!("noise means the phoneme-to-token mapping is wrong.");
    }

    Ok(())
}

/// Reads `--name value` from an argument list.
fn flag(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn mark(present: bool) -> &'static str {
    if present {
        "ok"
    } else {
        "MISSING"
    }
}
