//! Writes Whisper's front-end artefacts to disk so they can be checked.
//!
//! The mel spectrogram is the part of speech-to-text most likely to be subtly
//! wrong, because being wrong does not error — it produces a *plausible*
//! transcript. Reading the code back is not enough to be sure, so this dumps
//! the three things that can be compared against an independent implementation:
//!
//! * `signal.f32` — the test waveform, raw little-endian `f32`
//! * `filters.f32` — the 80 × 201 mel filterbank
//! * `mel_log.f32` — the 80 × 3000 log-mel values, before the global steps
//! * `mel.f32` — the same after the clamp and the affine normalisation
//!
//! The log values are dumped separately because the clamp and the normalisation
//! are **global** — both depend on the peak across the whole spectrogram. That
//! makes them impossible to verify by recomputing part of one, and trivial to
//! verify in full from the arrays alone, which is what the checker does. Every
//! structural step is checked against `mel_log`, where no global state is
//! involved.
//!
//! `scripts/check-mel.py` reads the signal and recomputes the rest.
//!
//! ```text
//! cargo run -p loom-core --example dump_mel
//! python scripts/check-mel.py
//! ```

use std::io::Write;
use std::path::PathBuf;

use loom_core::voice::audio::TARGET_RATE;
use loom_core::voice::mel::{self, MelFilters, N_FREQ_BINS, N_MELS};

/// Where the artefacts go. Overridable with `--out`.
fn output_dir() -> PathBuf {
    let args: Vec<String> = std::env::args().collect();
    if let Some(index) = args.iter().position(|arg| arg == "--out") {
        if let Some(path) = args.get(index + 1) {
            return PathBuf::from(path);
        }
    }
    std::env::temp_dir().join("loom-mel")
}

/// A deterministic test signal, written so the Python side can reproduce it
/// exactly if it ever needs to.
///
/// Three tones rather than one: a single sine only exercises a couple of mel
/// bins, while a sum at different frequencies lights up the low, middle and
/// high parts of the filterbank at once. A short silence is included so the
/// `log10` floor and the dynamic-range clamp are both exercised.
fn test_signal() -> Vec<f32> {
    let seconds = 2usize;
    let mut samples = vec![0.0f32; TARGET_RATE as usize * seconds];

    for (index, sample) in samples.iter_mut().enumerate() {
        let t = index as f32 / TARGET_RATE as f32;
        let mut value = 0.0f32;

        // Low, middle and high, with amplitudes that differ so the mel rows
        // are not all the same.
        value += 0.30 * (2.0 * std::f32::consts::PI * 120.0 * t).sin();
        value += 0.20 * (2.0 * std::f32::consts::PI * 1_000.0 * t).sin();
        value += 0.10 * (2.0 * std::f32::consts::PI * 4_000.0 * t).sin();

        // Amplitude modulation, so successive frames differ and a bug that
        // ignores the hop length shows up as a mismatch.
        value *= 1.0 + 0.5 * (2.0 * std::f32::consts::PI * 3.0 * t).sin();

        // The first half-second is silence, which is what exercises the
        // flooring.
        *sample = if t < 0.5 { 0.0 } else { value };
    }

    samples
}

fn write_f32(path: &PathBuf, values: &[f32]) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    for value in values {
        file.write_all(&value.to_le_bytes())?;
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = output_dir();
    std::fs::create_dir_all(&dir)?;

    println!("output        : {}", dir.display());

    // 1. The signal.
    let signal = test_signal();
    let signal_path = dir.join("signal.f32");
    write_f32(&signal_path, &signal)?;
    println!("signal        : {} samples ({:.2} s)", signal.len(), signal.len() as f32 / TARGET_RATE as f32);

    // 2. The filterbank.
    let filters = MelFilters::new();
    let filters_path = dir.join("filters.f32");
    write_f32(&filters_path, filters.data())?;
    let weight_sum: f32 = filters.data().iter().sum();
    let peak = filters.data().iter().cloned().fold(f32::MIN, f32::max);
    println!(
        "filterbank    : {} x {} ({} values), sum {:.2}, peak {:.4}",
        N_MELS,
        N_FREQ_BINS,
        filters.data().len(),
        weight_sum,
        peak
    );

    // 3. The spectrogram, both ways. Timed, because 3000 frames of a
    //    400-point FFT plus an 80 x 201 matmul each is on the path that runs
    //    per dictation.
    let started = std::time::Instant::now();
    let raw = mel::log_mel_raw(&signal, &filters);
    let raw_elapsed = started.elapsed();

    let raw_path = dir.join("mel_log.f32");
    write_f32(&raw_path, &raw.data)?;

    // 4. The normalised spectrogram, which is what the encoder actually gets.
    let mut spectrogram = raw.clone();
    mel::normalize(&mut spectrogram);
    let mel_path = dir.join("mel.f32");
    write_f32(&mel_path, &spectrogram.data)?;

    let (low, high) = spectrogram.range();
    let (raw_low, raw_high) = raw.range();

    println!(
        "spectrogram   : {} x {} ({} values)",
        spectrogram.bins,
        spectrogram.frames,
        spectrogram.data.len()
    );
    println!("log range     : {raw_low:.4} .. {raw_high:.4}");
    println!("normalised    : {low:.4} .. {high:.4}");
    println!(
        "computed in   : {:.1} ms for {:.1} s of audio",
        raw_elapsed.as_secs_f64() * 1000.0,
        signal.len() as f32 / TARGET_RATE as f32
    );
    println!();
    println!("Now run: python scripts/check-mel.py");

    Ok(())
}
