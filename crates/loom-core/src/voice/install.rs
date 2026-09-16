//! Installing voice mode's components from inside the app.
//!
//! `scripts/setup-voice.py` does the same job on a development machine. This
//! exists so the app can do it too, without requiring Python — the whole point
//! of shipping nothing in the installer is that the app fetches it later.
//!
//! Four components, in dependency order:
//!
//! | | source | why not bundled |
//! |---|---|---|
//! | ONNX Runtime | a GitHub release archive | its correct build depends on the host CPU |
//! | espeak-ng | a PyPI wheel | it is GPL, and the user may already have one |
//! | Kokoro model | the `kokoro-onnx` release | 325 MB, and nothing else needs it |
//! | Kokoro voices | the same release | 28 MB |
//!
//! The two archives are extracted with `zip`, which was already a dependency
//! for reading the voices file.
//!
//! # Why espeak-ng comes from a wheel
//!
//! The PyPI `espeakng-loader` wheel contains a prebuilt `espeak-ng.dll` and the
//! 400-file data directory it needs. Building espeak-ng instead would need
//! MSVC Build Tools, which is not something an installer can reasonably require
//! — see `voice/espeak.rs`.

use std::io::Read;
use std::path::{Path, PathBuf};

use super::manifest;
use crate::{Error, Result};

/// How far along one component is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    NotInstalled,
    Downloading { received: u64, total: Option<u64> },
    Extracting,
    Done,
    Failed { reason: String },
}

impl Step {
    /// Whether the component is usable.
    pub fn is_done(&self) -> bool {
        matches!(self, Step::Done)
    }

    /// A short label, for a log line or a status pill.
    pub fn label(&self) -> String {
        match self {
            Step::NotInstalled => "not installed".to_string(),
            Step::Downloading { received, total } => match total {
                Some(total) if *total > 0 => {
                    format!("{}% of {:.1} MB", received * 100 / total, *total as f64 / 1e6)
                }
                _ => format!("{:.1} MB", *received as f64 / 1e6),
            },
            Step::Extracting => "extracting".to_string(),
            Step::Done => "installed".to_string(),
            Step::Failed { reason } => format!("failed: {reason}"),
        }
    }
}

/// The components, in the order they must be installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    /// ONNX Runtime, loaded by `ort` at run time.
    OnnxRuntime,
    /// espeak-ng, reached by hand-written FFI.
    Espeak,
    /// The Kokoro graph.
    Model,
    /// The voice style matrix.
    Voices,
}

impl Component {
    /// Every component, in install order.
    pub const ALL: [Component; 4] = [
        Component::OnnxRuntime,
        Component::Espeak,
        Component::Model,
        Component::Voices,
    ];

    /// The id used in progress events and in the UI.
    pub fn id(self) -> &'static str {
        match self {
            Component::OnnxRuntime => "onnxruntime",
            Component::Espeak => "espeak",
            Component::Model => "kokoro-model",
            Component::Voices => "kokoro-voices",
        }
    }

    /// What the user sees.
    pub fn label(self) -> &'static str {
        match self {
            Component::OnnxRuntime => "ONNX Runtime",
            Component::Espeak => "espeak-ng",
            Component::Model => "Kokoro voice model",
            Component::Voices => "Kokoro voices",
        }
    }

    /// The licence the user is agreeing to by installing it.
    ///
    /// Shown before the download, not after: espeak-ng is GPL, and that is a
    /// fact about what is being put on the machine.
    pub fn licence(self) -> &'static str {
        match self {
            Component::OnnxRuntime => "MIT",
            Component::Espeak => "GPL-3.0",
            Component::Model => "Apache 2.0",
            Component::Voices => "Apache 2.0",
        }
    }

    /// Roughly how large the download is, in bytes, for a progress estimate.
    pub fn expected_bytes(self) -> u64 {
        match self {
            Component::OnnxRuntime => 72_000_000,
            Component::Espeak => 9_500_000,
            Component::Model => 325_505_369,
            Component::Voices => 28_214_398,
        }
    }

    /// Whether this needs a network fetch at all.
    pub fn is_download(self) -> bool {
        true
    }
}

/// Where each component lives, and whether it is present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub component: Component,
    pub step: Step,
}

/// Receives progress as the install runs.
pub trait Progress {
    fn update(&mut self, component: Component, step: Step);
}

impl<F> Progress for F
where
    F: FnMut(Component, Step),
{
    fn update(&mut self, component: Component, step: Step) {
        self(component, step)
    }
}

/// Discards progress, for callers that only want the outcome.
pub struct Silent;

impl Progress for Silent {
    fn update(&mut self, _component: Component, _step: Step) {}
}

/// The runtime library's file name for this platform.
fn runtime_library() -> &'static str {
    if cfg!(windows) {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    }
}

/// The espeak-ng library's file name for this platform.
fn espeak_library() -> &'static str {
    if cfg!(windows) {
        "espeak-ng.dll"
    } else if cfg!(target_os = "macos") {
        "libespeak-ng.dylib"
    } else {
        "libespeak-ng.so"
    }
}

/// Which component, if any, already needs no work.
pub fn status(home: &Path) -> Vec<Status> {
    let paths = super::tts::Paths::from_home(home);
    let espeak_paths = super::espeak::Paths::from_home(home);
    let data = espeak_paths.data_root.join("espeak-ng-data").is_dir();

    Component::ALL
        .into_iter()
        .map(|component| {
            let step = match component {
                Component::OnnxRuntime if paths.runtime.exists() => Step::Done,
                Component::Espeak if espeak_paths.library.exists() && data => Step::Done,
                Component::Model if paths.model.exists() => Step::Done,
                Component::Voices if paths.voices.exists() => Step::Done,
                _ => Step::NotInstalled,
            };
            Status { component, step }
        })
        .collect()
}

/// Whether everything is present.
pub fn is_complete(home: &Path) -> bool {
    status(home).iter().all(|entry| entry.step.is_done())
}

/// Installs every component that is missing.
///
/// Idempotent: anything already present is reported `Done` and skipped, so a
/// partial install resumes rather than starting over. A failure does not abort
/// the rest — each component is independent, and a user who is missing only
/// espeak-ng should still get the model.
pub async fn install_all<P: Progress>(
    home: &Path,
    client: &reqwest::Client,
    progress: &mut P,
) -> Vec<(Component, Result<()>)> {
    let mut results = Vec::new();

    for component in Component::ALL {
        if status(home)
            .into_iter()
            .any(|entry| entry.component == component && entry.step.is_done())
        {
            progress.update(component, Step::Done);
            results.push((component, Ok(())));
            continue;
        }

        let outcome = match component {
            Component::OnnxRuntime => install_onnxruntime(home, client, progress).await,
            Component::Espeak => install_espeak(home, client, progress).await,
            Component::Model | Component::Voices => {
                install_kokoro(home, client, component, progress).await
            }
        };

        match &outcome {
            Ok(()) => progress.update(component, Step::Done),
            Err(error) => progress.update(
                component,
                Step::Failed {
                    reason: error.to_string(),
                },
            ),
        }
        results.push((component, outcome));
    }

    results
}

/// Fetches a URL into memory, reporting progress.
///
/// The bytes are held whole because both consumers parse their payload in
/// memory — a 72 MB runtime archive and a 9 MB wheel. The 325 MB model goes
/// through [`super::assets::ensure`] instead, which streams to disk.
async fn fetch<P: Progress>(
    client: &reqwest::Client,
    component: Component,
    url: &str,
    progress: &mut P,
) -> Result<Vec<u8>> {
    use futures_util::StreamExt;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Http(format!("{}: {e}", component.label())))?;

    if !response.status().is_success() {
        return Err(Error::Http(format!(
            "{}: HTTP {}",
            component.label(),
            response.status().as_u16()
        )));
    }

    let total = response.content_length();
    let mut received = 0u64;
    let mut body = Vec::with_capacity(total.unwrap_or(0) as usize);
    let mut stream = response.bytes_stream();
    let mut tick = 0u64;

    progress.update(component, Step::Downloading { received, total });

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Error::Http(format!("{}: {e}", component.label())))?;
        body.extend_from_slice(&chunk);
        received += chunk.len() as u64;

        // Redraw roughly every 2 MB: often enough to look alive, rarely enough
        // not to flood the event channel.
        if received - tick >= 2 * 1024 * 1024 {
            tick = received;
            progress.update(component, Step::Downloading { received, total });
        }
    }

    progress.update(component, Step::Downloading { received, total });
    Ok(body)
}

/// Writes `bytes` to `path`, creating parent directories.
fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    std::fs::write(path, bytes).map_err(|e| Error::io(path, e))
}

/// Extracts every archive entry whose base name matches into `dest`.
///
/// Some archives — the PyPI wheels — put the file they carry at a long,
/// version-specific path, so matching on the base name is the only stable way
/// to find it. Returns how many entries were written.
fn extract_named(archive: &[u8], wanted: &str, dest: &Path) -> Result<usize> {
    let reader = std::io::Cursor::new(archive);
    let mut zip = zip::ZipArchive::new(reader)
        .map_err(|e| Error::Http(format!("not a readable archive: {e}")))?;

    let mut written = 0usize;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| Error::Http(format!("archive entry {index}: {e}")))?;

        let name = entry.name().to_string();
        let base = name.rsplit('/').next().unwrap_or(&name);
        if !base.eq_ignore_ascii_case(wanted) {
            continue;
        }

        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| Error::Http(format!("reading {name}: {e}")))?;
        write(&dest.join(wanted), &bytes)?;
        written += 1;
    }

    if written == 0 {
        return Err(Error::Http(format!(
            "the archive does not contain {wanted}"
        )));
    }
    Ok(written)
}

/// Extracts a directory subtree out of an archive, flattened under `dest`.
///
/// The wheels nest their payload several levels deep
/// (`espeak_loader.data/data/lib/espeak-ng-data/...`), and only the part from
/// `marker` onward is wanted.
fn extract_tree(archive: &[u8], marker: &str, dest: &Path) -> Result<usize> {
    let reader = std::io::Cursor::new(archive);
    let mut zip = zip::ZipArchive::new(reader)
        .map_err(|e| Error::Http(format!("not a readable archive: {e}")))?;

    let mut written = 0usize;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| Error::Http(format!("archive entry {index}: {e}")))?;

        let name = entry.name().to_string();
        if !name.contains(marker) || name.ends_with('/') {
            continue;
        }

        // Keep the tail from the marker onward, so the layout on disk is
        // `<dest>/<marker>/<rest>` and matches what espeak-ng expects.
        let tail = name
            .split_once(marker)
            .map(|(_, rest)| format!("{marker}{rest}"))
            .unwrap_or_else(|| name.clone());

        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| Error::Http(format!("reading {name}: {e}")))?;
        write(&dest.join(tail), &bytes)?;
        written += 1;
    }

    if written == 0 {
        return Err(Error::Http(format!(
            "the archive does not contain {marker}"
        )));
    }
    Ok(written)
}

/// ONNX Runtime, from its official release archive.
async fn install_onnxruntime<P: Progress>(
    home: &Path,
    client: &reqwest::Client,
    progress: &mut P,
) -> Result<()> {
    let component = Component::OnnxRuntime;
    let (archive_name, url) = onnxruntime_source();

    let bytes = fetch(client, component, &url, progress).await?;
    progress.update(component, Step::Extracting);

    let dest = home.join("ort");
    std::fs::create_dir_all(&dest).map_err(|e| Error::io(&dest, e))?;

    // The archive extracts to `onnxruntime-<platform>-<version>/lib/`, so the
    // library is copied to a flat location that `Paths` can predict.
    extract_named(&bytes, runtime_library(), &dest).map_err(|error| {
        Error::Http(format!("{archive_name}: {error}"))
    })?;

    Ok(())
}

/// The ONNX Runtime archive name and URL for this platform.
///
/// **Pinned to match the `ort` crate.** 2.0.0-rc.13 wraps ONNX Runtime 1.28,
/// and a mismatch is not benign: 1.22.0 loaded, inferred, and then aborted the
/// process during teardown with `STATUS_STACK_BUFFER_OVERRUN`. The version is
/// therefore a constant here and in `scripts/fetch-onnxruntime.py`, and the two
/// have to move together.
pub fn onnxruntime_source() -> (String, String) {
    const VERSION: &str = ONNX_RUNTIME_VERSION;
    let platform = if cfg!(windows) {
        "win-x64"
    } else if cfg!(target_os = "macos") {
        // Apple silicon is the common developer machine; Intel Macs would need
        // a runtime check this does not yet do.
        "osx-arm64"
    } else {
        "linux-x64"
    };
    let archive = format!("onnxruntime-{platform}-{VERSION}.zip");
    let url = format!(
        "https://github.com/microsoft/onnxruntime/releases/download/v{VERSION}/{archive}"
    );
    (archive, url)
}

/// The ONNX Runtime the `ort` crate wraps. Changing this means changing
/// `scripts/fetch-onnxruntime.py` too.
pub const ONNX_RUNTIME_VERSION: &str = "1.28.2";

/// espeak-ng, from the `espeakng-loader` wheel.
async fn install_espeak<P: Progress>(
    home: &Path,
    client: &reqwest::Client,
    progress: &mut P,
) -> Result<()> {
    let component = Component::Espeak;
    let url = espeak_wheel_url(client).await?;

    let bytes = fetch(client, component, &url, progress).await?;
    progress.update(component, Step::Extracting);

    let dest = home.join("espeak");
    std::fs::create_dir_all(&dest).map_err(|e| Error::io(&dest, e))?;

    extract_named(&bytes, espeak_library(), &dest)?;
    // espeak-ng will not initialise without its data directory, and it reports
    // that as a bare error code rather than a missing path.
    extract_tree(&bytes, "espeak-ng-data", &dest)?;

    Ok(())
}

/// Resolves the newest `espeakng-loader` wheel for this platform from PyPI.
///
/// Resolved rather than pinned because the URL carries a content hash that
/// changes with every release, and the package is a thin wrapper around a
/// stable upstream — there is no ABI here to protect, unlike ONNX Runtime.
async fn espeak_wheel_url(client: &reqwest::Client) -> Result<String> {
    let response = client
        .get("https://pypi.org/pypi/espeakng-loader/json")
        .send()
        .await
        .map_err(|e| Error::Http(format!("pypi: {e}")))?;

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| Error::Http(format!("pypi returned unexpected JSON: {e}")))?;

    let releases = body
        .get("releases")
        .and_then(|value| value.as_object())
        .ok_or_else(|| Error::Http("pypi response has no releases".to_string()))?;

    // Prefer a platform wheel, then a pure-Python one. Newest version wins.
    let mut best: Option<(Vec<u32>, u8, String)> = None;

    for (version, files) in releases {
        let Some(files) = files.as_array() else {
            continue;
        };
        let rank = if cfg!(windows) {
            2
        } else if cfg!(target_os = "macos") {
            1
        } else {
            0
        };

        for file in files {
            let name = file.get("filename").and_then(|v| v.as_str()).unwrap_or("");
            let yanked = file
                .get("yanked")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let url = file
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if yanked || !name.ends_with(".whl") || url.is_empty() {
                continue;
            }
            if !name.contains("win_amd64") && !name.contains("none-any") {
                continue;
            }

            let key: Vec<u32> = version
                .split('.')
                .map(|part| part.parse().unwrap_or(0))
                .collect();
            let platform_rank = u8::from(name.contains("win_amd64")).max(rank.min(1));

            if best.as_ref().is_none_or(|(v, r, _)| (&key, platform_rank) > (v, *r)) {
                best = Some((key, platform_rank, url));
            }
        }
    }

    best.map(|(_, _, url)| url)
        .ok_or_else(|| Error::Http("no usable espeakng-loader wheel on PyPI".to_string()))
}

/// The Kokoro files, through the streaming downloader.
async fn install_kokoro<P: Progress>(
    home: &Path,
    client: &reqwest::Client,
    component: Component,
    progress: &mut P,
) -> Result<()> {
    let artifact = manifest::artifact(component.id())
        .ok_or_else(|| Error::Http(format!("unknown artifact {}", component.id())))?;

    let dir: PathBuf = home.join("voice");

    // `assets::ensure` streams to disk and verifies against the pin; the
    // progress callback is adapted to this module's richer step type.
    let mut adapter = |_artifact: &manifest::Artifact, received: u64, total: Option<u64>| {
        progress.update(component, Step::Downloading { received, total });
    };

    super::assets::ensure(client, artifact, &dir, &mut adapter).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&path);
        let _ = std::fs::create_dir_all(&path);
        path
    }

    /// Writes a stub file, creating the directories it needs.
    ///
    /// `std::fs::write` does not create parents, so a fixture that writes
    /// `home/ort/onnxruntime.dll` without this panics on a missing directory
    /// and the test never reaches its assertion.
    fn stub(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("could not create the fixture directory");
        }
        std::fs::write(path, b"stub").expect("could not write the fixture file");
    }

    #[test]
    fn components_are_listed_in_dependency_order() {
        // The runtime and phonemizer are useless without each other, but the
        // model is the largest download, so it goes last: a user who stops
        // early has the small pieces rather than a 325 MB orphan.
        assert_eq!(
            Component::ALL.map(|c| c.id()),
            ["onnxruntime", "espeak", "kokoro-model", "kokoro-voices"]
        );
    }

    #[test]
    fn every_component_has_a_label_licence_and_size() {
        for component in Component::ALL {
            assert!(!component.label().is_empty());
            assert!(!component.licence().is_empty());
            assert!(component.expected_bytes() > 0);
        }
    }

    #[test]
    fn espeak_is_the_only_gpl_component() {
        // The one licence fact a user has to be told about before downloading.
        for component in Component::ALL {
            let gpl = component.licence().contains("GPL");
            assert_eq!(
                gpl,
                component == Component::Espeak,
                "{} has an unexpected licence {}",
                component.id(),
                component.licence()
            );
        }
    }

    #[test]
    fn component_ids_match_the_manifest() {
        // The Kokoro components are downloaded through `manifest`, so an id
        // that does not match would fail at install time rather than here.
        assert!(manifest::artifact("kokoro-model").is_some());
        assert!(manifest::artifact("kokoro-voices").is_some());
        assert_eq!(Component::Model.id(), "kokoro-model");
        assert_eq!(Component::Voices.id(), "kokoro-voices");
    }

    #[test]
    fn step_labels_read_sensibly() {
        assert_eq!(Step::NotInstalled.label(), "not installed");
        assert_eq!(Step::Done.label(), "installed");
        assert!(Step::Extracting.label().contains("extract"));
        assert!(Step::Failed {
            reason: "no network".into()
        }
        .label()
        .contains("no network"));

        let half = Step::Downloading {
            received: 50,
            total: Some(100),
        };
        assert!(half.label().contains("50%"), "{}", half.label());

        // An unknown total must not divide by zero or claim a percentage.
        let unknown = Step::Downloading {
            received: 1_000_000,
            total: None,
        };
        assert!(!unknown.label().contains('%'));
    }

    #[test]
    fn an_empty_home_reports_nothing_installed() {
        let home = temp_home("loom-voice-install-empty");
        let entries = status(&home);
        assert_eq!(entries.len(), 4);
        for entry in &entries {
            assert_eq!(entry.step, Step::NotInstalled, "{}", entry.component.id());
        }
        assert!(!is_complete(&home));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_complete_install_is_reported_as_such() {
        let home = temp_home("loom-voice-install-full");

        // Lay out exactly the files each component is judged on.
        stub(&home.join("ort").join(runtime_library()));
        stub(&home.join("espeak").join(espeak_library()));
        std::fs::create_dir_all(home.join("espeak").join("espeak-ng-data")).unwrap();
        stub(&home.join("voice").join("kokoro-v1.0.onnx"));
        stub(&home.join("voice").join("voices-v1.0.bin"));

        let entries = status(&home);
        assert!(
            entries.iter().all(|entry| entry.step.is_done()),
            "not all components were recognised: {entries:?}"
        );
        assert!(is_complete(&home));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn espeak_needs_its_data_directory_not_just_the_library() {
        // A library without its data directory initialises to a bare error
        // code, so treating the pair as one component is deliberate.
        let home = temp_home("loom-voice-install-espeak-half");
        stub(&home.join("espeak").join(espeak_library()));

        let espeak = status(&home)
            .into_iter()
            .find(|entry| entry.component == Component::Espeak)
            .expect("espeak is always reported");
        assert_eq!(espeak.step, Step::NotInstalled);
        assert!(!is_complete(&home));

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn extracting_finds_a_named_file_at_any_depth() {
        // The wheels put their payload several directories deep.
        let home = temp_home("loom-voice-extract-named");

        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options: zip::write::SimpleFileOptions = Default::default();
            writer.start_file("deep/nested/espeak-ng.dll", options).unwrap();
            use std::io::Write;
            writer.write_all(b"payload").unwrap();
            writer.finish().unwrap();
        }
        let archive = cursor.into_inner();

        let written = extract_named(&archive, "espeak-ng.dll", &home).unwrap();
        assert_eq!(written, 1);
        assert_eq!(std::fs::read(home.join("espeak-ng.dll")).unwrap(), b"payload");

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn extracting_a_missing_name_is_an_error_not_a_silent_success() {
        let home = temp_home("loom-voice-extract-missing");

        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options: zip::write::SimpleFileOptions = Default::default();
            writer.start_file("something-else.txt", options).unwrap();
            writer.finish().unwrap();
        }
        let archive = cursor.into_inner();

        let error = extract_named(&archive, "onnxruntime.dll", &home).unwrap_err();
        assert!(error.to_string().contains("does not contain"), "{error}");

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn extracting_a_tree_flattens_the_path_to_the_marker() {
        let home = temp_home("loom-voice-extract-tree");

        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options: zip::write::SimpleFileOptions = Default::default();
            writer
                .start_file("loader.data/data/lib/espeak-ng-data/phontab", options)
                .unwrap();
            use std::io::Write;
            writer.write_all(b"phoneme table").unwrap();
            writer
                .start_file("loader.data/data/lib/espeak-ng-data/lang/en", options)
                .unwrap();
            writer.write_all(b"english").unwrap();
            // An unrelated file in the same archive must not be unpacked.
            writer.start_file("loader.data/OTHER/ignored.txt", options).unwrap();
            writer.write_all(b"nope").unwrap();
            writer.finish().unwrap();
        }
        let archive = cursor.into_inner();

        let written = extract_tree(&archive, "espeak-ng-data", &home).unwrap();
        assert_eq!(written, 2);
        assert!(home.join("espeak-ng-data").join("phontab").exists());
        assert!(home.join("espeak-ng-data").join("lang").join("en").exists());
        assert!(!home.join("ignored.txt").exists());

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_onnxruntime_url_matches_the_version_the_ort_crate_wraps() {
        let (archive, url) = onnxruntime_source();
        assert!(archive.ends_with(".zip"));
        assert!(url.starts_with("https://"));
        // A `latest` URL would let the ABI drift out from under `ort`.
        assert!(url.contains("/releases/download/"), "{url}");
        assert!(!url.contains("/latest/"), "{url}");
        assert!(
            url.contains(ONNX_RUNTIME_VERSION),
            "{url} does not name {ONNX_RUNTIME_VERSION}"
        );
        // Pinned to 1.28, which is what ort 2.0.0-rc.13 wraps. 1.22 loaded but
        // aborted the process on teardown.
        assert_eq!(ONNX_RUNTIME_VERSION, "1.28.2");
    }

    #[test]
    fn the_runtime_library_name_matches_the_platform() {
        let name = runtime_library();
        if cfg!(windows) {
            assert_eq!(name, "onnxruntime.dll");
        } else {
            assert!(name.starts_with("libonnxruntime"));
        }
    }

    #[tokio::test]
    async fn installing_into_an_unwritable_root_fails_rather_than_pretending() {
        // A path that cannot be created: the install must report a failure
        // rather than returning `Ok` with nothing on disk.
        let client = match super::super::assets::client() {
            Ok(client) => client,
            Err(_) => return,
        };
        let bad = if cfg!(windows) {
            PathBuf::from("Z:\\nonexistent\\loom")
        } else {
            PathBuf::from("/proc/nonexistent/loom")
        };

        let mut progress = Silent;
        let results = install_all(&bad, &client, &mut progress).await;
        // Skip on a machine where that path happens to be writable.
        if results.iter().any(|(_, outcome)| outcome.is_ok()) {
            return;
        }
        assert_eq!(results.len(), Component::ALL.len());
    }
}
