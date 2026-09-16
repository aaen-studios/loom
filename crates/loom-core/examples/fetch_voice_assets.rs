//! Fetches voice mode's downloadable assets and reports their hashes.
//!
//! This is the Phase 1 downloader driven from the command line, which makes it
//! usable as a pinning tool: for an artifact with no hash yet it prints the
//! observed digest, which is then pasted into `manifest.rs`.
//!
//! ```text
//! cargo run -p loom-core --example fetch_voice_assets
//! cargo run -p loom-core --example fetch_voice_assets -- --only kokoro-model
//! ```
//!
//! Deliberately not a `#[test]`: it touches the network, writes hundreds of
//! megabytes, and takes minutes. Nothing in the test suite should do that.

use std::path::PathBuf;

use loom_core::voice::assets::{self, Outcome};
use loom_core::voice::manifest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let only: Option<String> = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|pair| pair[0] == "--only")
        .map(|pair| pair[1].clone());

    let dir: PathBuf = match std::env::var_os("LOOM_HOME") {
        Some(home) => PathBuf::from(home).join("voice"),
        None => loom_core::paths::voice_dir()?,
    };

    println!("voice assets -> {}", dir.display());
    println!(
        "{} of {} artifacts pinned in this build\n",
        manifest::ARTIFACTS.iter().filter(|a| a.is_pinned()).count(),
        manifest::ARTIFACTS.len()
    );

    let client = assets::client()?;

    let mut unverified: Vec<(String, String)> = Vec::new();
    let mut failures: Vec<(String, String)> = Vec::new();

    for artifact in manifest::ARTIFACTS {
        if let Some(only) = &only {
            if artifact.id != only {
                continue;
            }
        }

        if artifact.url.is_empty() {
            println!("{:16} skipped — no URL (resolved at install time)", artifact.id);
            continue;
        }

        print!("{:16} ", artifact.id);
        let mut last_reported = 0u64;
        let mut progress = |_a: &manifest::Artifact, received: u64, total: Option<u64>| {
            // Redraw roughly every 32 MB so the line stays readable.
            if received / (32 * 1024 * 1024) != last_reported / (32 * 1024 * 1024) {
                last_reported = received;
                match total {
                    Some(total) => print!(
                        "\r{:16} {:>6.1} / {:.1} MB",
                        "",
                        received as f64 / 1e6,
                        total as f64 / 1e6
                    ),
                    None => print!("\r{:16} {:.1} MB", "", received as f64 / 1e6),
                }
            }
        };

        match assets::ensure(&client, artifact, &dir, &mut progress).await {
            Ok(Outcome::AlreadyInstalled) => {
                println!("\r{:16} already installed, hash matches", artifact.id);
            }
            Ok(Outcome::Installed) => {
                println!("\r{:16} installed and verified", artifact.id);
            }
            Ok(Outcome::Unverified { observed_sha256 }) => {
                println!("\r{:16} installed, NOT verified (no pin)", artifact.id);
                unverified.push((artifact.id.to_string(), observed_sha256));
            }
            Err(error) => {
                println!("\r{:16} failed: {error}", artifact.id);
                failures.push((artifact.id.to_string(), error.to_string()));
            }
        }
    }

    if !unverified.is_empty() {
        println!("\nObserved hashes — paste these into manifest.rs to pin them:");
        for (id, hash) in &unverified {
            let bytes = std::fs::metadata(dir.join(
                manifest::artifact(id).expect("known id").file_name,
            ))
            .map(|meta| meta.len())
            .unwrap_or(0);
            println!(
                "  {id}\n    sha256: \"{hash}\",\n    bytes: {bytes},"
            );
        }
    }

    if !failures.is_empty() {
        println!("\nFailures:");
        for (id, error) in &failures {
            println!("  {id}: {error}");
        }
        return Err(format!("{} artifact(s) failed", failures.len()).into());
    }

    Ok(())
}
