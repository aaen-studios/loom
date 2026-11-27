//! Voice in the desktop's model (PLAN.md, "Voice"): the person's settings and tiles as
//! the instance gives them (`voice.get`, `voice.updated`), what Settings › Voice has
//! open, an engine's voices for the picker, the voice_setup card in the chat, and the
//! words dictation puts in the composer. The core does the dictating and the speaking;
//! this only keeps what the screens draw.

use std::ops::Range;

use serde_json::{json, Value};

use super::s;

/// The id the page's commands go with.
const ASK: &str = "app.voice_";

/// The tabs beside Settings › Voice's title.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Dictation,
    Speech,
}

impl Tab {
    pub fn kind(self) -> &'static str {
        match self {
            Tab::Dictation => "dictation",
            Tab::Speech => "speech",
        }
    }
}

/// An engine, as its tile shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tile {
    pub id: String,
    pub name: String,
    pub line: String,
    /// `cloud` for a service, `laptop` for a model on the device.
    pub icon: String,
    pub streams: bool,
    /// Whether it needs a key, and whether the person has it.
    pub key: bool,
    pub have: bool,
    pub in_use: bool,
}

impl Tile {
    pub fn of(v: &Value) -> Tile {
        let b = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
        Tile {
            id: s(v, "id"),
            name: s(v, "name"),
            line: s(v, "line"),
            icon: s(v, "icon"),
            streams: b("streams"),
            key: b("key"),
            have: b("have"),
            in_use: b("in_use"),
        }
    }

    /// The tile's button: Use, Add key or Change key, or none for the one in use with
    /// no key to change.
    pub fn action(&self) -> Option<Action> {
        if self.key && !self.have {
            return Some(Action::AddKey);
        }
        if !self.in_use {
            return Some(Action::Use);
        }
        self.key.then_some(Action::ChangeKey)
    }
}

/// What a tile's button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Use,
    AddKey,
    ChangeKey,
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::Use => "Use",
            Action::AddKey => "Add key",
            Action::ChangeKey => "Change key",
        }
    }
}

/// A menu open on the page: the language's, or the voice's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    Language,
    Voice,
}

/// Dictation on screen: where its words start in the composer, and how many bytes of
/// them are there now, which the next words take the place of.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dictating {
    pub state: String,
    pub at: usize,
    pub len: usize,
    pub started: bool,
}

#[derive(Default)]
pub struct Voice {
    /// `voice.get`'s answer came.
    pub came: bool,
    pub settings: Value,
    pub dictation: Vec<Tile>,
    pub speech: Vec<Tile>,
    pub languages: Vec<(String, String)>,
    pub tab: Tab,
    /// The engine whose key field is open, by its id and kind.
    pub adding: Option<(Tab, String)>,
    pub menu: Option<Menu>,
    /// Each speech engine's voices, `(id, name)`, as the core fetched them, or why not.
    pub voices: std::collections::HashMap<String, Result<Vec<(String, String)>, String>>,
    /// A dictation under way.
    pub dictating: Option<Dictating>,
    /// What `core.dictation` said last, for the composer: its words, once they changed.
    pub words: Option<String>,
    /// Settings › Voice is to open, as dictation has no engine.
    pub open_page: bool,
    /// Why dictation or speech failed, for a notice.
    pub said: Option<String>,
    next: u64,
}

impl Voice {
    fn cmd(&mut self, name: &str, args: Value) -> Value {
        self.next += 1;
        json!({"t": "cmd", "id": format!("{ASK}{}", self.next), "name": name, "args": args})
    }

    /// Asks for the settings, as the page opens or the connection is up.
    pub fn load(&mut self) -> Value {
        self.cmd("voice.get", json!({}))
    }

    /// Takes the settings whole, as `voice.get` and `voice.updated` give them.
    pub fn took(&mut self, v: &Value) {
        self.came = true;
        // A key added or an engine changed: voices that didn't come are asked for again.
        self.voices
            .retain(|_, got| got.as_ref().is_ok_and(|l| !l.is_empty()));
        self.settings = v.get("settings").cloned().unwrap_or(Value::Null);
        let tiles = |k: &str| -> Vec<Tile> {
            v.pointer(&format!("/engines/{k}"))
                .and_then(Value::as_array)
                .map(|a| a.iter().map(Tile::of).collect())
                .unwrap_or_default()
        };
        self.dictation = tiles("dictation");
        self.speech = tiles("speech");
        self.languages = v
            .get("languages")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|l| (s(l, "code"), s(l, "name"))).collect())
            .unwrap_or_default();
    }

    /// A frame for voice from the core or the instance. Gives whether it was one.
    pub fn take(&mut self, f: &Value) -> bool {
        let t = s(f, "t");
        if t == "result" && s(f, "id").starts_with(ASK) {
            if let Some(v) = f.pointer("/value/voice") {
                self.took(v);
            } else if let Some(e) = f.get("error") {
                self.said = Some(s(e, "message"));
            }
            return true;
        }
        if t == "event" && s(f, "kind") == "voice.updated" {
            if let Some(v) = f.get("voice") {
                self.took(v);
            }
            return true;
        }
        if t == "core.voices" {
            let list = f
                .get("voices")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(|v| (s(v, "id"), s(v, "name"))).collect())
                .unwrap_or_default();
            let got = match f.get("error").and_then(Value::as_str) {
                Some(e) => Err(e.to_string()),
                None => Ok(list),
            };
            self.voices.insert(s(f, "engine"), got);
            return true;
        }
        if t == "core.dictation" {
            let state = s(f, "state");
            match state.as_str() {
                "no_engine" => {
                    self.dictating = None;
                    self.open_page = true;
                    self.tab = Tab::Dictation;
                }
                "failed" => {
                    self.said = Some(s(f, "why"));
                    self.words = Some(s(f, "text"));
                    if let Some(d) = &mut self.dictating {
                        d.state = state;
                    }
                }
                _ => {
                    self.words = Some(s(f, "text"));
                    if let Some(d) = &mut self.dictating {
                        d.state = state;
                    }
                }
            }
            return true;
        }
        if t == "core.speak" {
            if let Some(e) = f.get("error").and_then(Value::as_str) {
                self.said = Some(e.to_string());
            }
            return true;
        }
        false
    }

    /// The tiles of a tab.
    pub fn tiles(&self, tab: Tab) -> &[Tile] {
        match tab {
            Tab::Dictation => &self.dictation,
            Tab::Speech => &self.speech,
        }
    }

    /// A setting of a kind, as text.
    pub fn setting(&self, tab: Tab, key: &str) -> String {
        s(&self.settings[tab.kind()], key)
    }

    /// The engine in use for a kind, or "".
    pub fn engine(&self, tab: Tab) -> String {
        self.setting(tab, "engine")
    }

    /// The language dictation hears, by its name.
    pub fn language_name(&self) -> String {
        let code = self.setting(Tab::Dictation, "language");
        self.languages
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| "Detect".into())
    }

    /// The speed speech goes at.
    pub fn speed(&self) -> f64 {
        self.settings["speech"]["speed"].as_f64().unwrap_or(1.0)
    }

    /// The voice speech speaks in, by its name, or the first of the engine's.
    pub fn voice_name(&self) -> String {
        let named = self.setting(Tab::Speech, "voice_name");
        if !named.is_empty() {
            return named;
        }
        let engine = self.engine(Tab::Speech);
        match self.voices.get(&engine) {
            Some(Ok(list)) => list.first().map(|(_, n)| n.clone()).unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// Use, on a tile.
    pub fn use_engine(&mut self, tab: Tab, id: &str) -> Value {
        self.cmd("voice.put", json!({ tab.kind(): {"engine": id} }))
    }

    /// One of a kind's rows changed.
    pub fn set(&mut self, tab: Tab, change: Value) -> Value {
        self.cmd("voice.put", json!({ tab.kind(): change }))
    }

    /// Save, on a tile's key field: the key, and the engine put in use if it wasn't.
    pub fn save_key(&mut self, tab: Tab, id: &str, key: &str) -> Vec<Value> {
        self.adding = None;
        let mut out = vec![self.cmd("voice.key", json!({"service": id, "key": key.trim()}))];
        if self.engine(tab) != id {
            out.push(self.use_engine(tab, id));
        }
        out
    }

    /// The voices of the speech engine in use, to ask the core for once.
    pub fn want_voices(&self) -> Option<Value> {
        let engine = self.engine(Tab::Speech);
        if engine.is_empty() || self.voices.contains_key(&engine) {
            return None;
        }
        Some(json!({"t": "voice.voices", "engine": engine}))
    }

    /// A voice's preview, by its play button.
    pub fn preview(&self, voice: &str, name: &str) -> Value {
        json!({"t": "voice.speak", "id": "app.voice_preview", "text": format!("Hi, I'm {name}."),
            "engine": self.engine(Tab::Speech), "voice": voice})
    }

    /// The mic pressed: dictation starts at the composer's caret, `at`.
    pub fn listen(&mut self, at: usize) -> Value {
        self.dictating = Some(Dictating {
            state: "listening".into(),
            at,
            len: 0,
            started: true,
        });
        self.words = None;
        json!({"t": "voice.listen"})
    }

    /// Where dictation's words go in the composer's `text`, in place of the ones it put
    /// there before, and what goes there. None when nothing changed. Ends the dictation
    /// once it's done or failed.
    pub fn composer_words(&mut self, text: &str) -> Option<(Range<usize>, String)> {
        let words = self.words.take()?;
        let d = self.dictating.as_mut()?;
        let (range, put) = splice(text, d.at, d.len, &words);
        d.at = range.start;
        d.len = put.len();
        if d.state == "done" || d.state == "failed" {
            self.dictating = None;
        }
        Some((range, put))
    }
}

/// Where `words` go in `text`, in place of the `len` bytes from `at`, and what goes
/// there: the words, with a space before them where the text before has none and after
/// them where the text after starts with a letter.
pub fn splice(text: &str, at: usize, len: usize, words: &str) -> (Range<usize>, String) {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    let mut end = (at + len).min(text.len());
    while !text.is_char_boundary(end) {
        end += 1;
    }
    let before = &text[..at];
    let after = &text[end..];
    let words = words.trim();
    if words.is_empty() {
        return (at..end, String::new());
    }
    let space_before = !before.is_empty() && !before.ends_with(char::is_whitespace);
    let space_after = after.starts_with(|c: char| c.is_alphanumeric());
    let put = format!(
        "{}{words}{}",
        if space_before { " " } else { "" },
        if space_after { " " } else { "" }
    );
    (at..end, put)
}

/// The voice_setup card an agent put in its reply (PLAN.md, "Agents set it up"): its
/// engine's tile, the rows as set, and whether it waits for the person's key.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    pub id: String,
    pub message: String,
    pub title: String,
    pub kind: Tab,
    pub tile: Tile,
    /// The rows, each its name and value, as the page shows them.
    pub rows: Vec<(String, String)>,
    pub asking: bool,
}

/// The rows a card's settings show, as the page names them.
pub fn card_rows(kind: Tab, settings: &Value) -> Vec<(String, String)> {
    match kind {
        Tab::Dictation => {
            let code = s(settings, "language");
            let name = LANGUAGES
                .iter()
                .find(|(c, _)| *c == code)
                .map(|(_, n)| n.to_string())
                .unwrap_or_else(|| "Detect".into());
            vec![("Language".into(), name)]
        }
        Tab::Speech => {
            let voice = s(settings, "voice_name");
            let mut rows = Vec::new();
            if !voice.is_empty() {
                rows.push(("Voice".into(), voice));
            }
            let speed = settings["speed"].as_f64().unwrap_or(1.0);
            rows.push(("Speed".into(), speed_label(speed)));
            let read = match s(settings, "read_aloud").as_str() {
                "always" => "Always",
                "live" => "Only in live",
                _ => "Never",
            };
            rows.push(("Read replies aloud".into(), read.into()));
            rows
        }
    }
}

/// A speed as the Speed row writes it: 1×, 0.9×.
pub fn speed_label(speed: f64) -> String {
    let r = (speed * 10.0).round() / 10.0;
    if (r - r.round()).abs() < 1e-9 {
        format!("{}×", r.round() as i64)
    } else {
        format!("{r:.1}×")
    }
}

/// The speeds, as the Speed row offers them.
pub const SPEEDS: [f64; 5] = [0.8, 0.9, 1.0, 1.1, 1.2];

/// The languages dictation hears, as shared/voice.calx has them, for a card whose page
/// hasn't loaded them.
const LANGUAGES: [(&str, &str); 20] = [
    ("", "Detect"),
    ("en", "English"),
    ("no", "Norwegian"),
    ("sv", "Swedish"),
    ("da", "Danish"),
    ("fi", "Finnish"),
    ("de", "German"),
    ("nl", "Dutch"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("it", "Italian"),
    ("pt", "Portuguese"),
    ("pl", "Polish"),
    ("uk", "Ukrainian"),
    ("tr", "Turkish"),
    ("hi", "Hindi"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("zh", "Chinese"),
    ("ar", "Arabic"),
];

/// The card's body, from the instance's `voice_setup` card in message `message`.
pub fn card_of(c: &Value, message: &str) -> Card {
    let kind = if s(c, "kind") == "speech" {
        Tab::Speech
    } else {
        Tab::Dictation
    };
    Card {
        id: s(c, "id"),
        message: message.to_string(),
        title: s(c, "title"),
        kind,
        tile: Tile::of(&c["engine"]),
        rows: card_rows(kind, &c["settings"]),
        asking: s(c, "state") == "asking",
    }
}

/// Add, on the card: `voice.card` with the key typed there.
pub fn card_answer(card: &Card, key: &str) -> Value {
    json!({"t": "cmd", "id": format!("{ASK}card_{}", card.id), "name": "voice.card",
        "args": {"message": card.message, "card": card.id, "key": key.trim()}})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed() -> Value {
        json!({
            "settings": {"dictation": {"engine": "deepgram", "language": "no"},
                "speech": {"engine": "openai", "voice": "cedar", "voice_name": "Cedar", "speed": 1.1, "read_aloud": "never"}},
            "engines": {
                "dictation": [
                    {"id": "elevenlabs", "name": "ElevenLabs Scribe", "line": "Words as you say them", "icon": "cloud", "streams": true, "key": true, "have": false, "in_use": false},
                    {"id": "deepgram", "name": "Deepgram Nova-3", "line": "Words as you say them", "icon": "cloud", "streams": true, "key": true, "have": true, "in_use": true},
                    {"id": "groq", "name": "Groq", "line": "Whisper large v3, writes once you stop", "icon": "cloud", "streams": false, "key": true, "have": true, "in_use": false}
                ],
                "speech": [{"id": "openai", "name": "OpenAI", "line": "gpt-4o-mini-tts, 13 voices", "icon": "cloud", "streams": false, "key": true, "have": true, "in_use": true}]
            },
            "languages": [{"code": "", "name": "Detect"}, {"code": "no", "name": "Norwegian"}]
        })
    }

    #[test]
    fn the_settings_come_whole() {
        let mut v = Voice::default();
        assert!(v.take(
            &json!({"t": "result", "id": "app.voice_1", "ok": true, "value": {"voice": listed()}})
        ));
        assert!(v.came && v.dictation.len() == 3 && v.speech.len() == 1);
        assert_eq!(v.engine(Tab::Dictation), "deepgram");
        assert_eq!(v.language_name(), "Norwegian");
        assert_eq!(v.voice_name(), "Cedar");
        assert_eq!(speed_label(v.speed()), "1.1×");
        assert_eq!(speed_label(1.0), "1×");
        // Each device hears a change.
        let mut changed = listed();
        changed["settings"]["dictation"]["engine"] = json!("groq");
        assert!(v.take(&json!({"t": "event", "kind": "voice.updated", "voice": changed})));
        assert_eq!(v.engine(Tab::Dictation), "groq");
    }

    #[test]
    fn each_tile_has_its_button() {
        let mut v = Voice::default();
        v.took(&listed());
        let actions: Vec<_> = v.dictation.iter().map(Tile::action).collect();
        assert_eq!(
            actions,
            vec![
                Some(Action::AddKey),
                Some(Action::ChangeKey),
                Some(Action::Use)
            ]
        );
        let frames = v.save_key(Tab::Dictation, "elevenlabs", " k ");
        assert_eq!(frames[0]["name"], "voice.key");
        assert_eq!(
            frames[0]["args"],
            json!({"service": "elevenlabs", "key": "k"})
        );
        assert_eq!(
            frames[1]["args"],
            json!({"dictation": {"engine": "elevenlabs"}})
        );
        // A key changed for the engine in use puts nothing else in use.
        assert_eq!(v.save_key(Tab::Dictation, "deepgram", "k").len(), 1);
    }

    /// The text once `splice`'s words are in.
    fn spliced(text: &str, at: usize, len: usize, words: &str) -> (String, usize) {
        let (r, put) = splice(text, at, len, words);
        (
            format!("{}{put}{}", &text[..r.start], &text[r.end..]),
            put.len(),
        )
    }

    #[test]
    fn dictation_writes_in_at_the_caret() {
        assert_eq!(spliced("Hello", 5, 0, "there"), ("Hello there".into(), 6));
        assert_eq!(
            spliced("Hello there", 5, 6, "world"),
            ("Hello world".into(), 6)
        );
        assert_eq!(spliced("", 0, 0, " hi "), ("hi".into(), 2));
        assert_eq!(spliced("ab", 1, 0, "x"), ("a x b".into(), 3));
        assert_eq!(spliced("a b", 1, 4, ""), ("a".into(), 0));
        let mut v = Voice::default();
        let f = v.listen(5);
        assert_eq!(f, json!({"t": "voice.listen"}));
        v.take(&json!({"t": "core.dictation", "state": "listening", "text": "how are"}));
        assert_eq!(v.composer_words("Hello"), Some((5..5, " how are".into())));
        assert!(v.composer_words("Hello how are").is_none());
        v.take(&json!({"t": "core.dictation", "state": "done", "text": "how are you"}));
        assert_eq!(
            v.composer_words("Hello how are"),
            Some((5..13, " how are you".into()))
        );
        assert!(v.dictating.is_none());
    }

    #[test]
    fn no_engine_opens_the_page() {
        let mut v = Voice::default();
        v.listen(0);
        v.take(&json!({"t": "core.dictation", "state": "no_engine"}));
        assert!(v.open_page && v.dictating.is_none());
    }

    #[test]
    fn the_card_shows_the_rows_as_set() {
        let c = json!({"id": "voice_tc_4", "type": "voice_setup", "title": "Set up speech", "kind": "speech",
            "engine": {"id": "elevenlabs", "name": "ElevenLabs", "in_use": false, "have": false, "key": true},
            "settings": {"engine": "elevenlabs", "voice": "Rachel", "voice_name": "Rachel", "speed": 0.9, "read_aloud": "always"},
            "service": "elevenlabs", "state": "asking"});
        let card = card_of(&c, "m_1");
        assert!(card.asking && card.kind == Tab::Speech && card.tile.name == "ElevenLabs");
        assert_eq!(
            card.rows,
            vec![
                ("Voice".into(), "Rachel".into()),
                ("Speed".into(), "0.9×".into()),
                ("Read replies aloud".into(), "Always".into())
            ]
        );
        let f = card_answer(&card, " sk ");
        assert_eq!(f["name"], "voice.card");
        assert_eq!(
            f["args"],
            json!({"message": "m_1", "card": "voice_tc_4", "key": "sk"})
        );
        let d = card_of(
            &json!({"kind": "dictation", "settings": {"language": "de"}, "state": "set"}),
            "m",
        );
        assert_eq!(d.rows, vec![("Language".into(), "German".into())]);
        assert!(!d.asking);
    }
}
