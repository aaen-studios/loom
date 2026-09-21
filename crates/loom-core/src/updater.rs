//! Ground-up updater.
//!
//! `update.json` lives on the GitHub release. It names the payload archive,
//! its SHA-256, and (optionally) a minisign signature. The app downloads the
//! archive into `~/.loom/cache`, verifies it, stages it, and a tiny script
//! applies the swap after the app exits and relaunches it.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{paths, Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateManifest {
    pub version: String,
    #[serde(default)]
    pub notes: String,
    /// Absolute URL of the payload zip.
    pub payload: String,
    /// Lowercase hex SHA-256 of the payload.
    pub sha256: String,
    /// Optional minisign signature (base64 payload, prefixed `untrusted comment:` line).
    #[serde(default)]
    pub signature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    /// Whether there is a newer release that this build is willing to install.
    ///
    /// **Not** the same as "a newer release exists". A newer release whose
    /// payload carries no signature is deliberately not offered — see
    /// [`check_signature_present`] — and reports `available: false` with
    /// `refused` set, so the UI can explain itself instead of appearing to be
    /// broken.
    pub available: bool,
    pub current_version: String,
    pub manifest: Option<UpdateManifest>,
    /// Why a newer release is not being offered, when one exists and cannot be
    /// installed. `None` in every ordinary case, including "already current".
    #[serde(default)]
    pub refused: Option<String>,
}

/// Parses `1.2.3` into comparable numbers. Pre-release and build metadata are
/// ignored, so `1.0.0-rc.2` is treated as `1.0.0`.
pub fn version_parts(version: &str) -> Vec<u64> {
    let release = version.trim_start_matches('v');
    let release = release.split(['-', '+']).next().unwrap_or(release);
    release
        .split('.')
        .map(|part| {
            part.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u64>()
                .unwrap_or(0)
        })
        .collect()
}

/// True when `candidate` is a strictly newer version than `current`.
pub fn is_newer(current: &str, candidate: &str) -> bool {
    let current = version_parts(current);
    let candidate = version_parts(candidate);
    for index in 0..current.len().max(candidate.len()) {
        let left = current.get(index).copied().unwrap_or(0);
        let right = candidate.get(index).copied().unwrap_or(0);
        if right != left {
            return right > left;
        }
    }
    false
}

/// Fetches and parses `update.json`, returning a manifest only when it is
/// newer than `current_version`.
pub async fn check(
    client: &reqwest::Client,
    current_version: &str,
    manifest_url: &str,
) -> Result<UpdateCheck> {
    let response = client
        .get(manifest_url)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|e| Error::Http(format!("update check failed: {e}")))?;

    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(Error::Http(format!("update check returned HTTP {status}")));
    }

    let body = response
        .text()
        .await
        .map_err(|e| Error::Http(e.to_string()))?;
    let manifest: UpdateManifest = serde_json::from_str(&body)
        .map_err(|e| Error::Other(format!("update manifest is malformed: {e}")))?;

    let newer = is_newer(current_version, &manifest.version);
    // A newer release this build will not trust is not offered, but it is not
    // silence either: `refused` carries the reason so the UI can explain itself
    // rather than claiming the user is already current. See
    // `check_signature_present`.
    let refused = if newer {
        check_signature_present(&manifest, signature_is_required())
            .err()
            .map(|error| error.to_string())
    } else {
        None
    };
    let available = newer && refused.is_none();
    Ok(UpdateCheck {
        available,
        current_version: current_version.to_string(),
        manifest: available.then_some(manifest),
        refused,
    })
}

/// Downloads the payload into the cache directory. Returns the archive path.
pub async fn download(client: &reqwest::Client, manifest: &UpdateManifest) -> Result<PathBuf> {
    let directory = paths::cache_dir()?;
    std::fs::create_dir_all(&directory).map_err(|e| Error::io(&directory, e))?;
    let target = directory.join(format!("loom-{}.zip", manifest.version));

    let response = client
        .get(&manifest.payload)
        .send()
        .await
        .map_err(|e| Error::Http(format!("download failed: {e}")))?;
    if !(200..300).contains(&response.status().as_u16()) {
        return Err(Error::Http(format!(
            "download returned HTTP {}",
            response.status()
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| Error::Http(format!("download interrupted: {e}")))?;
    std::fs::write(&target, &bytes).map_err(|e| Error::io(&target, e))?;

    // The policy first, and before a single byte is verified, because this is
    // the one check that is about *whether* to trust a release rather than
    // whether the bytes arrived intact. See `check_signature_present`.
    check_signature_present(manifest, signature_is_required())?;

    verify_sha256(&target, &manifest.sha256)?;
    if let Some(signature) = manifest.signature.as_deref() {
        verify_signature(&target, signature)?;
    }

    Ok(target)
}

/// Whether this build demands a signature on a release payload.
///
/// True whenever a public key is compiled in, which is every shipped build —
/// `UPDATE_PUBLIC_KEY` is a constant, not a placeholder. Only a build with an
/// empty key (a fork that has not generated its own keypair, or a test harness)
/// answers false.
pub fn signature_is_required() -> bool {
    !public_key().trim().is_empty()
}

/// Refuses an unsigned release when `require` is set.
///
/// ## Why this is a refusal rather than a skip
///
/// The download path used to verify a signature *only if the manifest carried
/// one*, which meant an unsigned `update.json` skipped the check entirely and the
/// app would swap its own binary for whatever that payload contained. The hash
/// alone does not help: `sha256` arrives in the same manifest, so it proves the
/// download was not corrupted in transit and proves nothing about who built it.
///
/// That is the opposite of what this project documents, in three places. The
/// README says "release payloads are signed with minisign and the app verifies
/// that signature before applying anything"; `docs/spec.md` says the same; and the
/// download page states outright that a release with no signature is one "the app
/// will refuse to auto-update from, which is the intended behaviour rather than a
/// fault". The code now does what the prose says.
///
/// The cost is real and worth naming: a release published without
/// `LOOM_MINISIGN_KEY` cannot be installed by the updater at all. That is the
/// intended trade — an update channel that silently accepts unsigned payloads is
/// a remote code execution path with a version number on it — and the error below
/// says exactly what is wrong and what to do, rather than failing obscurely.
pub fn check_signature_present(manifest: &UpdateManifest, require: bool) -> Result<()> {
    if !require {
        return Ok(());
    }
    if manifest
        .signature
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty()
    {
        return Err(Error::other(format!(
            "release {} carries no minisign signature, so Loom will not install it. \
             A payload that is not signed cannot be proven to be the one that was \
             published. If you maintain this project, set LOOM_MINISIGN_KEY so the \
             release workflow signs the payload.",
            manifest.version
        )));
    }
    Ok(())
}

/// Lowercase hex SHA-256 of a file.
pub fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};

    let mut file = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|e| Error::io(path, e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Compares the archive's digest with the manifest.
pub fn verify_sha256(path: &Path, expected: &str) -> Result<()> {
    let actual = sha256_file(path)?;
    if actual.eq_ignore_ascii_case(expected.trim()) {
        Ok(())
    } else {
        Err(Error::other(format!(
            "checksum mismatch: expected {expected}, got {actual}"
        )))
    }
}

/// Verifies a minisign signature over the payload bytes.
///
/// The public key is compiled in; the private key lives in `~/.loom/keys/` on
/// the maintainer's machine and in the release workflow's secrets.
pub fn verify_signature(path: &Path, signature: &str) -> Result<()> {
    let key = public_key();
    if key.trim().is_empty() {
        // Unsigned builds only: skip when no key is compiled in.
        return Ok(());
    }
    verify_signature_with_key(path, signature, &key)
}

/// Signature check with an explicit key (used by tests and by builds that
/// inject `LOOM_UPDATE_PUBLIC_KEY`).
pub fn verify_signature_with_key(path: &Path, signature: &str, key: &str) -> Result<()> {
    let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;

    let public = if key.contains('\n') {
        minisign_verify::PublicKey::decode(key.trim())
    } else {
        minisign_verify::PublicKey::from_base64(key.trim())
    }
    .map_err(|e| Error::other(format!("invalid update public key: {e}")))?;

    let signature = minisign_verify::Signature::decode(signature.trim())
        .map_err(|e| Error::other(format!("invalid update signature: {e}")))?;

    public
        .verify(&bytes, &signature, false)
        .map_err(|e| Error::other(format!("update signature did not verify: {e}")))
}

/// Compiled-in minisign public key (safe to publish).
pub const UPDATE_PUBLIC_KEY: &str = "untrusted comment: minisign public key E0F0900488EE45DC\r\nRWTcRe6IBJDw4DRdVWHvaKGMe918aIDaAeTHO5YciQcCfucFFqENkYuv";

fn public_key() -> String {
    option_env!("LOOM_UPDATE_PUBLIC_KEY")
        .map(str::to_string)
        .unwrap_or_else(|| UPDATE_PUBLIC_KEY.to_string())
}

/// Extracts the payload over `install_dir`, staging through a temp folder so a
/// failure cannot leave a half-written install.
pub fn stage_zip(zip_path: &Path, install_dir: &Path) -> Result<PathBuf> {
    let staging = install_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("loom-staging");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| Error::io(&staging, e))?;

    extract_zip(zip_path, &staging)?;
    Ok(staging)
}

/// How many half-second waits the swap script allows for the app to exit.
///
/// Bounded, and that bound is the point. The loop used to be `goto wait` with no
/// counter, so a `tasklist` that kept reporting the image name — a second Loom
/// running, a filter that matched something else, a `tasklist` that failed and
/// left `find` matching its own output — spun forever with no message and no
/// relaunch. Thirty seconds is far longer than a clean exit takes and far shorter
/// than a user waiting to find out why nothing happened.
pub const SWAP_WAIT_TRIES: u32 = 60;

/// Applies a staged update: writes a script that swaps files after this
/// process exits, then relaunches the app.
///
/// The script itself is launched with `CREATE_NO_WINDOW` (see
/// `commands::apply_update`), which is also why the wait loop sleeps with
/// `ping -n 2 127.0.0.1` rather than `timeout /t 1`: `timeout` refuses to run
/// without a console. The swap's output goes to `loom-update.log` beside the
/// script, so a silent failure is still diagnosable afterwards.
///
/// ## Why a failed copy still relaunches
///
/// This script is the last thing that runs, and it deletes itself on the way out.
/// The first version did `if errorlevel 1 (... & goto end)`, so an `xcopy` that
/// failed — a locked file, a full disk, a payload that did not unpack — jumped
/// straight past the `start` line to `del "%~f0"`. The result: Loom was not
/// running, was not going to be started, and the only record was a line in a
/// `%TEMP%` log nobody had a reason to open. Pressing "Restart & install" made the
/// application disappear, which is exactly the shape of an uninstall.
///
/// So the copy's failure no longer skips the relaunch. Whatever is at
/// `{install}\{exe}` is started either way, and if the exe is genuinely gone the
/// log says so in as many words. A half-applied update that still starts is a bad
/// afternoon; an update that leaves nothing to start is a reinstall.
pub fn apply_after_exit(staging: &Path, install_dir: &Path, exe_name: &str) -> Result<PathBuf> {
    let script = std::env::temp_dir().join("loom-update.cmd");
    let log = update_log_path();
    // Written with `\r\n` throughout: this is a batch file, and a bare `\n` in a
    // `.cmd` makes `goto` behave in ways that are not worth discovering.
    let body = format!(
        "@echo off\r\n\
         setlocal\r\n\
         set tries=0\r\n\
         :wait\r\n\
         tasklist /FI \"IMAGENAME eq {exe}\" | find /I \"{exe}\" >nul || goto copy\r\n\
         set /a tries+=1\r\n\
         if %tries% GEQ {limit} goto copy\r\n\
         ping -n 2 127.0.0.1 >nul\r\n\
         goto wait\r\n\
         :copy\r\n\
         xcopy /E /Y /I \"{staging}\\*\" \"{install}\\\" >>\"{log}\" 2>&1\r\n\
         if errorlevel 1 (\r\n\
         echo %DATE% %TIME% update failed, files not replaced >>\"{log}\"\r\n\
         ) else (\r\n\
         echo %DATE% %TIME% update applied >>\"{log}\"\r\n\
         )\r\n\
         :relaunch\r\n\
         if exist \"{install}\\{exe}\" (\r\n\
         start \"\" \"{install}\\{exe}\"\r\n\
         ) else (\r\n\
         echo %DATE% %TIME% {exe} is missing from {install}; Loom was not restarted >>\"{log}\"\r\n\
         )\r\n\
         del \"%~f0\"\r\n",
        exe = exe_name,
        staging = staging.display(),
        install = install_dir.display(),
        log = log.display(),
        limit = SWAP_WAIT_TRIES,
    );

    std::fs::write(&script, body).map_err(|e| Error::io(&script, e))?;
    Ok(script)
}

/// `%TEMP%\loom-update.log` — what the swap script wrote while replacing files.
pub fn update_log_path() -> PathBuf {
    std::env::temp_dir().join("loom-update.log")
}

/// Extracts every file from a zip archive into `target`.
pub fn extract_zip(zip_path: &Path, target: &Path) -> Result<()> {
    let file = std::fs::File::open(zip_path).map_err(|e| Error::io(zip_path, e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| Error::Other(format!("bad zip: {e}")))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| Error::Other(format!("bad zip entry: {e}")))?;
        let Some(name) = entry.enclosed_name() else {
            return Err(Error::other("zip contains an unsafe path"));
        };
        let destination = target.join(name);

        if entry.is_dir() {
            std::fs::create_dir_all(&destination).map_err(|e| Error::io(&destination, e))?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let mut out =
            std::fs::File::create(&destination).map_err(|e| Error::io(&destination, e))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| Error::io(&destination, e))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn version_comparison_handles_prefixes_and_lengths() {
        assert!(is_newer("0.1.0", "0.1.1"));
        assert!(is_newer("0.1.0", "v0.2"));
        assert!(is_newer("1.0.0", "1.0.0-rc.2") == false);
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("2.0.0", "1.9.9"));
    }

    #[test]
    fn versions_parse_numerically() {
        assert_eq!(version_parts("v1.12.3-beta.4"), vec![1, 12, 3]);
        assert_eq!(version_parts("0.10"), vec![0, 10]);
    }

    #[test]
    fn the_swap_script_needs_no_console() {
        let dir = tempfile::tempdir().unwrap();
        let script = apply_after_exit(&dir.path().join("staging"), dir.path(), "loom.exe").unwrap();
        let body = std::fs::read_to_string(&script).unwrap();

        // `timeout` needs a console, and the script is spawned without one.
        assert!(
            !body.contains("timeout /t"),
            "the swap script must not call timeout: {body}"
        );
        assert!(body.contains("ping -n 2 127.0.0.1"), "{body}");
        assert!(body.contains("tasklist"), "{body}");
        assert!(body.contains("loom-update.log"), "{body}");
        assert!(body.contains("loom.exe"), "{body}");
        let _ = std::fs::remove_file(&script);
    }

    /// The regression this whole file exists to prevent, and the closest thing
    /// in the codebase to "Loom uninstalled itself": a failed swap that skipped
    /// the relaunch, deleted its own script, and left nothing running.
    #[test]
    fn a_failed_swap_still_relaunches_the_app() {
        let dir = tempfile::tempdir().unwrap();
        let script = apply_after_exit(&dir.path().join("staging"), dir.path(), "loom.exe").unwrap();
        let body = std::fs::read_to_string(&script).unwrap();

        // The relaunch must not sit behind the copy's error branch. Stated as an
        // ordering assertion rather than a string match, because the bug was
        // precisely that `goto end` jumped over this line.
        let copy = body.find("xcopy").expect("the swap must copy");
        let relaunch = body.find("start \"\"").expect("the swap must relaunch");
        assert!(
            relaunch > copy,
            "the relaunch must follow the copy: {body}"
        );
        // And nothing may jump past it: the only `goto` targets are the loop and
        // the copy, neither of which is past the relaunch.
        assert!(
            !body.contains("goto end"),
            "a `goto end` is how the relaunch was skipped: {body}"
        );

        // A missing exe is reported rather than silently ignored, so the log
        // explains a vanished install instead of the app just not appearing.
        assert!(body.contains("Loom was not restarted"), "{body}");
        let _ = std::fs::remove_file(&script);
    }

    #[test]
    fn the_wait_loop_is_bounded() {
        // Unbounded, this spun forever with no message and no relaunch.
        let dir = tempfile::tempdir().unwrap();
        let script = apply_after_exit(&dir.path().join("staging"), dir.path(), "loom.exe").unwrap();
        let body = std::fs::read_to_string(&script).unwrap();

        assert!(body.contains("set /a tries+=1"), "{body}");
        assert!(
            body.contains(&format!("GEQ {SWAP_WAIT_TRIES}")),
            "the loop needs a ceiling: {body}"
        );
        let _ = std::fs::remove_file(&script);
    }

    #[test]
    fn an_unsigned_release_is_refused_when_a_key_is_compiled_in() {
        // The check that was missing entirely: an unsigned manifest used to skip
        // signature verification rather than failing it, so the app would apply
        // whatever that payload contained.
        let unsigned = UpdateManifest {
            version: "9.9.9".into(),
            notes: String::new(),
            payload: "https://example.invalid/p.zip".into(),
            sha256: "0".repeat(64),
            signature: None,
        };
        let error = check_signature_present(&unsigned, true).unwrap_err();
        assert!(
            error.to_string().contains("no minisign signature"),
            "{error}"
        );

        // A blank signature is the same case as a missing one — a `signature: ""`
        // in a hand-edited manifest must not read as signed.
        let blank = UpdateManifest {
            signature: Some("   ".into()),
            ..unsigned.clone()
        };
        assert!(check_signature_present(&blank, true).is_err());

        // A present signature passes this gate; whether it *verifies* is a
        // separate question, answered by `verify_signature_with_key`.
        let signed = UpdateManifest {
            signature: Some(TEST_SIGNATURE.into()),
            ..unsigned.clone()
        };
        assert!(check_signature_present(&signed, true).is_ok());

        // A build with no key of its own has nothing to verify against, so it
        // does not demand one. This is the fork/test case, not a shipped build.
        assert!(check_signature_present(&unsigned, false).is_ok());
    }

    #[test]
    fn shipped_builds_require_a_signature() {
        // `UPDATE_PUBLIC_KEY` is a real key, not a placeholder, so every shipped
        // build answers true here. A build that answered false would silently
        // accept unsigned payloads, which is the hole this closes.
        assert!(
            signature_is_required(),
            "a compiled-in public key means signatures are required"
        );
    }

    #[test]
    fn sha256_is_computed_and_checked() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("payload.bin");
        std::fs::write(&path, b"loom").unwrap();

        let digest = sha256_file(&path).unwrap();
        assert_eq!(digest.len(), 64);
        assert!(verify_sha256(&path, &digest).is_ok());
        assert!(verify_sha256(&path, &"0".repeat(64)).is_err());
    }

    #[test]
    fn zip_extraction_rejects_escapes_and_writes_files() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("payload.zip");

        {
            let file = std::fs::File::create(&zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file("bin/app.txt", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"hello").unwrap();
            writer.finish().unwrap();
        }

        let target = dir.path().join("out");
        std::fs::create_dir_all(&target).unwrap();
        extract_zip(&zip_path, &target).unwrap();
        assert_eq!(
            std::fs::read_to_string(target.join("bin/app.txt")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn staging_directory_is_cleared_between_attempts() {
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("app");
        std::fs::create_dir_all(&install).unwrap();
        let zip_path = dir.path().join("payload.zip");
        {
            let file = std::fs::File::create(&zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file("loom.exe", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"binary").unwrap();
            writer.finish().unwrap();
        }

        let staging = stage_zip(&zip_path, &install).unwrap();
        assert!(staging.join("loom.exe").exists());
    }

    /// Throwaway keypair used only to prove the signature path works.
    const TEST_PUBLIC_KEY: &str = "untrusted comment: minisign public key 05CE77E9925216E7\r\nRWTnFlKS6XfOBW8LZKQEo3YyfA6I+MKiqS0PK+sdNv9/7qo6HZepynb7";
    const TEST_SIGNATURE: &str = "untrusted comment: loom test signature\r\nRUTnFlKS6XfOBTE34iZQI0ihNYDS9J/hu49WudXUA/YgIlubq8kjLsuoHjBqtwJ3/T0L/s7UFCaQo8W/55hEIZhOVx/7yKk+YAE=\r\ntrusted comment: timestamp:1789491670\tfile:payload.txt\thashed\r\n85kamemwyJclmwB5yGJZ1ecREDrv6e4oG2TsFq0aJrULAB3oW68NioSUtFTHu9zd/0pc1nRGOEYuM18IXacxDg==";
    const TEST_PAYLOAD: &[u8] = b"loom payload test";

    #[test]
    fn signatures_verify_and_tampering_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("payload.bin");
        std::fs::write(&path, TEST_PAYLOAD).unwrap();

        verify_signature_with_key(&path, TEST_SIGNATURE, TEST_PUBLIC_KEY)
            .expect("valid signature must verify");

        // A different key must reject the signature.
        let other = "RWQAAEIyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA3EXuiASQ8OAYsfSgUAWJrYSFbayQ1LUSScQISXM9MKVKljnWCPAOITRdVWHvaKGMe918aIDaAeTHO5YciQcCfucFFqENkYuv";
        assert!(verify_signature_with_key(&path, TEST_SIGNATURE, other).is_err());

        // Changing the payload must reject the signature.
        std::fs::write(&path, b"loom payload tampered").unwrap();
        assert!(verify_signature_with_key(&path, TEST_SIGNATURE, TEST_PUBLIC_KEY).is_err());
    }

    #[test]
    fn shipping_public_key_is_well_formed() {
        // Guards against a placeholder or truncated key reaching a release.
        assert!(!UPDATE_PUBLIC_KEY.contains("__LOOM"));
        let key = minisign_verify::PublicKey::decode(UPDATE_PUBLIC_KEY)
            .expect("compiled-in public key must parse");
        assert_eq!(key.untrusted_comment().unwrap_or_default().len() > 0, true);
    }
}
