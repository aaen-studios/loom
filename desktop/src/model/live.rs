//! Live mode in the desktop's model (PLAN.md, "Live mode"): what `core.live` says, kept
//! for the view that takes the chat's place while it runs, and the frames its buttons
//! send. The core does the listening, the turn-taking and the speaking; this keeps what
//! plays and works out from it, by the clock, which of its words are said and how far
//! the thread swings.

use std::time::Instant;

use serde_json::{json, Value};

use super::s;

/// How the thread swings: its target's rise and fall, in seconds.
const RISE: f32 = 0.08;
const FALL: f32 = 0.25;
/// How still it lies while the agent thinks, and how much of the mic's level it takes.
const THINKING: f32 = 0.03;
const MIC: f32 = 0.5;

/// What live mode is doing, as the header says it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Listening,
    Thinking,
    Speaking,
}

impl State {
    pub fn word(self) -> &'static str {
        match self {
            State::Listening => "Listening",
            State::Thinking => "Thinking",
            State::Speaking => "Speaking",
        }
    }
}

/// What the composer's main button is (LOO-443): Stop while the agent answers and
/// there's nothing to send, voice mode while the composer is empty in an agent's chat,
/// and Send otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Main {
    Stop,
    Send,
    Live,
}

impl Main {
    /// `empty` is no words but spaces and nothing attached; `live` is whether live mode
    /// can start here, as it can in an agent's chat and not in a group chat.
    pub fn of(busy: bool, empty: bool, live: bool) -> Main {
        if busy && empty {
            Main::Stop
        } else if empty && live {
            Main::Live
        } else {
            Main::Send
        }
    }

    /// Its name, for its tooltip and for a screen reader.
    pub fn name(self) -> &'static str {
        match self {
            Main::Stop => "Stop",
            Main::Send => "Send",
            Main::Live => "Start voice mode",
        }
    }
}

/// A piece of the reply playing: its words, when each starts, and its level each
/// `step` ms, from when it started.
#[derive(Clone, Debug)]
pub struct Piece {
    pub file: String,
    pub words: Vec<String>,
    pub at: Vec<u64>,
    pub levels: Vec<f32>,
    pub step: u64,
    pub ms: u64,
    pub since: Instant,
}

impl Piece {
    fn of(v: &Value, now: Instant) -> Piece {
        let nums = |k: &str| -> Vec<f64> {
            v.get(k)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_f64).collect())
                .unwrap_or_default()
        };
        Piece {
            file: s(v, "file"),
            words: v
                .get("words")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|w| w.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            at: nums("at").into_iter().map(|x| x.max(0.0) as u64).collect(),
            levels: nums("levels").into_iter().map(|x| x as f32).collect(),
            step: v.get("step").and_then(Value::as_u64).unwrap_or(20).max(1),
            ms: v.get("ms").and_then(Value::as_u64).unwrap_or(0),
            since: now,
        }
    }

    fn elapsed_ms(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.since).as_millis() as u64
    }

    /// Whether it's still playing.
    pub fn playing(&self, now: Instant) -> bool {
        self.elapsed_ms(now) < self.ms
    }

    /// Its level now.
    pub fn level(&self, now: Instant) -> f32 {
        let i = (self.elapsed_ms(now) / self.step) as usize;
        self.levels.get(i).copied().unwrap_or(0.0)
    }

    /// How many of its words are said by now.
    pub fn said(&self, now: Instant) -> usize {
        let t = self.elapsed_ms(now);
        if !self.playing(now) {
            return self.words.len();
        }
        self.at.iter().filter(|&&a| a <= t).count()
    }
}

#[derive(Default)]
pub struct Live {
    pub on: bool,
    pub state: State,
    pub agent: String,
    pub name: String,
    pub conversation: String,
    pub muted: bool,
    /// The words show (Text), starting as Show what's said has it.
    pub show_words: bool,
    /// What shows while nothing plays: the person's words, or Never's reply.
    pub words: String,
    /// The mic's level, as the core last said it.
    pub level: f32,
    pub piece: Option<Piece>,
    /// When it started, on this clock, for the time it has run.
    pub since: Option<Instant>,
    /// How far the thread swings now, and when that was worked out.
    swing: f32,
    swung: Option<Instant>,
    /// Speech to play now, or to stop, for the app.
    pub play: Option<String>,
    pub stop: bool,
    /// Settings › Voice is to open on this tab (dictation or speech), with why.
    pub open_page: Option<(String, String)>,
    /// Why something failed, for a notice.
    pub said: Option<String>,
}

impl Live {
    /// A `core.live` frame. Gives whether it was one.
    pub fn take(&mut self, f: &Value) -> bool {
        if s(f, "t") != "core.live" {
            return false;
        }
        self.take_at(f, Instant::now());
        true
    }

    pub fn take_at(&mut self, f: &Value, now: Instant) {
        let state = s(f, "state");
        if f.get("stop").and_then(Value::as_bool) == Some(true) {
            self.piece = None;
            self.stop = true;
        }
        if let Some(why) = f.get("why").and_then(Value::as_str) {
            if state != "no_engine" {
                self.said = Some(why.to_string());
            }
        }
        match state.as_str() {
            "ended" => {
                self.on = false;
                self.piece = None;
                return;
            }
            "no_engine" => {
                self.on = false;
                self.piece = None;
                self.open_page = Some((s(f, "kind"), s(f, "why")));
                return;
            }
            _ => {}
        }
        if !self.on {
            self.on = true;
            self.since = Some(now);
            self.show_words = f.get("captions").and_then(Value::as_bool).unwrap_or(true);
            self.swing = 0.0;
            self.swung = None;
        }
        self.state = match state.as_str() {
            "speaking" => State::Speaking,
            "thinking" => State::Thinking,
            _ => State::Listening,
        };
        self.agent = s(f, "agent");
        self.name = s(f, "name");
        self.conversation = s(f, "conversation");
        self.muted = f.get("muted").and_then(Value::as_bool).unwrap_or(false);
        self.words = s(f, "words");
        if let Some(l) = f.get("level").and_then(Value::as_f64) {
            self.level = l as f32;
        }
        if self.muted || self.state != State::Listening {
            self.level = 0.0;
        }
        if let Some(p) = f.get("play") {
            let piece = Piece::of(p, now);
            self.play = Some(piece.file.clone());
            self.piece = Some(piece);
        }
    }

    /// The piece playing now, if one is.
    pub fn playing(&self, now: Instant) -> Option<&Piece> {
        self.piece.as_ref().filter(|p| p.playing(now))
    }

    /// The words under the thread, and how many of them are said: a piece's while it
    /// plays, or else what the core said, all of it said.
    pub fn caption(&self, now: Instant) -> (Vec<String>, usize) {
        if let Some(p) = self.playing(now) {
            return (p.words.clone(), p.said(now));
        }
        if self.state == State::Speaking {
            if let Some(p) = &self.piece {
                return (p.words.clone(), p.words.len());
            }
        }
        let words: Vec<String> = self.words.split_whitespace().map(str::to_string).collect();
        let n = words.len();
        (words, n)
    }

    /// What the thread swings towards now: the level of what plays while the agent
    /// speaks, the mic's lightly while it listens, and nearly nothing while it thinks.
    pub fn target(&self, now: Instant) -> f32 {
        if let Some(p) = self.playing(now) {
            return p.level(now);
        }
        match self.state {
            State::Speaking => 0.0,
            State::Thinking => THINKING,
            State::Listening if self.muted => 0.0,
            State::Listening => self.level * MIC,
        }
    }

    /// How far the thread swings now, following its target smoothly: quickly as it
    /// rises and slower as it falls.
    pub fn swing(&mut self, now: Instant) -> f32 {
        let target = self.target(now);
        let dt = self
            .swung
            .map(|t| now.saturating_duration_since(t).as_secs_f32())
            .unwrap_or(0.0)
            .min(0.25);
        let tau = if target > self.swing { RISE } else { FALL };
        self.swing += (target - self.swing) * (1.0 - (-dt / tau).exp());
        self.swung = Some(now);
        self.swing
    }

    /// Whether the thread is drawn in ink, as while the agent speaks, or the quiet grey.
    pub fn inked(&self, now: Instant) -> bool {
        self.playing(now).is_some() || self.state == State::Speaking
    }

    /// The time it has run, m:ss.
    pub fn clock(&self, now: Instant) -> String {
        let secs = self
            .since
            .map(|t| now.saturating_duration_since(t).as_secs())
            .unwrap_or(0);
        format!("{}:{:02}", secs / 60, secs % 60)
    }

    /// The frames for the buttons: starting with an agent in its chat, Mute and End.
    pub fn start(agent: &str, conversation: &str) -> Value {
        json!({"t": "live.start", "agent": agent, "conversation": conversation})
    }

    pub fn mute(&self) -> Value {
        json!({"t": "live.mute", "on": !self.muted})
    }

    pub fn end() -> Value {
        json!({"t": "live.end"})
    }
}

/// Where the thread is at `u` (0 to 1 across) at `t` seconds, as a share of how far it
/// can swing: two waves, the ends held still.
pub fn wave(u: f32, t: f32) -> f32 {
    use std::f32::consts::PI;
    let held = (PI * u).sin();
    held * (0.6 * (2.0 * PI * (1.5 * u - 0.9 * t)).sin()
        + 0.4 * (2.0 * PI * (2.5 * u + 0.6 * t)).sin())
}

/// Fake frames for the perf build (`LOOM_LIVE=speaking` or `listening`), with levels and
/// words made up, so the view can be looked at with no mic or service.
pub fn sample(state: &str, agent: &str, name: &str, conversation: &str) -> Vec<Value> {
    let base = json!({"t": "core.live", "agent": agent, "name": name, "conversation": conversation,
        "started": 0, "muted": false, "captions": true});
    let mut f = base.clone();
    if state == "speaking" {
        f["state"] = json!("speaking");
        let words = [
            "The", "build", "passed", "on", "main,", "in", "two", "minutes,", "and", "the",
            "deploy", "went", "out", "at", "two.",
        ];
        let at: Vec<u64> = (0..words.len() as u64).map(|i| 400 + i * 700).collect();
        let levels: Vec<f64> = (0..600)
            .map(|i| {
                let x = i as f64 * 0.31;
                (0.55 + 0.3 * (x).sin() * (x * 0.37).cos()).clamp(0.0, 1.0)
            })
            .collect();
        f["play"] = json!({"file": "", "words": words, "at": at, "levels": levels, "step": 20, "ms": 12000});
    } else {
        f["state"] = json!("listening");
        f["words"] = json!("What's on my plate today?");
        f["level"] = json!(0.7);
    }
    vec![f]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn frame(v: Value) -> Live {
        let mut l = Live::default();
        l.take_at(&v, Instant::now());
        l
    }

    #[test]
    fn it_takes_what_the_core_says() {
        let mut l = frame(
            json!({"t": "core.live", "state": "thinking", "agent": "a_1", "name": "Juniper",
            "conversation": "c_1", "muted": false, "captions": false, "words": "hello there"}),
        );
        assert!(l.on && l.state == State::Thinking && l.name == "Juniper" && !l.show_words);
        assert_eq!(State::Thinking.word(), "Thinking");
        let now = Instant::now();
        assert_eq!(l.caption(now), (vec!["hello".into(), "there".into()], 2));
        assert!((l.target(now) - THINKING).abs() < 1e-6);
        l.take_at(&json!({"t": "core.live", "state": "ended"}), now);
        assert!(!l.on);
        assert!(!Live::default().take(&json!({"t": "core.speak"})));
    }

    #[test]
    fn a_piece_says_its_words_by_the_clock() {
        let start = Instant::now();
        let mut l = Live::default();
        l.take_at(&json!({"t": "core.live", "state": "speaking", "play": {"file": "a.wav",
            "words": ["Sure,", "it", "passed."], "at": [0, 350, 550], "levels": [0.1, 0.9, 0.5], "step": 20, "ms": 1000}}), start);
        assert_eq!(l.play.take().as_deref(), Some("a.wav"));
        let at = |ms| start + Duration::from_millis(ms);
        assert_eq!(l.caption(at(10)).1, 1);
        assert_eq!(l.caption(at(400)).1, 2);
        assert_eq!(l.caption(at(2000)).1, 3);
        assert!((l.target(at(25)) - 0.9).abs() < 1e-6);
        assert!(l.inked(at(25)));
        // The thread follows quickly as it rises, and doesn't jump.
        l.swing(at(20));
        let once = l.swing(at(36));
        assert!(once > 0.1 && once < 0.9);
        // Talking over it stops it.
        l.take_at(
            &json!({"t": "core.live", "state": "listening", "stop": true, "level": 0.6}),
            at(40),
        );
        assert!(
            l.stop
                && l.piece.is_none()
                && (l.target(at(40)) - 0.3).abs() < 1e-6
                && !l.inked(at(40))
        );
    }

    #[test]
    fn muted_lies_still_and_buttons_send() {
        let l = frame(json!({"t": "core.live", "state": "listening", "muted": true, "level": 0.8}));
        assert_eq!(l.target(Instant::now()), 0.0);
        assert_eq!(l.mute(), json!({"t": "live.mute", "on": false}));
        assert_eq!(
            Live::start("a_1", "c_1"),
            json!({"t": "live.start", "agent": "a_1", "conversation": "c_1"})
        );
        assert_eq!(Live::end(), json!({"t": "live.end"}));
        assert_eq!(l.clock(l.since.unwrap() + Duration::from_secs(75)), "1:15");
    }

    #[test]
    fn the_main_button_is_voice_mode_while_the_composer_is_empty() {
        assert_eq!(Main::of(false, true, true), Main::Live);
        assert_eq!(Main::of(false, false, true), Main::Send);
        // A group chat has no live mode, so its button stays Send.
        assert_eq!(Main::of(false, true, false), Main::Send);
        // While a reply streams, Stop comes first, unless there's something to queue.
        assert_eq!(Main::of(true, true, true), Main::Stop);
        assert_eq!(Main::of(true, false, true), Main::Send);
        assert_eq!(Main::Live.name(), "Start voice mode");
        assert_eq!(Main::Send.name(), "Send");
    }

    #[test]
    fn no_engine_opens_the_page() {
        let l = frame(
            json!({"t": "core.live", "state": "no_engine", "kind": "speech", "why": "Set up speech"}),
        );
        assert!(!l.on && l.open_page == Some(("speech".into(), "Set up speech".into())));
    }

    #[test]
    fn the_thread_is_held_at_both_ends() {
        for t in [0.0, 0.3, 1.7] {
            assert!(wave(0.0, t).abs() < 1e-5 && wave(1.0, t).abs() < 1e-5);
        }
        assert!(wave(0.3, 0.1).abs() > 0.01);
    }
}
