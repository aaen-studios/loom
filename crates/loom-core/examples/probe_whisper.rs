//! Prints the tensor signatures of the Whisper ONNX models.
//!
//! Written before `voice/whisper.rs` existed, because the input and output
//! names of an ONNX export are not something to remember: they differ between
//! exporters and between versions of the same exporter, and getting one wrong
//! is a runtime error at best. Everything downstream is written against what
//! this prints.
//!
//! ```text
//! cargo run -p loom-core --example probe_whisper
//! ```

use ort::session::Session;
use ort::value::ValueType;

use loom_core::voice::tts::Paths;

/// Renders a `ValueType` as a name, a dtype and a shape.
fn describe(role: &str, name: &str, dtype: &ValueType) {
    match dtype {
        ValueType::Tensor { ty, shape, .. } => {
            let dims: Vec<String> = shape
                .iter()
                .map(|dim| {
                    if *dim < 0 {
                        // A symbolic dimension: the export means "any", which
                        // for the decoder is the token sequence length.
                        format!("?({dim})")
                    } else {
                        dim.to_string()
                    }
                })
                .collect();
            println!("  {role:<6} {name:<26} {ty:?}  [{}]", dims.join(", "));
        }
        other => {
            // A sequence or map input has no shape; printing the type is all
            // that can be done, and it is enough to see it is not a tensor.
            println!("  {role:<6} {name:<26} {other:?}");
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let home = dirs::home_dir()
        .ok_or("no home directory")?
        .join(".loom");
    let paths = Paths::from_home(&home);

    // Loading ONNX Runtime by path, exactly as the engine does.
    ort::init_from(&paths.runtime).map_err(|e| {
        format!(
            "could not load ONNX Runtime from {}: {e}",
            paths.runtime.display()
        )
    })?
    .commit();
    println!("runtime      : {}", paths.runtime.display());

    let whisper = home.join("voice").join("whisper");

    for (label, file) in [
        ("encoder", whisper.join("encoder_model.onnx")),
        ("decoder", whisper.join("decoder_model.onnx")),
    ] {
        println!();
        println!("=== {label} ===");
        println!("file         : {}", file.display());

        if !file.exists() {
            println!("  MISSING — run: python scripts/fetch-whisper.py");
            continue;
        }

        let started = std::time::Instant::now();
        let session = Session::builder()?.commit_from_file(&file)?;
        println!(
            "loaded in    : {:.0} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );

        println!();
        for input in session.inputs() {
            describe("in", input.name(), input.dtype());
        }
        for output in session.outputs() {
            describe("out", output.name(), output.dtype());
        }
    }

    // The decode ids, from the configs the downloader fetched.
    println!();
    println!("=== decode ids ===");
    let generation =
        std::fs::read_to_string(whisper.join("generation_config.json")).unwrap_or_default();
    let config = std::fs::read_to_string(whisper.join("config.json")).unwrap_or_default();
    let ids = loom_core::voice::tokenizer::DecodeIds::from_configs(&generation, &config);
    println!("  start          : {}", ids.start);
    println!("  no_timestamps  : {}", ids.no_timestamps);
    println!("  eos            : {}", ids.eos);
    println!("  prompt         : {:?}", ids.prompt());

    Ok(())
}
