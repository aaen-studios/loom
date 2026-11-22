//! How this device shows loom, as the person set it in Settings: the theme, how much of
//! an agent's work the chat shows, and how the app behaves on this computer. Kept in
//! `prefs.json` in the app's data folder, so each device has its own. What belongs to
//! the account (agent defaults, the profile) lives on the instance instead.

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
    /// Whether an agent's replies show as bubbles, as the person's messages do.
    pub bubbles: bool,
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
    pub width: Width,
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
            ctrl_enter: false,
            tray: true,
            notify: true,
            preview: true,
            sound: true,
            awake: false,
            auto_update: false,
            search: false,
            width: Width::Normal,
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
            bubbles: b("bubbles", d.bubbles),
            ctrl_enter: b("ctrl_enter", d.ctrl_enter),
            tray: b("tray", d.tray),
            notify: b("notify", d.notify),
            preview: b("preview", d.preview),
            sound: b("sound", d.sound),
            awake: b("awake", d.awake),
            auto_update: b("auto_update", d.auto_update),
            search: b("search", d.search),
            width: match s("width") {
                "comfortable" => Width::Comfortable,
                "full" => Width::Full,
                _ => d.width,
            },
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
        m.insert("bubbles".into(), json!(self.bubbles));
        m.insert("ctrl_enter".into(), json!(self.ctrl_enter));
        m.insert("tray".into(), json!(self.tray));
        m.insert("notify".into(), json!(self.notify));
        m.insert("preview".into(), json!(self.preview));
        m.insert("sound".into(), json!(self.sound));
        m.insert("awake".into(), json!(self.awake));
        m.insert("auto_update".into(), json!(self.auto_update));
        m.insert("search".into(), json!(self.search));
        m.insert(
            "width".into(),
            json!(match self.width {
                Width::Normal => "normal",
                Width::Comfortable => "comfortable",
                Width::Full => "full",
            }),
        );
        Value::Object(m)
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
    fn search_is_hidden_until_the_person_shows_it() {
        assert!(!Prefs::default().search);
        // A prefs file kept before search could be hidden still loads, with it hidden.
        let older = serde_json::json!({"theme": "dark", "bubbles": false, "tray": false});
        let p = Prefs::from_json(&older);
        assert!(!p.search && p.theme == Theme::Dark && !p.bubbles && !p.tray);
        let shown = Prefs::from_json(&serde_json::json!({"search": true}));
        assert!(shown.search);
        assert_eq!(Prefs::from_json(&shown.to_json()), shown);
    }
}
