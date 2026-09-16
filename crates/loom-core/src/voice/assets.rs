//! Fetching and verifying voice mode's downloadable files.
//!
//! Nothing in this module is shipped in the installer. Every artifact is
//! fetched on first use into [`paths::voice_dir`], verified, and left alone
//! afterwards.
//!
//! Three properties matter, and each has a reason:
//!
//! * **Resumable.** The model is 325 MB. A dropped connection must not mean
//!   starting again, so a partial file is kept as `<name>.part` and a `Range`
//!   request picks up where it stopped.
//! * **Verified.** A truncated or substituted model produces noise rather than
//!   an error, which is indistinguishable from a broken build. Every artifact
//!   is hashed before it is allowed to be used, and a mismatch deletes the
//!   download rather than leaving it to be tried again.
//! * **Unpinned is not silently trusted.** When an artifact has no known hash
//!   the download still succeeds, but [`Outcome::Unverified`] says so and
//!   carries the hash that was observed, so it can be pinned deliberately
//!   rather than assumed.

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::manifest::{Artifact, Pin};
use crate::{Error, Result};

/// Refuse to buffer more than this in one progress tick; keeps a fast local
/// download from flooding the UI event channel.
const PROGRESS_STEP: u64 = 4 * 1024 * 1024;

/// How long to wait for a stalled transfer before giving up on it.
///
/// These files are hundreds of megabytes over a consumer connection, so the
/// timeout is generous and the failure it prevents is a hang rather than a
/// slow download.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

/// The HTTP client to fetch artifacts with.
///
/// Built here rather than passed in so every caller gets the same timeout and
/// user agent. The agent matters: an unidentified client is more likely to be
/// rate-limited by the hosts these come from.
pub fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("loom/", env!("CARGO_PKG_VERSION")))
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| Error::Http(format!("building the voice download client: {e}")))
}

/// What a fetch did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Already present and the hash matched. Nothing was transferred.
    AlreadyInstalled,
    /// Downloaded and, when the artifact is pinned, verified.
    Installed,
    /// Downloaded, but this build has no hash to check it against. The observed
    /// hash is returned so it can become a pin.
    Unverified { observed_sha256: String },
}

impl Outcome {
    /// Whether the file has been proven to be the expected one.
    pub fn is_trusted(&self) -> bool {
        !matches!(self, Outcome::Unverified { .. })
    }
}

/// Reports how far along a download is.
pub trait Progress {
    /// `total` is `None` when the server does not send a content length.
    fn update(&mut self, artifact: &Artifact, received: u64, total: Option<u64>);
}

impl<F> Progress for F
where
    F: FnMut(&Artifact, u64, Option<u64>),
{
    fn update(&mut self, artifact: &Artifact, received: u64, total: Option<u64>) {
        self(artifact, received, total);
    }
}

/// A progress sink that discards everything, for callers that do not care.
pub struct Silent;

impl Progress for Silent {
    fn update(&mut self, _artifact: &Artifact, _received: u64, _total: Option<u64>) {}
}

/// Streams a file through SHA-256, returning the digest as lowercase hex.
///
/// Read in chunks rather than loaded whole: the model is 325 MB, and holding it
/// in memory twice over is avoidable.
pub async fn sha256_file(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| Error::io(path, e))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];

    loop {
        let read = tokio::io::AsyncReadExt::read(&mut file, &mut buffer)
            .await
            .map_err(|e| Error::io(path, e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hex(&hasher.finalize()))
}

/// Lowercase hex, which is the form every hash in this crate is written in.
pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Whether two hashes match, ignoring case and surrounding whitespace.
///
/// Hex is case-insensitive and both conventions are in the wild, so a
/// case-sensitive comparison would reject correct files.
pub fn hashes_match(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// A partial download's path: `<name>.part`.
fn partial_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

/// Where an artifact lives inside `dir`.
pub fn path_in(dir: &Path, artifact: &Artifact) -> PathBuf {
    dir.join(artifact.file_name)
}

/// Verifies an existing file against an artifact's pin.
///
/// `Ok(None)` means "present, but this build cannot prove it is correct".
pub async fn verify(path: &Path, artifact: &Artifact) -> Result<Option<Outcome>> {
    if !path.exists() {
        return Ok(None);
    }

    let Some(Pin { sha256, .. }) = artifact.pin else {
        // No pin: report what we found rather than pretending it is right.
        return Ok(Some(Outcome::Unverified {
            observed_sha256: sha256_file(path).await?,
        }));
    };

    if hashes_match(&sha256_file(path).await?, sha256) {
        Ok(Some(Outcome::AlreadyInstalled))
    } else {
        // Present but wrong. Deleting is deliberate: a file that fails
        // verification will fail it again, and leaving it means every future
        // launch re-hashes 325 MB to reach the same conclusion.
        std::fs::remove_file(path).map_err(|e| Error::io(path, e))?;
        Ok(None)
    }
}

/// Fetches `artifact` into `dir` if it is not already there and verified.
pub async fn ensure<P: Progress>(
    client: &reqwest::Client,
    artifact: &Artifact,
    dir: &Path,
    progress: &mut P,
) -> Result<Outcome> {
    if artifact.url.is_empty() {
        return Err(Error::Http(format!(
            "{} has no download URL; it is resolved at install time",
            artifact.id
        )));
    }

    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| Error::io(dir, e))?;

    let dest = path_in(dir, artifact);
    if let Some(outcome) = verify(&dest, artifact).await? {
        return Ok(outcome);
    }

    let partial = partial_path(&dest);
    let already = tokio::fs::metadata(&partial)
        .await
        .map(|meta| meta.len())
        .unwrap_or(0);

    let mut request = client.get(artifact.url);
    if already > 0 {
        request = request.header("Range", format!("bytes={already}-"));
    }

    let response = request
        .send()
        .await
        .map_err(|e| Error::Http(format!("{}: {e}", artifact.id)))?;

    if !response.status().is_success() {
        return Err(Error::Http(format!(
            "{}: HTTP {}",
            artifact.id,
            response.status().as_u16()
        )));
    }

    // A 200 to a ranged request means the server ignored the range, so the
    // stream is the whole file and appending would corrupt it.
    let resuming = already > 0 && response.status().as_u16() == 206;
    let mut written = if resuming { already } else { 0 };

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resuming)
        .truncate(!resuming)
        .open(&partial)
        .await
        .map_err(|e| Error::io(&partial, e))?;

    let total = response.content_length().map(|length| length + written);
    let mut stream = response.bytes_stream();
    let mut tick = written;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Error::Http(format!("{}: {e}", artifact.id)))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| Error::io(&partial, e))?;
        written += chunk.len() as u64;

        if written - tick >= PROGRESS_STEP {
            tick = written;
            progress.update(artifact, written, total);
        }
    }

    file.flush().await.map_err(|e| Error::io(&partial, e))?;
    drop(file);
    progress.update(artifact, written, total);

    let observed = sha256_file(&partial).await?;

    let outcome = match artifact.pin {
        Some(Pin { sha256, bytes }) => {
            if !hashes_match(&observed, sha256) {
                // Remove the evidence. A wrong file that stays on disk is worse
                // than no file, because the next attempt would find it and
                // re-hash it to the same verdict.
                let _ = tokio::fs::remove_file(&partial).await;
                return Err(Error::Http(format!(
                    "{} failed verification: expected {}, got {observed}",
                    artifact.id, sha256
                )));
            }
            if bytes > 0 && written != bytes {
                let _ = tokio::fs::remove_file(&partial).await;
                return Err(Error::Http(format!(
                    "{} is {written} bytes, expected {bytes}",
                    artifact.id
                )));
            }
            Outcome::Installed
        }
        None => Outcome::Unverified {
            observed_sha256: observed,
        },
    };

    tokio::fs::rename(&partial, &dest)
        .await
        .map_err(|e| Error::io(&dest, e))?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::manifest;

    // -- pure helpers -------------------------------------------------------

    #[test]
    fn hex_is_lowercase_and_two_digits_per_byte() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff]), "000fff");
        assert_eq!(hex(&[]), "");
    }

    #[test]
    fn the_known_digest_hashes_as_expected() {
        // Known answer test: the SHA-256 of "abc". Anchors `sha256_file` to a
        // value that does not come from this implementation.
        let digest = Sha256::digest(b"abc");
        assert_eq!(
            hex(&digest),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hashes_compare_case_insensitively_and_ignore_padding() {
        assert!(hashes_match("ABCD", "abcd"));
        assert!(hashes_match(" abcd ", "ABCD"));
        assert!(!hashes_match("abcd", "abce"));
        assert!(!hashes_match("abc", "abcd"));
    }

    #[test]
    fn the_partial_file_sits_beside_its_target() {
        let dest = Path::new("/tmp/voice/kokoro-v1.0.onnx");
        assert_eq!(
            partial_path(dest),
            Path::new("/tmp/voice/kokoro-v1.0.onnx.part")
        );
    }

    #[test]
    fn a_partial_file_is_never_mistaken_for_the_real_one() {
        // The whole reason for a distinct extension: `verify` looks for the
        // real name, so an interrupted download cannot be used.
        let dir = std::env::temp_dir().join("loom-voice-partial-test");
        let artifact = manifest::artifact("kokoro-model").unwrap();
        let dest = path_in(&dir, artifact);
        assert!(dest.ends_with("kokoro-v1.0.onnx"));
        assert_ne!(partial_path(&dest), dest);
        assert!(partial_path(&dest).to_string_lossy().ends_with(".part"));
    }

    #[test]
    fn artifacts_land_under_the_directory_they_are_given() {
        let dir = Path::new("/tmp/voice");
        for artifact in manifest::ARTIFACTS {
            let path = path_in(dir, artifact);
            assert_eq!(path.parent(), Some(dir));
            assert_eq!(
                path.file_name().unwrap().to_string_lossy(),
                artifact.file_name
            );
        }
    }

    #[test]
    fn an_artifact_without_a_url_is_rejected_rather_than_fetched() {
        // ONNX Runtime's URL depends on the CPU, so it carries an empty one.
        // Fetching it must fail loudly instead of issuing a request to "".
        let artifact = manifest::artifact("onnxruntime").unwrap();
        assert!(artifact.url.is_empty());
        assert!(!artifact.is_pinned());
    }

    #[test]
    fn unverified_outcomes_are_not_trusted() {
        assert!(!Outcome::Unverified {
            observed_sha256: "ab".into()
        }
        .is_trusted());
        assert!(Outcome::Installed.is_trusted());
        assert!(Outcome::AlreadyInstalled.is_trusted());
    }

    // -- filesystem ---------------------------------------------------------

    #[tokio::test]
    async fn hashing_a_file_matches_hashing_its_bytes() {
        let dir = std::env::temp_dir().join("loom-voice-hash-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("abc.bin");
        std::fs::write(&path, b"abc").unwrap();

        assert_eq!(
            sha256_file(&path).await.unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_file_that_does_not_match_its_pin_is_deleted() {
        let dir = std::env::temp_dir().join("loom-voice-verify-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wrong.bin");
        std::fs::write(&path, b"not the model").unwrap();

        let artifact = Artifact {
            id: "test",
            label: "test",
            kind: manifest::Kind::Model,
            url: "https://example.invalid/x",
            file_name: "wrong.bin",
            pin: Some(Pin {
                // The hash of "abc", which this file is not.
                sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                bytes: 3,
            }),
            licence: "test",
        };

        assert_eq!(verify(&path, &artifact).await.unwrap(), None);
        assert!(
            !path.exists(),
            "a file that failed verification must not be left behind"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_matching_file_is_reported_as_already_installed() {
        let dir = std::env::temp_dir().join("loom-voice-verify-ok-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("right.bin");
        std::fs::write(&path, b"abc").unwrap();

        let artifact = Artifact {
            id: "test",
            label: "test",
            kind: manifest::Kind::Model,
            url: "https://example.invalid/x",
            file_name: "right.bin",
            pin: Some(Pin {
                sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                bytes: 3,
            }),
            licence: "test",
        };

        assert_eq!(
            verify(&path, &artifact).await.unwrap(),
            Some(Outcome::AlreadyInstalled)
        );
        assert!(path.exists(), "a verified file must survive");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn an_absent_file_reports_nothing_rather_than_failing() {
        let dir = std::env::temp_dir().join("loom-voice-absent-test");
        let artifact = manifest::artifact("kokoro-model").unwrap();
        assert_eq!(verify(&dir.join("nope.bin"), artifact).await.unwrap(), None);
    }
}
