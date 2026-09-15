use std::path::Path;

fn main() {
    // `payload.zip` is embedded in the binary (see `src/lib.rs`), so the Setup
    // app is a single portable exe with no installer framework and no sidecar
    // files. `scripts/make-payload.mjs` produces the real one during a release;
    // for a plain build we drop in an empty (but valid) zip so the crate
    // compiles from a fresh clone.
    //
    // Build scripts run with the crate root as the working directory, and
    // `include_bytes!("../payload.zip")` resolves to the same place.
    let payload = Path::new("payload.zip");
    if !payload.exists() {
        // End-of-central-directory record for an empty archive.
        let empty_zip: [u8; 22] = [
            0x50, 0x4B, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        std::fs::write(payload, empty_zip).expect("failed to write placeholder payload.zip");
    }

    println!("cargo:rerun-if-changed=payload.zip");
    tauri_build::build()
}
