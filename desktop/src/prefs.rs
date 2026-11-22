//! How this device shows loom, as the person set it in Settings: the theme, how much of
//! an agent's work the chat shows, and how the app behaves on this computer. Kept in
//! `prefs.json` in the app's data folder, so each device has its own. What belongs to
//! the account (agent defaults, the profile) lives on the instance instead.

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
    /// None, but for one that failed or was denied.
    Hidden,
    /// One line for each run of them, which opens to show each.
    Folded,
    /// Every call, each on its own line.
    All,
}

/// How big the words of the chat are.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Text {
    Small,
    Normal,
    Large,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Prefs {
    pub theme: Theme,
    pub text: Text,
    pub tools: Tools,
    /// Whether an agent's thoughts show open, rather than folded under "Thought".
    pub thoughts_open: bool,
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
}

impl Default for Prefs {
    fn default() -> Prefs {
        Prefs {
            theme: Theme::System,
            text: Text::Normal,
            tools: Tools::Hidden,
            thoughts_open: false,
            bubbles: true,
            ctrl_enter: false,
            tray: true,
            notify: true,
            preview: true,
            sound: true,
            awake: false,
            auto_update: false,
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
            tools: match s("tools") {
                "folded" => Tools::Folded,
                "all" => Tools::All,
                _ => d.tools,
            },
            thoughts_open: b("thoughts_open", d.thoughts_open),
            bubbles: b("bubbles", d.bubbles),
            ctrl_enter: b("ctrl_enter", d.ctrl_enter),
            tray: b("tray", d.tray),
            notify: b("notify", d.notify),
            preview: b("preview", d.preview),
            sound: b("sound", d.sound),
            awake: b("awake", d.awake),
            auto_update: b("auto_update", d.auto_update),
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
        m.insert("thoughts_open".into(), json!(self.thoughts_open));
        m.insert("bubbles".into(), json!(self.bubbles));
        m.insert("ctrl_enter".into(), json!(self.ctrl_enter));
        m.insert("tray".into(), json!(self.tray));
        m.insert("notify".into(), json!(self.notify));
        m.insert("preview".into(), json!(self.preview));
        m.insert("sound".into(), json!(self.sound));
        m.insert("awake".into(), json!(self.awake));
        m.insert("auto_update".into(), json!(self.auto_update));
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

    /// How much bigger or smaller than the design's sizes the chat's words are.
    pub fn text_scale(&self) -> f32 {
        match self.text {
            Text::Small => 0.9,
            Text::Normal => 1.0,
            Text::Large => 1.15,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Prefs, Theme, Tools};

    #[test]
    fn tool_calls_are_hidden_until_the_person_asks_for_them() {
        assert_eq!(Prefs::default().tools, Tools::Hidden);
        assert_eq!(Prefs::from_json(&serde_json::Value::Null), Prefs::default());
    }

    #[test]
    fn prefs_come_back_as_they_were_kept() {
        let p = Prefs {
            theme: Theme::Dark,
            tools: Tools::Folded,
            ctrl_enter: true,
            sound: false,
            ..Prefs::default()
        };
        assert_eq!(Prefs::from_json(&p.to_json()), p);
    }
}
