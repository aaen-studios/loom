//! How this device shows loom, as the person set it in Settings: the theme, how much of
//! an agent's work the chat shows, and how the app behaves on this computer. Kept in
//! `prefs.json` in the app's data folder, so each device has its own. What belongs to
//! the account (agent defaults, the profile) lives on the instance instead.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{json, Map, Value};

/// The colours: the system's, or always light or dark.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    System,
    Light,
    Dark,
}

/// How much of an agent's tool calls a reply shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tools {
    /// None, not even one that failed: the agent says so when it matters.
    Hidden,
    /// One line for each run of them, which opens to show each.
    Folded,
    /// Every call, each on its own line.
    All,
}

/// Whether a reply shows its agent's thoughts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Thoughts {
    /// Not at all: it's a chat, and the answer is what matters.
    Hidden,
    /// Each under "Thought", to open.
    Folded,
    /// Each open.
    Open,
}

/// How big the words of the chat are.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Text {
    Small,
    Normal,
    Large,
}

/// How wide the chat's column is at most.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Width {
    Normal,
    Comfortable,
    /// As wide as the window leaves it.
    Full,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Prefs {
    pub theme: Theme,
    pub text: Text,
    /// Whether the words are in the system's font, rather than loom's own (Geist), and
    /// code in the system's monospace one, rather than Geist Mono.
    pub system_font: bool,
    pub system_mono: bool,
    pub tools: Tools,
    pub thoughts: Thoughts,
    /// Whether an agent's replies show as grey bubbles, rather than plain text in the
    /// column.
    pub bubbles: bool,
    /// Whether the person's bubbles are filled with the agent's colour itself (Vibrant),
    /// rather than a breath of it (Soft, the default), or `None` while they haven't
    /// picked, so a choice is kept when the default changes. `vibrant()` says which shows.
    pub vibrant: Option<bool>,
    /// Whether Enter makes a new line and Ctrl+Enter sends, rather than Enter sending.
    pub ctrl_enter: bool,
    /// Whether closing the window keeps loom running in the tray.
    pub tray: bool,
    /// Whether loom notifies, and whether a notification says what was said.
    pub notify: bool,
    pub preview: bool,
    /// Whether a notification makes a sound.
    pub sound: bool,
    /// Whether the computer stays awake while an agent works.
    pub awake: bool,
    /// Whether a downloaded update goes in on its own, while loom is in the tray and
    /// no agent works.
    pub auto_update: bool,
    /// Whether the sidebar shows the search box over the agents.
    pub search: bool,
    /// Settings → Emoji: whether the composer shows the emoji button, whether moving
    /// emoji play only under the pointer, the skin tone (0 for none, 1 to 5 light to
    /// dark), and whether `:)` becomes 🙂.
    pub emoji_button: bool,
    pub emoji_hover: bool,
    pub skin_tone: u32,
    pub emoticons: bool,
    pub width: Width,
    /// The names the person gave servers on this device, by address (empty for the
    /// instance in the app).
    pub server_names: BTreeMap<String, String>,
}

impl Default for Prefs {
    fn default() -> Prefs {
        Prefs {
            theme: Theme::System,
            text: Text::Normal,
            system_font: false,
            system_mono: false,
            tools: Tools::Hidden,
            thoughts: Thoughts::Hidden,
            bubbles: true,
            vibrant: None,
            ctrl_enter: false,
            tray: true,
            notify: true,
            preview: true,
            sound: true,
            awake: false,
            auto_update: false,
            search: false,
            emoji_button: true,
            emoji_hover: false,
            skin_tone: 0,
            emoticons: false,
            width: Width::Normal,
            server_names: BTreeMap::new(),
        }
    }
}

fn file(dir: &str) -> std::path::PathBuf {
    std::path::Path::new(dir).join("prefs.json")
}

impl Prefs {
    /// The prefs kept in `dir`, or the defaults for any that aren't.
    pub fn load(dir: &str) -> Prefs {
        let v: Value = std::fs::read_to_string(file(dir))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        Prefs::from_json(&v)
    }

    pub fn save(&self, dir: &str) {
        let _ = std::fs::write(file(dir), self.to_json().to_string());
    }

    pub fn from_json(v: &Value) -> Prefs {
        let d = Prefs::default();
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default();
        let b = |k: &str, or: bool| v.get(k).and_then(Value::as_bool).unwrap_or(or);
        Prefs {
            theme: match s("theme") {
                "light" => Theme::Light,
                "dark" => Theme::Dark,
                _ => d.theme,
            },
            text: match s("text") {
                "small" => Text::Small,
                "large" => Text::Large,
                _ => d.text,
            },
            system_font: b("system_font", d.system_font),
            system_mono: b("system_mono", d.system_mono),
            tools: match s("tools") {
                "folded" => Tools::Folded,
                "all" => Tools::All,
                _ => d.tools,
            },
            // Before thoughts could be hidden, `thoughts_open` chose open or folded. Every
            // file kept it, so only an open one was chosen: folded comes to hidden.
            thoughts: match (
                s("thoughts"),
                v.get("thoughts_open").and_then(Value::as_bool),
            ) {
                ("hidden", _) => Thoughts::Hidden,
                ("folded", _) => Thoughts::Folded,
                ("open", _) | ("", Some(true)) => Thoughts::Open,
                _ => d.thoughts,
            },
            // Kept under a new name each time the default changed, since every file kept
            // the old one: `bubbles` (true, for the tinted bubble replies had) and then
            // `reply_bubbles` (false, while replies were plain by default).
            bubbles: match s("replies") {
                "plain" => false,
                "bubbles" => true,
                _ => d.bubbles,
            },
            // `bubble_colour` is kept only once the person picks. Before it, 0.2.7 to 0.2.9
            // kept `your_bubbles` in every file, "vibrant" while that was the default, so
            // only its "soft" was picked; and 0.2.6 kept `vibrant_bubbles`, false in every
            // file whose person didn't touch it, so only its true was.
            vibrant: match (s("bubble_colour"), s("your_bubbles")) {
                ("vibrant", _) => Some(true),
                ("soft", _) | (_, "soft") => Some(false),
                (_, "") if b("vibrant_bubbles", false) => Some(true),
                _ => d.vibrant,
            },
            ctrl_enter: b("ctrl_enter", d.ctrl_enter),
            tray: b("tray", d.tray),
            notify: b("notify", d.notify),
            preview: b("preview", d.preview),
            sound: b("sound", d.sound),
            awake: b("awake", d.awake),
            auto_update: b("auto_update", d.auto_update),
            search: b("search", d.search),
            emoji_button: b("emoji_button", d.emoji_button),
            emoji_hover: b("emoji_hover", d.emoji_hover),
            skin_tone: v
                .get("skin_tone")
                .and_then(Value::as_u64)
                .filter(|t| *t <= 5)
                .map_or(d.skin_tone, |t| t as u32),
            emoticons: b("emoticons", d.emoticons),
            width: match s("width") {
                "comfortable" => Width::Comfortable,
                "full" => Width::Full,
                _ => d.width,
            },
            server_names: v
                .get("server_names")
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                        .filter(|(_, v)| !v.trim().is_empty())
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert(
            "theme".into(),
            json!(match self.theme {
                Theme::System => "system",
                Theme::Light => "light",
                Theme::Dark => "dark",
            }),
        );
        m.insert(
            "text".into(),
            json!(match self.text {
                Text::Small => "small",
                Text::Normal => "normal",
                Text::Large => "large",
            }),
        );
        m.insert(
            "tools".into(),
            json!(match self.tools {
                Tools::Hidden => "hidden",
                Tools::Folded => "folded",
                Tools::All => "all",
            }),
        );
        m.insert(
            "thoughts".into(),
            json!(match self.thoughts {
                Thoughts::Hidden => "hidden",
                Thoughts::Folded => "folded",
                Thoughts::Open => "open",
            }),
        );
        m.insert("system_font".into(), json!(self.system_font));
        m.insert("system_mono".into(), json!(self.system_mono));
        m.insert(
            "replies".into(),
            json!(if self.bubbles { "bubbles" } else { "plain" }),
        );
        if let Some(v) = self.vibrant {
            m.insert(
                "bubble_colour".into(),
                json!(if v { "vibrant" } else { "soft" }),
            );
        }
        m.insert("ctrl_enter".into(), json!(self.ctrl_enter));
        m.insert("tray".into(), json!(self.tray));
        m.insert("notify".into(), json!(self.notify));
        m.insert("preview".into(), json!(self.preview));
        m.insert("sound".into(), json!(self.sound));
        m.insert("awake".into(), json!(self.awake));
        m.insert("auto_update".into(), json!(self.auto_update));
        m.insert("search".into(), json!(self.search));
        m.insert("emoji_button".into(), json!(self.emoji_button));
        m.insert("emoji_hover".into(), json!(self.emoji_hover));
        m.insert("skin_tone".into(), json!(self.skin_tone));
        m.insert("emoticons".into(), json!(self.emoticons));
        m.insert(
            "width".into(),
            json!(match self.width {
                Width::Normal => "normal",
                Width::Comfortable => "comfortable",
                Width::Full => "full",
            }),
        );
        m.insert("server_names".into(), json!(self.server_names));
        Value::Object(m)
    }

    /// The name the person gave the server at `key`, if they gave it one.
    pub fn server_name(&self, key: &str) -> Option<String> {
        self.server_names.get(key).cloned()
    }

    /// Names the server at `key`, or with an empty name, takes its name away.
    pub fn rename_server(&mut self, key: &str, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.server_names.remove(key);
        } else {
            self.server_names.insert(key.to_string(), name.to_string());
        }
    }

    /// Whether the person's bubbles show Vibrant: only once they've picked it.
    pub fn vibrant(&self) -> bool {
        self.vibrant.unwrap_or(false)
    }

    /// Whether the window is dark, for a system that's `system_dark`.
    pub fn dark(&self, system_dark: bool) -> bool {
        match std::env::var("LOOM_THEME").as_deref() {
            Ok("dark") => return true,
            Ok("light") => return false,
            _ => {}
        }
        match self.theme {
            Theme::System => system_dark,
            Theme::Light => false,
            Theme::Dark => true,
        }
    }

    /// The chat column's widest, in pixels.
    pub fn column(&self) -> f32 {
        match self.width {
            Width::Normal => crate::tokens::space::COLUMN,
            Width::Comfortable => crate::tokens::space::COLUMN_WIDE,
            Width::Full => f32::MAX,
        }
    }

    /// How much bigger or smaller than the design's sizes the chat's words are.
    pub fn text_scale(&self) -> f32 {
        match self.text {
            Text::Small => 0.9,
            Text::Normal => 1.0,
            Text::Large => 1.15,
        }
    }
}

/// The fonts in use, as the prefs last set them, for every place that draws words.
static SYSTEM_FONT: AtomicBool = AtomicBool::new(false);
static SYSTEM_MONO: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
const SYSTEM_UI: &str = "Segoe UI";
#[cfg(windows)]
const SYSTEM_CODE: &str = "Consolas";
#[cfg(not(windows))]
const SYSTEM_UI: &str = ".SystemUIFont";
#[cfg(not(windows))]
const SYSTEM_CODE: &str = "Menlo";

/// Puts `p`'s fonts in use.
pub fn use_fonts(p: &Prefs) {
    SYSTEM_FONT.store(p.system_font, Ordering::Relaxed);
    SYSTEM_MONO.store(p.system_mono, Ordering::Relaxed);
}

/// The font for words.
pub fn ui_font() -> &'static str {
    if SYSTEM_FONT.load(Ordering::Relaxed) {
        SYSTEM_UI
    } else {
        crate::tokens::font::UI
    }
}

/// The font for code.
pub fn mono_font() -> &'static str {
    if SYSTEM_MONO.load(Ordering::Relaxed) {
        SYSTEM_CODE
    } else {
        crate::tokens::font::MONO
    }
}

#[cfg(test)]
mod tests {
    use super::{Prefs, Theme, Thoughts, Tools, Width};
    use serde_json::json;

    #[test]
    fn tool_calls_are_hidden_until_the_person_asks_for_them() {
        assert_eq!(Prefs::default().tools, Tools::Hidden);
        assert_eq!(Prefs::from_json(&serde_json::Value::Null), Prefs::default());
    }

    #[test]
    fn thoughts_are_hidden_until_the_person_asks_for_them() {
        assert_eq!(Prefs::default().thoughts, Thoughts::Hidden);
        // An older file, from when thoughts were open or folded.
        let old = |open: bool| Prefs::from_json(&json!({"thoughts_open": open})).thoughts;
        assert_eq!(old(true), Thoughts::Open);
        assert_eq!(old(false), Thoughts::Hidden);
        let now = |v: &str| Prefs::from_json(&json!({"thoughts": v, "thoughts_open": true}));
        assert_eq!(now("folded").thoughts, Thoughts::Folded);
        assert_eq!(now("hidden").thoughts, Thoughts::Hidden);
        let kept = Prefs {
            thoughts: Thoughts::Folded,
            ..Prefs::default()
        };
        assert_eq!(Prefs::from_json(&kept.to_json()), kept);
    }

    #[test]
    fn prefs_come_back_as_they_were_kept() {
        let p = Prefs {
            theme: Theme::Dark,
            tools: Tools::Folded,
            ctrl_enter: true,
            sound: false,
            search: true,
            width: Width::Comfortable,
            ..Prefs::default()
        };
        assert_eq!(Prefs::from_json(&p.to_json()), p);
        assert_eq!(Prefs::default().width, Width::Normal);
    }

    #[test]
    fn emoji_settings_are_kept_and_start_as_planned() {
        let d = Prefs::default();
        // The button shows, moving emoji play, no skin tone, and :) stays text.
        assert!(d.emoji_button && !d.emoji_hover && d.skin_tone == 0 && !d.emoticons);
        let p = Prefs {
            emoji_button: false,
            emoji_hover: true,
            skin_tone: 3,
            emoticons: true,
            ..Prefs::default()
        };
        assert_eq!(Prefs::from_json(&p.to_json()), p);
        // A tone past the six hands is none.
        assert_eq!(
            Prefs::from_json(&serde_json::json!({"skin_tone": 9})).skin_tone,
            0
        );
    }

    #[test]
    fn a_server_keeps_the_name_the_person_gave_it() {
        let mut p = Prefs::default();
        assert_eq!(p.server_name("https://vps:8443"), None);
        p.rename_server("https://vps:8443", "  Home  ");
        assert_eq!(p.server_name("https://vps:8443").as_deref(), Some("Home"));
        assert_eq!(Prefs::from_json(&p.to_json()), p);
        p.rename_server("https://vps:8443", " ");
        assert_eq!(p.server_name("https://vps:8443"), None);
    }

    #[test]
    fn search_is_hidden_until_the_person_shows_it() {
        assert!(!Prefs::default().search);
        // A prefs file kept before search could be hidden still loads, with it hidden.
        let older = serde_json::json!({"theme": "dark", "replies": "plain", "tray": false});
        let p = Prefs::from_json(&older);
        assert!(!p.search && p.theme == Theme::Dark && !p.bubbles && !p.tray);
        let shown = Prefs::from_json(&serde_json::json!({"search": true}));
        assert!(shown.search);
        assert_eq!(Prefs::from_json(&shown.to_json()), shown);
    }

    #[test]
    fn replies_are_grey_bubbles_until_the_person_picks_plain() {
        assert!(Prefs::default().bubbles);
        // Every file kept while replies were plain by default has `reply_bubbles` off,
        // and every one before that `bubbles` on: both come to the grey bubble.
        let plain_then = Prefs::from_json(&serde_json::json!({"reply_bubbles": false}));
        assert!(plain_then.bubbles);
        let older = Prefs::from_json(&serde_json::json!({"bubbles": false}));
        assert!(older.bubbles);
        let picked = Prefs {
            bubbles: false,
            ..Prefs::default()
        };
        assert_eq!(Prefs::from_json(&picked.to_json()), picked);
    }

    #[test]
    fn the_persons_bubbles_are_soft_until_they_pick_vibrant() {
        let fresh = Prefs::default();
        assert!(!fresh.vibrant());
        assert!(
            fresh.to_json().get("bubble_colour").is_none(),
            "nothing is kept until the person picks"
        );
        let load = |v| Prefs::from_json(&v).vibrant;
        // 0.2.7 to 0.2.9 kept "vibrant" in every file while it was the default.
        assert_eq!(load(json!({ "your_bubbles": "vibrant" })), None);
        assert_eq!(load(json!({ "your_bubbles": "soft" })), Some(false));
        // 0.2.6 kept false in every file, and true only where the person turned it on.
        assert_eq!(load(json!({ "vibrant_bubbles": false })), None);
        assert_eq!(load(json!({ "vibrant_bubbles": true })), Some(true));
        assert_eq!(
            load(json!({ "bubble_colour": "vibrant", "your_bubbles": "soft" })),
            Some(true)
        );
        for picked in [true, false] {
            let p = Prefs {
                vibrant: Some(picked),
                ..Prefs::default()
            };
            assert_eq!(Prefs::from_json(&p.to_json()), p);
            assert_eq!(p.vibrant(), picked);
        }
    }
}
