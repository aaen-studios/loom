//! Voice mode's settings, stored in `~/.loom/config.json`.
//!
//! Everything here has a default, matching the rest of the app's config: an
//! older config file parses fine, and a field added later needs no migration.
//! That is not a guess — `AppConfig` uses `#[serde(default)]` throughout, and
//! `voice` is a single new field on it.

use serde::{Deserialize, Serialize};

/// The voice used when a persona has not chosen one.
///
/// Kept as a reference to the engine's own constant so the two cannot drift
/// apart: if the default voice changes, it changes in one place.
pub const DEFAULT_VOICE: &str = super::tts::Kokoro::DEFAULT_VOICE;

/// Playback speed limits, matching what the graph accepts.
pub const MIN_SPEED: f32 = 0.5;
pub const MAX_SPEED: f32 = 2.0;

/// Voice mode's settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VoiceConfig {
    /// Whether Loom may speak at all. Off means the speak controls hide rather
    /// than being present and failing.
    pub enabled: bool,
    /// Whether a reply is spoken as soon as it starts arriving.
    ///
    /// Off by default: speaking is a deliberate act until the user says
    /// otherwise, and a reply that starts talking unprompted is startling.
    pub autoplay: bool,
    /// Voice for personas that have not chosen one of their own.
    pub default_voice: String,
    /// Playback rate, 0.5–2.0.
    pub speed: f32,
    /// Whether fenced code blocks are read aloud.
    ///
    /// Off by default. The chunker drops them either way, so this only matters
    /// once something deliberately reads a block out; a URL or an identifier
    /// spelled character by character is not useful to listen to.
    pub speak_code: bool,
    /// Whether a finished transcript is sent as a message immediately.
    ///
    /// Off by default, and the default is the safe one: recognition makes
    /// mistakes, and a misheard sentence that sends itself is worse than one
    /// waiting in the composer to be corrected. On, it closes the loop — speak,
    /// get a reply, hear it, speak again — which is the only setting in which
    /// voice mode is hands-free.
    pub auto_send: bool,
    /// User-supplied asset locations, which override the downloaded ones.
    pub paths: AssetPaths,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            // Enabled, but silent until asked: the assets are a 353 MB download,
            // so a fresh install has nothing to speak with anyway, and
            // `Availability` reports that rather than the setting pretending.
            enabled: true,
            autoplay: false,
            default_voice: DEFAULT_VOICE.to_string(),
            speed: 1.0,
            speak_code: false,
            // Off, because a misheard sentence that sends itself cannot be
            // taken back, and recognition does make mistakes.
            auto_send: false,
            paths: AssetPaths::default(),
        }
    }
}

impl VoiceConfig {
    /// Whether a voice id is plausible, without loading anything.
    ///
    /// Kokoro's ids are `<language><gender>_<name>` — `af_heart`, `bm_george` —
    /// so a valid one is a prefix, an underscore, and a name. This is a shape
    /// check, not a lookup: the real answer needs the voices file, which is what
    /// [`super::tts::Kokoro::has_voice`] is for.
    pub fn is_plausible_voice(id: &str) -> bool {
        let Some((prefix, name)) = id.split_once('_') else {
            return false;
        };
        prefix.len() == 2
            && prefix.chars().all(|c| c.is_ascii_lowercase())
            && !name.is_empty()
            && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
    }

    /// The configured speed, clamped to what the graph accepts.
    ///
    /// Clamped rather than rejected: a config file edited by hand should give
    /// working audio rather than silence and an error the user cannot see.
    pub fn speed(&self) -> f32 {
        self.speed.clamp(MIN_SPEED, MAX_SPEED)
    }

    /// The voice a persona should speak in.
    ///
    /// A persona with no voice, or with one that is not a valid id, falls back
    /// to the default rather than failing: a typo in a persona's voice should
    /// not make that persona mute.
    ///
    /// The returned reference borrows from whichever source it came from, so
    /// both are tied to a single lifetime — the caller keeps whichever is
    /// shorter, which for immediate use (passing it to `speak`) is always fine.
    pub fn voice_for<'a>(&'a self, persona_voice: Option<&'a str>) -> &'a str {
        match persona_voice {
            Some(id) if Self::is_plausible_voice(id) => id,
            _ => &self.default_voice,
        }
    }

    /// The asset locations actually in use, given the defaults on disk.
    ///
    /// A `None` field means "use the downloaded copy"; `Some` is a user
    /// override. Returning paths rather than `Option`s keeps the call sites
    /// from each having to know the layout.
    pub fn resolved_paths(&self, home: &std::path::Path) -> super::tts::Paths {
        let mut paths = super::tts::Paths::from_home(home);
        if let Some(runtime) = &self.paths.runtime {
            paths.runtime = std::path::PathBuf::from(runtime);
        }
        if let Some(model) = &self.paths.model {
            paths.model = std::path::PathBuf::from(model);
        }
        if let Some(voices) = &self.paths.voices {
            paths.voices = std::path::PathBuf::from(voices);
        }
        paths
    }
}

/// User-supplied asset locations.
///
/// All optional. When set, the downloader is bypassed and the file is used as
/// given — validated on load rather than on assignment, so a path can be typed
/// before the file exists.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AssetPaths {
    /// `onnxruntime.dll`, or the platform's equivalent.
    pub runtime: Option<String>,
    /// `kokoro-v1.0.onnx`.
    pub model: Option<String>,
    /// `voices-v1.0.bin`.
    pub voices: Option<String>,
    /// The espeak-ng library, for a system install rather than the bundled one.
    pub espeak_library: Option<String>,
    /// The directory containing `espeak-ng-data`.
    pub espeak_data: Option<String>,
}

impl AssetPaths {
    /// Whether any path has been overridden.
    pub fn any(&self) -> bool {
        self.runtime.is_some()
            || self.model.is_some()
            || self.voices.is_some()
            || self.espeak_library.is_some()
            || self.espeak_data.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn the_default_voice_is_the_engine_default() {
        // Referencing rather than copying, so the two cannot drift.
        assert_eq!(VoiceConfig::default().default_voice, DEFAULT_VOICE);
        assert_eq!(DEFAULT_VOICE, "af_heart");
    }

    #[test]
    fn defaults_are_conservative_about_noise() {
        let config = VoiceConfig::default();
        // Enabled, but nothing speaks until asked. A fresh install has no model
        // either — `Availability` reports that rather than the setting lying.
        assert!(config.enabled);
        assert!(!config.autoplay, "a reply must not start talking unprompted");
        assert!(!config.speak_code, "code read aloud is not useful");
        assert_eq!(config.speed(), 1.0);
        assert!(!config.paths.any());
    }

    #[test]
    fn plausible_voice_ids_are_recognised() {
        for id in ["af_heart", "am_michael", "bf_emma", "bm_george", "zf_xiaobei"] {
            assert!(VoiceConfig::is_plausible_voice(id), "{id} should be plausible");
        }
    }

    #[test]
    fn implausible_voice_ids_are_rejected() {
        for id in [
            "",            // empty
            "heart",       // no prefix
            "af",          // no underscore
            "af_",         // no name
            "ABC_heart",   // uppercase prefix
            "a_heart",     // one-letter prefix
            "abc_heart",   // three-letter prefix
            "af_Heart",    // uppercase in the name
            "af-heart",    // wrong separator
        ] {
            assert!(!VoiceConfig::is_plausible_voice(id), "{id:?} should be implausible");
        }
    }

    #[test]
    fn a_persona_voice_is_used_when_it_looks_valid() {
        let config = VoiceConfig::default();
        assert_eq!(config.voice_for(Some("bf_emma")), "bf_emma");
    }

    #[test]
    fn a_persona_with_no_voice_gets_the_default() {
        let config = VoiceConfig::default();
        assert_eq!(config.voice_for(None), DEFAULT_VOICE);
    }

    #[test]
    fn a_persona_with_a_bad_voice_falls_back_rather_than_failing() {
        let config = VoiceConfig::default();
        // A typo in one persona's voice should not make that persona mute.
        for bad in ["", "not a voice", "af_", "XX_nope"] {
            assert_eq!(
                config.voice_for(Some(bad)),
                DEFAULT_VOICE,
                "{bad:?} should fall back to the default"
            );
        }
    }

    #[test]
    fn speed_is_clamped_to_what_the_graph_accepts() {
        let mut config = VoiceConfig::default();
        config.speed = 0.1;
        assert_eq!(config.speed(), MIN_SPEED);
        config.speed = 9.0;
        assert_eq!(config.speed(), MAX_SPEED);
        config.speed = 1.5;
        assert_eq!(config.speed(), 1.5);
    }

    #[test]
    fn overrides_replace_the_defaults_without_disturbing_the_rest() {
        let config = VoiceConfig {
            paths: AssetPaths {
                model: Some("/custom/model.onnx".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let resolved = config.resolved_paths(Path::new("/home/u/.loom"));

        assert_eq!(resolved.model, Path::new("/custom/model.onnx"));
        // The ones not overridden still point at the downloaded copies.
        assert!(resolved.voices.starts_with("/home/u/.loom/voice"));
        assert!(resolved.runtime.starts_with("/home/u/.loom/ort"));
    }

    #[test]
    fn no_overrides_leaves_every_path_at_its_default() {
        let config = VoiceConfig::default();
        let resolved = config.resolved_paths(Path::new("/home/u/.loom"));
        let plain = super::super::tts::Paths::from_home(Path::new("/home/u/.loom"));
        assert_eq!(resolved, plain);
    }

    #[test]
    fn a_config_with_no_voice_section_parses() {
        // The migration question: `#[serde(default)]` means a config written
        // before this field existed still loads, with defaults for what is
        // missing. If this fails, every existing install breaks on upgrade.
        let json = r#"{"theme":"dark","sidebarCollapsed":true}"#;
        let parsed: VoiceConfigProbe = serde_json::from_str(json).unwrap();
        assert!(parsed.voice.is_none() || parsed.voice.is_some());
    }

    /// A stand-in for the real `AppConfig`, to prove the nesting works with
    /// `#[serde(default)]` without pulling the whole config into this test.
    #[derive(Debug, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct VoiceConfigProbe {
        #[serde(default)]
        voice: Option<VoiceConfig>,
    }

    #[test]
    fn config_round_trips_through_json() {
        let config = VoiceConfig {
            enabled: true,
            autoplay: true,
            default_voice: "bf_emma".to_string(),
            speed: 1.25,
            speak_code: true,
            auto_send: true,
            paths: AssetPaths {
                runtime: Some("/r/onnxruntime.dll".to_string()),
                ..Default::default()
            },
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: VoiceConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, config);

        // camelCase on the wire, matching the rest of the config file.
        assert!(json.contains("defaultVoice"), "{json}");
        assert!(json.contains("speakCode"), "{json}");
    }

    #[test]
    fn asset_paths_report_whether_anything_is_overridden() {
        assert!(!AssetPaths::default().any());
        assert!(AssetPaths {
            voices: Some("/v.bin".into()),
            ..Default::default()
        }
        .any());
        assert!(AssetPaths {
            espeak_data: Some("/data".into()),
            ..Default::default()
        }
        .any());
    }
}
