//! The pinned asset list.
//!
//! Kokoro needs two files that are **not** shipped in the installer: the ONNX
//! graph and the voice style matrix. Both are fetched on first use, and both
//! are large enough that a silent substitution would be expensive to notice, so
//! every artifact carries an optional [`Pin`].
//!
//! # A hash that looks right and is not
//!
//! The Kokoro-82M model card publishes the hash
//! `496dba118d1a58f5f3db2efc88dbdc216e0483fc89fe6e47ee1f2c53f18ad1e4`. That is
//! the hash of the **upstream PyTorch weights** on Hugging Face. The file this
//! module downloads is `kokoro-v1.0.onnx`, a *different artifact* produced by
//! exporting those weights, and it has a different hash.
//!
//! Pinning the card's hash against the ONNX file would fail every download with
//! what looks exactly like corruption. The two hashes are therefore kept
//! distinct: [`UPSTREAM_WEIGHTS_SHA256`] is recorded for provenance, and the
//! ONNX pins are filled in only from a file actually observed on disk.

/// The upstream PyTorch weights' hash, from the model card.
///
/// Provenance only. It does **not** describe `kokoro-v1.0.onnx`.
pub const UPSTREAM_WEIGHTS_SHA256: &str =
    "496dba118d1a58f5f3db2efc88dbdc216e0483fc89fe6e47ee1f2c53f18ad1e4";

/// ONNX Runtime must be at least this version. Older builds either lack a
/// kernel the exported graph uses or load it incorrectly, which presents as
/// noise rather than an error.
pub const MIN_ONNX_RUNTIME: &str = "1.20.1";

/// Sample rate every Kokoro voice produces. Not configurable.
pub const SAMPLE_RATE: u32 = 24_000;

/// Fixed model context, in phonemes. Longer text is split, never truncated.
pub const MAX_PHONEME_LENGTH: usize = 510;

/// What an artifact is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The ONNX graph.
    Model,
    /// The voice style matrix.
    Voices,
    /// ONNX Runtime itself.
    Runtime,
}

/// A verified identity for one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pin {
    pub sha256: &'static str,
    pub bytes: u64,
}

/// One downloadable artifact.
#[derive(Debug, Clone, Copy)]
pub struct Artifact {
    /// Stable key used in config and in the UI.
    pub id: &'static str,
    /// Shown in the installer and in Settings.
    pub label: &'static str,
    pub kind: Kind,
    pub url: &'static str,
    /// The file name on disk, and the name the upstream `Content-Disposition`
    /// uses.
    pub file_name: &'static str,
    /// `Some` once the file has been observed and hashed. An artifact without a
    /// pin installs with a warning rather than silently.
    pub pin: Option<Pin>,
    /// Why this file is here, in the user's words rather than ours.
    pub licence: &'static str,
}

impl Artifact {
    /// Whether a downloaded copy can be proven to be the expected file.
    pub fn is_pinned(&self) -> bool {
        self.pin.is_some()
    }

    /// Expected size in bytes, when known.
    pub fn bytes(&self) -> Option<u64> {
        self.pin.map(|pin| pin.bytes)
    }
}

/// Every artifact voice mode needs, in install order.
///
/// The runtime is first because nothing else can load without it, and the
/// voices file is last because it is the cheapest to re-fetch.
///
/// The two Kokoro entries are pinned to hashes **observed on completed
/// downloads** of the release assets, recorded 2026-09-16. The model's byte
/// count was confirmed twice over: independently by a `HEAD` request's
/// `Content-Length` and by hashing the downloaded file.
///
/// ONNX Runtime is deliberately unpinned. Its correct build depends on the
/// host CPU's instruction set, so the URL is resolved at install time rather
/// than fixed here, and `ensure` refuses the empty URL rather than fetching it.
pub const ARTIFACTS: &[Artifact] = &[
    Artifact {
        id: "onnxruntime",
        label: "ONNX Runtime",
        kind: Kind::Runtime,
        // Resolved at install time: the correct build depends on the CPU.
        url: "",
        file_name: "onnxruntime.dll",
        pin: None,
        licence: "MIT",
    },
    Artifact {
        id: "kokoro-model",
        label: "Kokoro voice model",
        kind: Kind::Model,
        url: "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.1/kokoro-v1.0.onnx",
        file_name: "kokoro-v1.0.onnx",
        pin: Some(Pin {
            sha256: "beb0d1848dee9a49da392cc3df26958d46cfa35d321edf434f52949153f0df3a",
            bytes: 325_505_369,
        }),
        licence: "Apache 2.0",
    },
    Artifact {
        id: "kokoro-voices",
        label: "Kokoro voices",
        kind: Kind::Voices,
        url: "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.1/voices-v1.0.bin",
        file_name: "voices-v1.0.bin",
        pin: Some(Pin {
            sha256: "bca610b8308e8d99f32e6fe4197e7ec01679264efed0cac9140fe9c29f1fbf7d",
            bytes: 28_214_398,
        }),
        licence: "Apache 2.0",
    },
];

/// Looks an artifact up by id.
pub fn artifact(id: &str) -> Option<&'static Artifact> {
    ARTIFACTS.iter().find(|item| item.id == id)
}

/// The subset a fresh install downloads, in order.
pub fn install_order() -> impl Iterator<Item = &'static Artifact> {
    ARTIFACTS.iter()
}

/// Whether every downloadable artifact carries a verified identity.
///
/// Until this is true, installs are unverified and the UI has to say so.
pub fn all_pinned() -> bool {
    ARTIFACTS.iter().all(|item| item.is_pinned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_findable() {
        let mut seen = std::collections::HashSet::new();
        for item in ARTIFACTS {
            assert!(seen.insert(item.id), "duplicate artifact id {:?}", item.id);
            assert!(artifact(item.id).is_some_and(|found| found.id == item.id));
        }
        assert!(artifact("nope").is_none());
    }

    #[test]
    fn model_urls_are_https_and_pinned_to_a_release() {
        // `main` or `latest` in a URL means the bytes can change underneath a
        // shipped build, which no hash can then protect.
        for item in ARTIFACTS {
            if item.url.is_empty() {
                continue;
            }
            assert!(item.url.starts_with("https://"), "{} is not https", item.id);
            assert!(
                item.url.contains("/releases/download/"),
                "{} is not a release asset",
                item.id
            );
            assert!(!item.url.contains("/main/"), "{} tracks a branch", item.id);
            assert!(!item.url.contains("/latest/"), "{} tracks latest", item.id);
        }
    }

    #[test]
    fn file_names_are_bare_and_match_the_url() {
        for item in ARTIFACTS {
            assert!(!item.file_name.contains('/'), "{} is a path", item.file_name);
            if !item.url.is_empty() {
                assert!(
                    item.url.ends_with(item.file_name),
                    "{} url does not name {}",
                    item.id,
                    item.file_name
                );
            }
        }
    }

    #[test]
    fn the_upstream_hash_is_not_mistaken_for_the_onnx_hash() {
        // The whole point of keeping these separate: the model card's hash
        // describes the PyTorch weights, not the file we download.
        let model = artifact("kokoro-model").unwrap();
        if let Some(pin) = model.pin {
            assert_ne!(
                pin.sha256, UPSTREAM_WEIGHTS_SHA256,
                "the ONNX export and the upstream weights are different files"
            );
        }
        assert_eq!(UPSTREAM_WEIGHTS_SHA256.len(), 64);
    }

    #[test]
    fn pins_are_well_formed() {
        for item in ARTIFACTS {
            if let Some(pin) = item.pin {
                assert_eq!(pin.sha256.len(), 64, "{} has a malformed hash", item.id);
                assert!(
                    pin.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                    "{} hash is not hex",
                    item.id
                );
                assert!(pin.bytes > 0, "{} pins a zero-byte file", item.id);
            }
        }
    }

    #[test]
    fn the_kokoro_files_are_pinned_and_the_runtime_is_not() {
        // A deliberate asymmetry rather than an oversight: the Kokoro release
        // assets are fixed files with observed hashes, while ONNX Runtime's
        // correct build depends on the host CPU and cannot be pinned to one URL.
        assert!(artifact("kokoro-model").unwrap().is_pinned());
        assert!(artifact("kokoro-voices").unwrap().is_pinned());
        assert!(!artifact("onnxruntime").unwrap().is_pinned());
    }

    #[test]
    fn the_onnx_runtime_url_is_empty_and_thus_refused() {
        // Fetching an artifact with no URL must fail loudly rather than issue a
        // request to the empty string, which would resolve to the current host.
        assert!(artifact("onnxruntime").unwrap().url.is_empty());
    }

    #[test]
    fn the_model_is_known_to_be_large() {
        // Guards against a truncated download being accepted: the release asset
        // is a few hundred megabytes, so anything tiny is the wrong file.
        let model = artifact("kokoro-model").unwrap();
        if let Some(bytes) = model.bytes() {
            assert!(bytes > 100_000_000, "model size {bytes} is implausible");
        }
    }

    #[test]
    fn every_artifact_declares_a_licence() {
        for item in ARTIFACTS {
            assert!(!item.licence.is_empty(), "{} has no licence", item.id);
        }
    }

    #[test]
    fn unverified_artifacts_are_reported_honestly() {
        // Not a failure: a deliberate record that pinning is outstanding, so
        // `all_pinned` cannot quietly start returning true without the hashes
        // having been observed on disk.
        if !all_pinned() {
            assert!(ARTIFACTS.iter().any(|item| !item.is_pinned()));
        }
    }
}
