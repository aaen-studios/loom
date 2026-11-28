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
    Live,
}

impl Tab {
    pub fn kind(self) -> &'static str {
        match self {
            Tab::Dictation => "dictation",
            Tab::Speech => "speech",
            Tab::Live => "live",
        }
    }
}

/// An engine, as its tile shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tile {
    pub id: String,
    pub name: String,
    pub line: String,
    /// `cloud` for a service, `laptop` for a model on the device, drawn where it has no
    /// mark.
    pub icon: String,
    /// Its maker's logo, one colour in `currentColor` (shared/voice/), and the name of it.
    pub icon_svg: Option<String>,
    pub mark: String,
    pub streams: bool,
    /// Whether it needs a key, and whether the person has it, or for a model on the
    /// device, whether it's downloaded.
    pub key: bool,
    pub have: bool,
    pub in_use: bool,
    /// A model on the device: where its download is on this computer, as the core says.
    pub model: Option<Model>,
    /// A service's models, newest first, and the one it uses, by its id; none for a
    /// model on the device, which is its own.
    pub models: Vec<Choice>,
    pub picked: String,
}

/// One of a service's models, as the tile's menu shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Choice {
    pub id: String,
    pub name: String,
    pub line: String,
}

/// Where a model on the device is, as the core tells it with the tile.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    /// `absent`, `downloading`, `ready`, `failed` or `unavailable`.
    pub state: String,
    pub percent: u8,
    /// Why it failed, or why it can't run here.
    pub why: String,
}

impl Tile {
    pub fn of(v: &Value) -> Tile {
        let b = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
        Tile {
            id: s(v, "id"),
            name: s(v, "name"),
            line: s(v, "line"),
            icon: s(v, "icon"),
            icon_svg: v
                .get("icon_svg")
                .and_then(Value::as_str)
                .filter(|m| !m.is_empty())
                .map(str::to_string),
            mark: s(v, "mark"),
            streams: b("streams"),
            key: b("key"),
            have: b("have"),
            in_use: b("in_use"),
            model: v.get("model").filter(|m| m.is_object()).map(|m| Model {
                state: s(m, "state"),
                percent: m
                    .get("percent")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    .min(100) as u8,
                why: s(m, "why"),
            }),
            models: v
                .get("models")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|m| Choice {
                            id: s(m, "id"),
                            name: s(m, "name"),
                            line: s(m, "line"),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            picked: s(v, "picked"),
        }
    }

    /// Whether the tile's line is its model, to pick from: the service in use, with more
    /// than one.
    pub fn picks_model(&self) -> bool {
        self.in_use && self.models.len() > 1
    }

    /// Where its download is, for a model on the device: `absent`, `downloading`,
    /// `ready`, `failed` or `unavailable`; "" for a service.
    pub fn model_state(&self) -> &str {
        self.model.as_ref().map(|m| m.state.as_str()).unwrap_or("")
    }

    /// Its download's percent, while it downloads.
    pub fn percent(&self) -> Option<u8> {
        (self.model_state() == "downloading")
            .then(|| self.model.as_ref().map(|m| m.percent).unwrap_or(0))
    }

    /// The tile's buttons, the main one last: Use, Add key or Change key for a service;
    /// for a model, Download, Cancel while it downloads, and Delete and Use once it's
    /// here. None for the one in use with no key to change, or a model that can't run.
    pub fn actions(&self) -> Vec<Action> {
        match self.model_state() {
            "" => self.action().into_iter().collect(),
            "absent" | "failed" => vec![Action::Download],
            "downloading" => vec![Action::Cancel],
            "ready" if self.in_use => vec![Action::Delete],
            "ready" => vec![Action::Delete, Action::Use],
            _ => Vec::new(),
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
    Download,
    Cancel,
    Delete,
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::Use => "Use",
            Action::AddKey => "Add key",
            Action::ChangeKey => "Change key",
            Action::Download => "Download",
            Action::Cancel => "Cancel",
            Action::Delete => "Delete",
        }
    }

    /// Whether it's the tile's main button, drawn in ink.
    pub fn main(self) -> bool {
        matches!(self, Action::Use | Action::Download)
    }

    /// What the core is asked to do with a model on the device, for the three that do.
    pub fn model_word(self) -> Option<&'static str> {
        match self {
            Action::Download => Some("download"),
            Action::Cancel => Some("cancel"),
            Action::Delete => Some("delete"),
            _ => None,
        }
    }
}

/// A menu open on the page: the language's, the voice's, the models of the tile in use,
/// or the microphones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    Language,
    Voice,
    Model,
    Microphone,
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
    /// A dictation the message was sent in the middle of (LOO-448): the core's frames
    /// about it are no one's until it says `cancelled`.
    pub dropping: bool,
    /// What `core.dictation` said last, for the composer: its words, once they changed.
    pub words: Option<String>,
    /// How its words start: those the engine won't change again (`kept`).
    pub kept: String,
    /// Where the words went in the composer last, for drawing them (LOO-445).
    pub dictated: Option<Dictated>,
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
        // A key added or an engine changed: voices that didn't come are asked for again,
        // and so are those of an engine whose model changed, as Deepgram's voices are
        // each model's.
        let was = self.settings["speech"]["models"].clone();
        let now = v
            .pointer("/settings/speech/models")
            .cloned()
            .unwrap_or(Value::Null);
        self.voices.retain(|engine, got| {
            got.as_ref().is_ok_and(|l| !l.is_empty()) && was.get(engine) == now.get(engine)
        });
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
        // Every device hears a change, and this one hears its downloads go as the core
        // tells the tiles again.
        if (t == "event" && s(f, "kind") == "voice.updated") || t == "core.voice" {
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
            if state == "cancelled" {
                self.dropping = false;
                return true;
            }
            if self.dropping {
                return true;
            }
            match state.as_str() {
                "no_engine" => {
                    self.dictating = None;
                    self.open_page = true;
                    self.tab = Tab::Dictation;
                }
                "failed" => {
                    self.said = Some(s(f, "why"));
                    self.words = Some(s(f, "text"));
                    self.kept = s(f, "kept");
                    if let Some(d) = &mut self.dictating {
                        d.state = state;
                    }
                }
                _ => {
                    self.words = Some(s(f, "text"));
                    self.kept = s(f, "kept");
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
            Tab::Live => &[],
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

    /// One of live mode's switches, Talk over a reply to stop it (`barge_in`) or Show
    /// what's said (`captions`), on unless the settings say otherwise.
    pub fn live_switch(&self, key: &str) -> bool {
        self.settings["live"][key].as_bool().unwrap_or(true)
    }

    /// How long live mode waits after the person stops talking, in seconds.
    pub fn pause(&self) -> f64 {
        self.settings["live"]["pause"].as_f64().unwrap_or(0.8)
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

    /// A model picked in the menu of the tile in use.
    pub fn use_model(&mut self, tab: Tab, model: &str) -> Value {
        self.menu = None;
        self.cmd("voice.put", json!({ tab.kind(): {"model": model} }))
    }

    /// Download, Cancel or Delete, on a model's tile or its card.
    pub fn model(&self, tab: Tab, id: &str, act: Action) -> Option<Value> {
        let word = act.model_word()?;
        Some(json!({"t": "voice.model", "engine": id, "kind": tab.kind(), "do": word}))
    }

    /// The page's tile for an engine, as this computer has it, which a card in the chat
    /// shows its download from.
    pub fn tile(&self, tab: Tab, id: &str) -> Option<&Tile> {
        self.tiles(tab).iter().find(|t| t.id == id)
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
        self.kept.clear();
        json!({"t": "voice.listen"})
    }

    /// The message sent while dictating (LOO-448): the dictation is dropped, so the
    /// words still to come don't write it back into the emptied composer, and the core
    /// is told to drop it too. None when there's no dictation.
    pub fn sent(&mut self) -> Option<Value> {
        self.dictating.take()?;
        self.words = None;
        self.kept.clear();
        self.dictated = None;
        self.dropping = true;
        Some(json!({"t": "voice.cancel"}))
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
        let over = d.state == "done" || d.state == "failed";
        let words = words.trim();
        let kept = self.kept.trim();
        self.dictated = Some(Dictated {
            at: range.start + put.len() - put.trim_start().len(),
            words: words.to_string(),
            // Once it's over, every word is kept.
            kept: if over {
                words.len()
            } else if words.starts_with(kept) {
                kept.len()
            } else {
                0
            },
        });
        if over {
            self.dictating = None;
        }
        Some((range, put))
    }
}

/// A dictation's words where they went in the composer: from byte `at`, the first
/// `kept` bytes of them kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dictated {
    pub at: usize,
    pub words: String,
    pub kept: usize,
}

/// A dictated word in the composer, as it's drawn (LOO-445): it comes into focus from
/// `came`, faint until it's kept and from `kept` turning to full ink.
#[derive(Clone, Debug, PartialEq)]
pub struct Heard {
    pub range: Range<usize>,
    pub word: String,
    pub came: std::time::Instant,
    pub kept: Option<std::time::Instant>,
}

/// How long a word takes to come into focus, and to turn from faint to full ink.
pub const FOCUS_SECS: f32 = 0.38;
pub const KEPT_SECS: f32 = 0.35;

/// The words of a dictation as `d` has them now, from those drawn `before`: a word that
/// is where it was, the same, keeps its time, and one that's new or changed comes in
/// again from `now`. Each word is kept from when `d` first says so.
pub fn heard(before: &[Heard], d: &Dictated, now: std::time::Instant) -> Vec<Heard> {
    let mut out = Vec::new();
    let mut from = 0;
    for (i, word) in d.words.split_whitespace().enumerate() {
        let start = from + d.words[from..].find(word).unwrap_or(0);
        let end = start + word.len();
        from = end;
        let same = before.get(i).filter(|h| h.word == word);
        let kept = end <= d.kept;
        out.push(Heard {
            range: d.at + start..d.at + end,
            word: word.to_string(),
            came: same.map_or(now, |h| h.came),
            kept: same
                .and_then(|h| h.kept)
                .or_else(|| kept.then_some(now))
                .filter(|_| kept),
        });
    }
    out
}

/// How far a word is into focus and into full ink at `now`, each from 0 to 1, eased.
pub fn focus_at(h: &Heard, now: std::time::Instant) -> (f32, f32) {
    let ease = |x: f32| 1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3);
    let since = |t: std::time::Instant| now.saturating_duration_since(t).as_secs_f32();
    (
        ease(since(h.came) / FOCUS_SECS),
        h.kept.map_or(0.0, |t| ease(since(t) / KEPT_SECS)),
    )
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
        Tab::Live => Vec::new(),
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

/// How long live mode waits before it sends, as Send when I stop talking offers it.
pub const PAUSES: [f64; 3] = [0.5, 0.8, 1.2];

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
                    {"id": "elevenlabs", "name": "ElevenLabs", "line": "Scribe v3 Live", "icon": "cloud", "streams": true, "key": true, "have": false, "in_use": false,
                        "models": [{"id": "scribe_v3_live", "name": "Scribe v3 Live", "line": "Words as you say them"}, {"id": "scribe_v2_realtime", "name": "Scribe v2 Realtime", "line": "The model before"}], "picked": "scribe_v3_live"},
                    {"id": "deepgram", "name": "Deepgram", "line": "Nova-3", "icon": "cloud", "icon_svg": "<svg id=\"deepgram\"/>", "mark": "deepgram", "streams": true, "key": true, "have": true, "in_use": true,
                        "models": [{"id": "nova-3", "name": "Nova-3", "line": "Words as you say them"}, {"id": "flux-general-en", "name": "Flux English", "line": "English, hears when you're done"}], "picked": "nova-3"},
                    {"id": "groq", "name": "Groq", "line": "Whisper large v3 turbo", "icon": "cloud", "streams": false, "key": true, "have": true, "in_use": false}
                ],
                "speech": [{"id": "openai", "name": "OpenAI", "line": "gpt-realtime-2.1-mini", "icon": "cloud", "streams": false, "key": true, "have": true, "in_use": true,
                    "models": [{"id": "gpt-realtime-2.1-mini", "name": "gpt-realtime-2.1-mini", "line": "10 voices"}], "picked": "gpt-realtime-2.1-mini"}]
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
        // A tile with its maker's mark has it; one without draws its icon.
        assert_eq!(
            v.dictation[1].icon_svg.as_deref(),
            Some("<svg id=\"deepgram\"/>")
        );
        assert_eq!(v.dictation[1].mark, "deepgram");
        assert_eq!(v.dictation[0].icon_svg, None);
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
    fn the_tile_in_use_picks_its_model() {
        let mut v = Voice::default();
        v.took(&listed());
        // Only the tile in use with a choice of models has the chip.
        let picks: Vec<_> = v.dictation.iter().map(Tile::picks_model).collect();
        assert_eq!(picks, vec![false, true, false]);
        assert!(!v.speech[0].picks_model());
        assert_eq!(v.dictation[1].picked, "nova-3");
        assert_eq!(v.dictation[1].models[1].name, "Flux English");
        v.menu = Some(Menu::Model);
        let f = v.use_model(Tab::Dictation, "flux-general-en");
        assert_eq!(f["name"], "voice.put");
        assert_eq!(
            f["args"],
            json!({"dictation": {"model": "flux-general-en"}})
        );
        assert_eq!(v.menu, None);
        // An engine's voices go once its model changes, as Deepgram's are each model's.
        let mut l = listed();
        l["settings"]["speech"]["models"] =
            json!({"deepgram": "flux", "openai": "gpt-realtime-2.1-mini"});
        v.took(&l);
        v.voices.insert(
            "deepgram".into(),
            Ok(vec![("flux-kit-en".into(), "Kit".into())]),
        );
        v.voices
            .insert("openai".into(), Ok(vec![("marin".into(), "Marin".into())]));
        l["settings"]["speech"]["models"]["deepgram"] = json!("aura-2");
        v.took(&l);
        assert!(!v.voices.contains_key("deepgram") && v.voices.contains_key("openai"));
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

    #[test]
    fn a_model_on_the_device_downloads_then_is_used() {
        let tile = |model: Value, in_use: bool| {
            Tile::of(
                &json!({"id": "parakeet", "icon": "laptop", "key": false, "have": false, "in_use": in_use, "model": model}),
            )
        };
        assert_eq!(
            tile(json!({"state": "absent", "size": 35}), false).actions(),
            vec![Action::Download]
        );
        let going = tile(
            json!({"state": "downloading", "percent": 24, "size": 35}),
            false,
        );
        assert_eq!(going.actions(), vec![Action::Cancel]);
        assert_eq!(going.percent(), Some(24));
        assert_eq!(
            tile(json!({"state": "ready"}), false).actions(),
            vec![Action::Delete, Action::Use]
        );
        assert_eq!(
            tile(json!({"state": "ready"}), true).actions(),
            vec![Action::Delete]
        );
        assert_eq!(
            tile(json!({"state": "failed", "why": "no"}), false).actions(),
            vec![Action::Download]
        );
        let phone = tile(
            json!({"state": "unavailable", "why": "Runs on Windows and macOS."}),
            false,
        );
        assert!(phone.actions().is_empty() && phone.percent().is_none());
        // A service's tile has its one button, as before.
        assert_eq!(
            Tile::of(&json!({"id": "groq", "key": true, "have": true})).actions(),
            vec![Action::Use]
        );
        assert!(
            Action::Download.main()
                && !Action::Cancel.main()
                && Action::Delete.model_word() == Some("delete")
        );
        // Its buttons go to the core, and the core's progress is taken as the page.
        let mut v = Voice::default();
        let mut l = listed();
        l["engines"]["dictation"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": "parakeet", "icon": "laptop", "key": false, "have": false, "in_use": false, "model": {"state": "absent"}}));
        v.took(&l);
        assert_eq!(
            v.model(Tab::Dictation, "parakeet", Action::Download),
            Some(
                json!({"t": "voice.model", "engine": "parakeet", "kind": "dictation", "do": "download"})
            )
        );
        assert_eq!(v.model(Tab::Dictation, "parakeet", Action::Use), None);
        l["engines"]["dictation"][3]["model"] = json!({"state": "downloading", "percent": 50});
        assert!(v.take(&json!({"t": "core.voice", "voice": l})));
        assert_eq!(
            v.tile(Tab::Dictation, "parakeet").and_then(Tile::percent),
            Some(50)
        );
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
    fn a_message_sent_while_dictating_stays_out_of_the_composer() {
        let mut v = Voice::default();
        // Nothing to drop with no dictation.
        assert_eq!(v.sent(), None);
        v.listen(0);
        v.take(&json!({"t": "core.dictation", "state": "listening", "text": "check the"}));
        assert_eq!(v.composer_words(""), Some((0..0, "check the".into())));
        // Sent while words are still coming: the composer is emptied, and the core told
        // to drop the dictation.
        assert_eq!(v.sent(), Some(json!({"t": "voice.cancel"})));
        assert!(v.dictating.is_none() && v.dictated.is_none());
        // What the core had on the way comes anyway, and goes nowhere.
        for (state, text) in [
            ("listening", "check the build"),
            ("finishing", "check the build"),
            ("done", "check the build"),
        ] {
            v.take(&json!({"t": "core.dictation", "state": state, "text": text}));
            assert_eq!(v.composer_words(""), None);
        }
        // Dictating again at once: the last one's words still on the way don't go into
        // the new one, which has its own once the core says the last is gone.
        v.listen(0);
        v.take(&json!({"t": "core.dictation", "state": "done", "text": "check the build"}));
        assert_eq!(v.composer_words(""), None);
        assert!(v.dictating.is_some());
        v.take(&json!({"t": "core.dictation", "state": "cancelled"}));
        v.take(&json!({"t": "core.dictation", "state": "listening", "text": "ship it"}));
        assert_eq!(v.composer_words(""), Some((0..0, "ship it".into())));
    }

    #[test]
    fn dictated_words_come_into_focus_and_turn_to_ink_once_kept() {
        use std::time::{Duration, Instant};
        let mut v = Voice::default();
        v.listen(5);
        v.take(&json!({"t": "core.dictation", "state": "listening", "text": "water the"}));
        v.composer_words("Hello");
        let d = v.dictated.take().unwrap();
        assert_eq!(
            d,
            Dictated {
                at: 6,
                words: "water the".into(),
                kept: 0
            }
        );
        let t0 = Instant::now();
        let a = heard(&[], &d, t0);
        assert_eq!(a.len(), 2);
        assert_eq!((a[0].range.clone(), a[1].range.clone()), (6..11, 12..15));
        assert!(a.iter().all(|h| h.came == t0 && h.kept.is_none()));
        // "the" is heard again as "tomatoes", and "water" is kept: only the changed word
        // comes in again.
        v.take(&json!({"t": "core.dictation", "state": "listening", "text": "Water tomatoes", "kept": "Water"}));
        v.composer_words("Hello water the");
        let d = v.dictated.take().unwrap();
        assert_eq!(d.kept, 5);
        let t1 = t0 + Duration::from_millis(200);
        let b = heard(&a, &d, t1);
        // "water" became "Water": changed, so it comes in again, and it's kept.
        assert_eq!((b[0].came, b[0].kept), (t1, Some(t1)));
        assert_eq!((b[1].came, b[1].kept), (t1, None));
        // The same words again: nothing comes in anew; once it's over, all are kept.
        v.take(&json!({"t": "core.dictation", "state": "done", "text": "Water tomatoes", "kept": "Water"}));
        v.composer_words("Hello Water tomatoes");
        let d = v.dictated.take().unwrap();
        let t2 = t1 + Duration::from_millis(100);
        let c = heard(&b, &d, t2);
        assert_eq!((c[0].came, c[0].kept), (t1, Some(t1)));
        assert_eq!((c[1].came, c[1].kept), (t1, Some(t2)));
        assert_eq!(focus_at(&c[1], t2), (focus_at(&c[1], t2).0, 0.0));
        assert_eq!(focus_at(&c[1], t2 + Duration::from_secs(1)), (1.0, 1.0));
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
