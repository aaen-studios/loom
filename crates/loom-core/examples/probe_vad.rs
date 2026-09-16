//! Prints Silero VAD's behaviour on real speech: the per-window probability,
//! and what the listener makes of it.
//!
//! This drives the real [`Vad`] and the real [`Listener`] rather than
//! reimplementing the call. An earlier version assembled the ONNX inputs itself
//! and passed 512 samples — which is what the graph *accepts* and not what it
//! *wants*, so it reported 0% speech on audio that Whisper transcribes at 100%
//! word accuracy. A probe that duplicates the production path drifts from it;
//! this one cannot.
//!
//! ```text
//! cargo run -p loom-core --example probe_vad
//! ```

use loom_core::voice::audio;
use loom_core::voice::listen::{Listener, Listening};
use loom_core::voice::tts::Paths;
use loom_core::voice::vad::{Vad, CONTEXT, SPEECH_THRESHOLD, WINDOW};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let home = dirs::home_dir()
        .ok_or("no home directory")?
        .join(".loom");
    let paths = Paths::from_home(&home);

    let model = home.join("voice").join("silero_vad.onnx");
    if !model.exists() {
        eprintln!("missing: {}", model.display());
        eprintln!("Run: python scripts/fetch-whisper.py");
        return Err("no model".into());
    }

    ort::init_from(&paths.runtime).map_err(|e| {
        format!(
            "could not load ONNX Runtime from {}: {e}",
            paths.runtime.display()
        )
    })?
    .commit();

    println!("model : {}", model.display());
    println!("window: {WINDOW} samples, plus {CONTEXT} carried from the last one");
    println!();

    // A tone, to show the state moving rather than sitting at a fixed point.
    println!("=== a steady tone, six identical windows ===");
    let mut vad = Vad::load(&model)?;
    let mut window = vec![0.0f32; WINDOW];
    for (index, sample) in window.iter_mut().enumerate() {
        let t = index as f32 / audio::TARGET_RATE as f32;
        *sample = 0.1 * (2.0 * std::f32::consts::PI * 300.0 * t).sin();
    }
    let mut probabilities = Vec::new();
    for index in 0..6 {
        let probability = vad.probability(&window)?;
        probabilities.push(probability);
        println!("  window {index}: {probability:.6}");
    }
    let spread = probabilities.iter().cloned().fold(f32::MIN, f32::max)
        - probabilities.iter().cloned().fold(f32::MAX, f32::min);
    if spread < 1e-9 {
        println!("  IDENTICAL every time — the recurrent state is not being threaded.");
    } else {
        println!("  varying by {spread:.6}, so the state is carrying history.");
    }
    println!();

    // The real thing: eleven seconds of speech the other modules already agree
    // about.
    let wav = std::env::temp_dir().join("loom-kokoro.wav");
    let Ok(bytes) = std::fs::read(&wav) else {
        println!("no {}: run the hello_kokoro example first", wav.display());
        return Ok(());
    };
    let Some((samples, rate, channels)) = read_wav(&bytes) else {
        println!("could not read {}", wav.display());
        return Ok(());
    };

    let mono = audio::downmix(&samples, channels);
    let audio = audio::resample_to_16k(&mono, rate);

    println!("=== the Kokoro sample ===");
    println!(
        "{} samples at 16 kHz ({:.2} s), peak {:.3}",
        audio.len(),
        audio.len() as f32 / audio::TARGET_RATE as f32,
        audio.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
    );
    println!();

    // Every window's probability, so a short utterance can be explained rather
    // than guessed at.
    let mut vad = Vad::load(&model)?;
    let probabilities = vad.probabilities(&audio)?;

    let over = probabilities
        .iter()
        .filter(|p| **p >= SPEECH_THRESHOLD)
        .count();
    let peak = probabilities.iter().cloned().fold(0.0f32, f32::max);
    let trough = probabilities.iter().cloned().fold(1.0f32, f32::min);
    println!(
        "  {} windows; {over} at or above {SPEECH_THRESHOLD} ({:.0}%)",
        probabilities.len(),
        100.0 * over as f32 / probabilities.len().max(1) as f32
    );
    println!("  peak {peak:.4}, trough {trough:.4}");
    println!();

    // The longest run of each, which is what the hysteresis cares about: four
    // consecutive quiet windows close an utterance, and a single quiet window
    // does not.
    println!("  runs:");
    let mut run_start = 0usize;
    for index in 1..=probabilities.len() {
        let ended = index == probabilities.len()
            || (probabilities[index] >= SPEECH_THRESHOLD)
                != (probabilities[run_start] >= SPEECH_THRESHOLD);
        if ended {
            let length = index - run_start;
            let kind = if probabilities[run_start] >= SPEECH_THRESHOLD {
                "speech"
            } else {
                "      "
            };
            // Only the long ones matter, and only the quiet ones can close.
            if length >= 3 || kind == "speech" {
                println!(
                    "    {kind} {length:>4} windows ({:.2} s)  from {:.2} s  p={:.3}",
                    length as f32 * WINDOW as f32 / audio::TARGET_RATE as f32,
                    run_start as f32 * WINDOW as f32 / audio::TARGET_RATE as f32,
                    probabilities[run_start]
                );
            }
            run_start = index;
        }
    }
    println!();

    // And what the listener does with it, in 100 ms blocks as a microphone
    // would deliver it.
    println!("=== what the listener makes of it ===");
    let vad = Vad::load(&model)?;
    let mut listener = Listener::new(vad);

    let mut reports = 0usize;
    let block = audio::TARGET_RATE as usize / 10;
    for (index, chunk) in audio.chunks(block).enumerate() {
        match listener.push(chunk)? {
            Listening::Nothing => {}
            Listening::Started { samples } => {
                reports += 1;
                println!(
                    "  started  at {:.2} s with {} samples",
                    index as f32 * block as f32 / audio::TARGET_RATE as f32,
                    samples.len()
                );
            }
            Listening::Finished { samples } => {
                reports += 1;
                println!(
                    "  finished at {:.2} s, {:.2} s of audio",
                    index as f32 * block as f32 / audio::TARGET_RATE as f32,
                    samples.len() as f32 / audio::TARGET_RATE as f32
                );
            }
            Listening::Truncated { samples } => {
                reports += 1;
                println!(
                    "  truncated at {:.2} s, {:.2} s of audio",
                    index as f32 * block as f32 / audio::TARGET_RATE as f32,
                    samples.len() as f32 / audio::TARGET_RATE as f32
                );
            }
        }
    }

    if let Some(report) = listener.finish()? {
        reports += 1;
        println!(
            "  on finish, {:.2} s of audio at the end",
            report.samples().map(<[f32]>::len).unwrap_or(0) as f32
                / audio::TARGET_RATE as f32
        );
    }

    println!();
    if reports == 0 {
        println!("  no utterances. Either the audio is not speech, or the detector");
        println!("  is not being called with the context it needs.");
    } else {
        println!("  {reports} reports over an 11 s recording of two sentences.");
        println!("  More than one utterance is expected and correct: there is a");
        println!("  sentence boundary in the middle, and 100 ms of quiet closes one.");
    }

    Ok(())
}

/// Minimal PCM WAV reader.
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
