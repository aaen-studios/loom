//! A draft card (PLAN.md, "Drafts"): an email or a message an agent proposes, which
//! the person sends, edits or rejects, and which loom sends as them. The whole of what
//! goes out is on the card: the service's mark, "Draft email" and the thread a reply
//! goes in; From, To, Cc and Subject as rows; the body; then "The message it answers",
//! folded, and Reject, Edit and Send, with Send later under the arrow. Edit makes the
//! rows and the body editable in place. Once sent it's one line, the subject and where
//! it went, Sent and its time, and Undo with its seconds left while it can still be
//! taken back. One that couldn't go says why, with Try again. `draft.answer` does each.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use super::{s, CardBody, Message, Model};

/// A row of a draft: its field, its label, what it says, and whether Edit changes it.
#[derive(Clone, Debug, PartialEq)]
pub struct DraftRow {
    pub key: String,
    pub label: String,
    pub value: String,
    pub edit: bool,
}

/// A draft as the instance keeps it on the card, with what the app keeps of it: the
/// reply it's in, whether it's being edited, whether Reject's note is open, whether the
/// message it answers is unfolded, whether Send later's times are showing, whether an
/// answer is on its way, and what the instance said to one it refused.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Draft {
    pub message: String,
    pub kind: String,
    pub service: String,
    pub mark: String,
    pub thread: String,
    pub answers: String,
    pub rows: Vec<DraftRow>,
    pub body: String,
    pub state: String,
    pub at: i64,
    pub until: i64,
    pub why: String,
    pub edited: bool,
    pub editing: bool,
    pub rejecting: bool,
    pub answers_open: bool,
    pub later_open: bool,
    pub sending: bool,
    pub error: String,
}

/// What a draft card says now.
#[derive(Clone, Debug, PartialEq)]
pub enum Shown {
    /// Waiting for the person: the whole draft, and its buttons.
    Open,
    /// Sent, and still to be taken back for `left` seconds more.
    Going {
        what: String,
        to: String,
        at: i64,
        left: i64,
    },
    /// Sent, at `at`; or on its way, once Undo's time is up.
    Sent { what: String, to: String, at: i64 },
    /// To go at `at`.
    Later { what: String, to: String, at: i64 },
    /// It couldn't go, and why.
    Failed {
        what: String,
        to: String,
        why: String,
    },
    /// Rejected.
    Rejected { what: String },
}

const ANSWER: &str = "app.draft:";

/// The card's body, from the instance's `draft` card.
pub(super) fn body_of(v: &Value) -> CardBody {
    let rows = v
        .get("rows")
        .and_then(Value::as_array)
        .map(|xs| {
            xs.iter()
                .map(|x| DraftRow {
                    key: s(x, "key"),
                    label: s(x, "label"),
                    value: s(x, "value"),
                    edit: x.get("edit") == Some(&Value::Bool(true)),
                })
                .collect()
        })
        .unwrap_or_default();
    let ms = |k: &str| v.get(k).and_then(Value::as_i64).unwrap_or(0);
    CardBody::Draft(Box::new(Draft {
        kind: s(v, "kind"),
        service: s(v, "service"),
        mark: s(v, "icon_svg"),
        thread: s(v, "thread"),
        answers: s(v, "answers"),
        rows,
        body: s(v, "body"),
        state: s(v, "state"),
        at: ms("at"),
        until: ms("until"),
        why: s(v, "why"),
        edited: v.get("edited") == Some(&Value::Bool(true)),
        ..Draft::default()
    }))
}

impl Draft {
    /// Its line once it's gone or going: the subject, or the start of the body, and
    /// where it goes.
    pub fn line(&self) -> (String, String) {
        let row = |k: &str| {
            self.rows
                .iter()
                .find(|r| r.key == k)
                .map(|r| r.value.clone())
                .unwrap_or_default()
        };
        let mut what = row("subject");
        if what.is_empty() {
            let flat = self.body.split_whitespace().collect::<Vec<_>>().join(" ");
            what = match flat.char_indices().nth(60) {
                Some((at, _)) => format!("{}…", &flat[..at]),
                None => flat,
            };
        }
        (what, row("to"))
    }

    /// What it says at `now` (milliseconds since 1970).
    pub fn shown(&self, now: i64) -> Shown {
        let (what, to) = self.line();
        match self.state.as_str() {
            "sending" if self.until > now => Shown::Going {
                what,
                to,
                at: self.at,
                left: ((self.until - now) + 999) / 1000,
            },
            "sending" | "sent" => Shown::Sent {
                what,
                to,
                at: self.at,
            },
            "later" => Shown::Later {
                what,
                to,
                at: self.at,
            },
            "failed" => Shown::Failed {
                what,
                to,
                why: self.why.clone(),
            },
            "rejected" => Shown::Rejected { what },
            _ => Shown::Open,
        }
    }

    /// The rows the card shows: From, To and Subject always, Cc and the rest where
    /// they say something, and while it's edited, an empty Cc to fill in for an email.
    pub fn rows_shown(&self) -> Vec<DraftRow> {
        let mut out: Vec<DraftRow> = self.rows.clone();
        if self.editing && self.kind == "email" && !out.iter().any(|r| r.key == "cc") {
            let at = out
                .iter()
                .position(|r| r.key == "to")
                .map(|i| i + 1)
                .unwrap_or(out.len());
            out.insert(
                at,
                DraftRow {
                    key: "cc".into(),
                    label: "Cc".into(),
                    value: String::new(),
                    edit: true,
                },
            );
        }
        out
    }
}

/// Send later's times from `now` (local), each its words and when, in milliseconds:
/// in an hour, in three hours, and tomorrow at 08:00.
pub fn later_times<Tz: chrono::TimeZone>(now: chrono::DateTime<Tz>) -> Vec<(String, i64)> {
    let ms = now.timestamp_millis();
    let mut out = vec![
        ("In 1 hour".to_string(), ms + 3_600_000),
        ("In 3 hours".to_string(), ms + 3 * 3_600_000),
    ];
    let tomorrow = now
        .date_naive()
        .succ_opt()
        .and_then(|d| d.and_hms_opt(8, 0, 0));
    if let Some(t) = tomorrow.and_then(|t| now.timezone().from_local_datetime(&t).earliest()) {
        out.push(("Tomorrow at 08:00".to_string(), t.timestamp_millis()));
    }
    out
}

/// What the app keeps of the chat's draft cards, by `message/card`: those being edited,
/// with Reject's note open, with the message they answer unfolded, and with Send later
/// showing; the answers on their way, and what the instance said to ones it refused.
#[derive(Clone, Debug, Default)]
pub struct DraftUi {
    editing: HashSet<String>,
    rejecting: HashSet<String>,
    open: HashSet<String>,
    later: HashSet<String>,
    sending: HashSet<String>,
    errors: HashMap<String, String>,
}

/// A draft's key, as the app keeps it.
pub fn key(message: &str, card: &str) -> String {
    format!("{message}/{card}")
}

/// What `toggle_draft` turns on or off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    Edit,
    Reject,
    Answers,
    Later,
}

impl Model {
    /// The person's answer on a draft: `send` (with `edits`, by field, when it was
    /// edited), `later` (at `at`), `reject` (with `note`), `undo` or `retry`.
    pub fn answer_draft(
        &mut self,
        message: &str,
        card: &str,
        answer: &str,
        edits: Option<Value>,
        note: &str,
        at: i64,
    ) -> Value {
        let k = key(message, card);
        self.draft_ui.errors.remove(&k);
        self.draft_ui.sending.insert(k.clone());
        self.draft_ui.later.remove(&k);
        let mut args = json!({"message": message, "card": card, "answer": answer});
        if let Some(e) = edits {
            args["edits"] = e;
        }
        if answer == "reject" && !note.trim().is_empty() {
            args["note"] = json!(note.trim());
        }
        if answer == "later" {
            args["at"] = json!(at);
        }
        json!({"t": "cmd", "id": format!("{ANSWER}{answer}:{k}"), "name": "draft.answer", "args": args})
    }

    /// Edit, Reject's note, the message it answers, or Send later's times, on or off.
    pub fn toggle_draft(&mut self, message: &str, card: &str, what: Toggle, on: bool) {
        let k = key(message, card);
        let set = match what {
            Toggle::Edit => &mut self.draft_ui.editing,
            Toggle::Reject => &mut self.draft_ui.rejecting,
            Toggle::Answers => &mut self.draft_ui.open,
            Toggle::Later => &mut self.draft_ui.later,
        };
        if on {
            set.insert(k);
        } else {
            set.remove(&k);
        }
    }

    /// An answer came: a refusal shows on the card, which stays as it was; once it's
    /// taken, the card's own message.updated says how it stands, and Edit and Reject's
    /// note close.
    pub(super) fn take_draft_answer(&mut self, id: &str, f: &Value) -> bool {
        let Some(rest) = id.strip_prefix(ANSWER) else {
            return false;
        };
        let (_, k) = rest.split_once(':').unwrap_or(("", rest));
        self.draft_ui.sending.remove(k);
        if let Some(e) = f.get("error") {
            self.draft_ui.errors.insert(k.to_string(), s(e, "message"));
        } else {
            self.draft_ui.errors.remove(k);
            self.draft_ui.editing.remove(k);
            self.draft_ui.rejecting.remove(k);
        }
        true
    }

    /// Each draft card in the chat with the reply it's in and what the app keeps of it.
    pub(super) fn draft_cards(&self, shown: &mut [Message]) {
        for m in shown.iter_mut() {
            let reply = m.id.clone();
            for b in m.blocks.iter_mut() {
                let Some(card) = b.card.as_mut() else {
                    continue;
                };
                let k = key(&reply, &card.id);
                if let CardBody::Draft(d) = &mut card.body {
                    d.message = reply.clone();
                    let open = d.state == "draft";
                    d.editing = open && self.draft_ui.editing.contains(&k);
                    d.rejecting = open && self.draft_ui.rejecting.contains(&k);
                    d.answers_open = self.draft_ui.open.contains(&k);
                    d.later_open = open && self.draft_ui.later.contains(&k);
                    d.sending = self.draft_ui.sending.contains(&k);
                    d.error = self.draft_ui.errors.get(&k).cloned().unwrap_or_default();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Card;

    fn card(state: &str, extra: Value) -> Value {
        let mut v = json!({"id": "draft_1", "type": "draft", "title": "Draft email", "kind": "email",
            "service": "gmail", "badge": {"name": "gmail", "initial": "G", "color": "sky"},
            "thread": "Partnership", "answers": "Can we meet?",
            "rows": [
                {"key": "from", "label": "From", "value": "mira@example.com", "edit": false},
                {"key": "to", "label": "To", "value": "hello@example.com", "edit": true},
                {"key": "subject", "label": "Subject", "value": "Re: Partnership", "edit": true}
            ],
            "body": "Hi Mira,\nThursday works.", "state": state});
        if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            for (k, x) in e {
                o.insert(k.clone(), x.clone());
            }
        }
        v
    }

    fn draft_of(v: Value) -> Draft {
        match Card::of(&v).expect("a card").body {
            CardBody::Draft(d) => *d,
            _ => panic!("a draft"),
        }
    }

    #[test]
    fn reads_the_whole_draft() {
        let d = draft_of(card("draft", json!({})));
        assert_eq!(d.rows.len(), 3);
        assert_eq!(d.rows[1].label, "To");
        assert!(d.rows[1].edit && !d.rows[0].edit);
        assert_eq!(d.thread, "Partnership");
        assert_eq!(d.shown(0), Shown::Open);
        assert_eq!(
            d.line(),
            (
                "Re: Partnership".to_string(),
                "hello@example.com".to_string()
            )
        );
    }

    #[test]
    fn says_how_it_went() {
        let going = draft_of(card("sending", json!({"at": 1_000, "until": 6_000})));
        assert_eq!(
            going.shown(1_500),
            Shown::Going {
                what: "Re: Partnership".into(),
                to: "hello@example.com".into(),
                at: 1_000,
                left: 5
            }
        );
        assert!(matches!(going.shown(6_000), Shown::Sent { at: 1_000, .. }));
        let failed = draft_of(card("failed", json!({"why": "Bad address"})));
        assert!(matches!(failed.shown(0), Shown::Failed { why, .. } if why == "Bad address"));
        assert!(matches!(
            draft_of(card("rejected", json!({}))).shown(0),
            Shown::Rejected { .. }
        ));
        assert!(matches!(
            draft_of(card("later", json!({"at": 9}))).shown(0),
            Shown::Later { at: 9, .. }
        ));
        // With no subject, the start of its body.
        let mut slack = draft_of(card("sent", json!({})));
        slack.rows.retain(|r| r.key != "subject");
        assert_eq!(slack.line().0, "Hi Mira, Thursday works.");
    }

    #[test]
    fn edit_offers_a_cc() {
        let mut d = draft_of(card("draft", json!({})));
        assert!(!d.rows_shown().iter().any(|r| r.key == "cc"));
        d.editing = true;
        let rows = d.rows_shown();
        assert_eq!(rows[2].key, "cc");
        assert!(rows[2].edit);
    }

    #[test]
    fn answers_go_as_draft_answer() {
        let mut m = Model::default();
        let f = m.answer_draft(
            "msg_1",
            "draft_1",
            "send",
            Some(json!({"body": "Hi"})),
            "",
            0,
        );
        assert_eq!(f["name"], "draft.answer");
        assert_eq!(
            f["args"],
            json!({"message": "msg_1", "card": "draft_1", "answer": "send", "edits": {"body": "Hi"}})
        );
        let r = m.answer_draft("msg_1", "draft_1", "reject", None, " Too formal. ", 0);
        assert_eq!(r["args"]["note"], "Too formal.");
        let l = m.answer_draft("msg_1", "draft_1", "later", None, "", 42);
        assert_eq!(l["args"]["at"], 42);
        m.toggle_draft("msg_1", "draft_1", Toggle::Edit, true);
        let id = f["id"].as_str().unwrap().to_string();
        assert!(m.take_draft_answer(
            &id,
            &json!({"ok": false, "error": {"code": "not_now", "message": "Answered already."}})
        ));
        let open = |m: &mut Model| {
            m.take(&json!({"t": "core.data", "what": "conversations", "items": [{"id": "c_1", "agent": "a_1", "at": 1}]}));
            m.select(Some("a_1".into()));
            m.take(
                &json!({"t": "event", "kind": "message.created", "conversation": "c_1", "message": {
                "id": "msg_1", "conversation": "c_1", "role": "agent", "agent": "a_1", "at": 1,
                "blocks": [{"kind": "card", "card": card("draft", json!({}))}]}}),
            );
        };
        let drawn = |m: &Model| -> Draft {
            let out = m.shown();
            let msg = out.iter().find(|x| x.id == "msg_1").expect("the reply");
            match &msg
                .blocks
                .iter()
                .find_map(|b| b.card.as_ref())
                .expect("its card")
                .body
            {
                CardBody::Draft(d) => (**d).clone(),
                _ => panic!("a draft"),
            }
        };
        open(&mut m);
        let d = drawn(&m);
        assert_eq!(d.message, "msg_1");
        assert!(d.editing);
        assert_eq!(d.error, "Answered already.");
        assert!(m.take_draft_answer(&id, &json!({"ok": true})));
        let d = drawn(&m);
        assert!(!d.editing && d.error.is_empty());
    }

    #[test]
    fn later_has_three_times() {
        use chrono::TimeZone;
        let utc = chrono::FixedOffset::east_opt(3600).unwrap();
        let now = utc.with_ymd_and_hms(2026, 11, 27, 22, 30, 0).unwrap();
        let times = later_times(now);
        assert_eq!(times.len(), 3);
        assert_eq!(times[0].1, now.timestamp_millis() + 3_600_000);
        let tomorrow = utc.with_ymd_and_hms(2026, 11, 28, 8, 0, 0).unwrap();
        assert_eq!(
            times[2],
            ("Tomorrow at 08:00".to_string(), tomorrow.timestamp_millis())
        );
    }
}
