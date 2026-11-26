//! The card a routine leaves in the chat where it posts when something other than its
//! person turns it off: five failures in a row, or its agent's pause_routine (PLAN.md,
//! "Routines"). While it's off, its name, Off, why on one line, and Turn on, which is
//! `routine.put` with `on`. Once it's on again, from the card or anywhere else, or
//! deleted, the instance says so on the card (`message.updated`), and it's one quiet
//! line with no button. It isn't about the session's context, so Show context doesn't
//! hide it.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use super::{s, Card, CardBody, Message, Model};

/// What the card says, as the chat draws it.
#[derive(Clone, Debug, PartialEq)]
pub enum Shown {
    /// Off: why, Turn on, whether Turn on is on its way, and what the instance said to
    /// one it refused.
    Off {
        why: String,
        sending: bool,
        error: String,
    },
    /// On again.
    On,
    /// The routine was deleted.
    Deleted,
}

/// The Turn ons on their way, by routine, and what the instance said to ones it
/// refused.
#[derive(Clone, Debug, Default)]
pub struct Turning {
    sending: HashSet<String>,
    errors: HashMap<String, String>,
}

const ANSWER: &str = "app.routine_on:";

/// The card's body, from the instance's `routine_off` card.
pub(super) fn body_of(v: &Value) -> CardBody {
    CardBody::RoutineOff {
        routine: s(v, "routine"),
        why: s(v, "why"),
        paused_by: s(v, "agent"),
        agent: String::new(),
        failures: v.get("failures").and_then(Value::as_i64).unwrap_or(0),
        state: s(v, "state"),
        sending: false,
        error: String::new(),
    }
}

/// The routine_off card a message is: a `loom` message with that one card.
pub fn card_of(m: &Message) -> Option<&Card> {
    if m.role != "loom" {
        return None;
    }
    m.blocks
        .iter()
        .find_map(|b| b.card.as_ref())
        .filter(|c| matches!(c.body, CardBody::RoutineOff { .. }))
}

/// What a routine_off card says now.
pub fn shown(card: &Card) -> Option<Shown> {
    let CardBody::RoutineOff {
        why,
        paused_by,
        agent,
        failures,
        state,
        sending,
        error,
        ..
    } = &card.body
    else {
        return None;
    };
    Some(match state.as_str() {
        "on" => Shown::On,
        "deleted" => Shown::Deleted,
        _ => Shown::Off {
            why: why_line(why, paused_by, agent, *failures),
            sending: *sending,
            error: error.clone(),
        },
    })
}

/// Why it's off, on one line: the agent that paused it and its reason, or the failures
/// and the last error.
fn why_line(why: &str, paused_by: &str, agent: &str, failures: i64) -> String {
    let why = why.trim();
    let who = if !paused_by.is_empty() {
        let name = if agent.is_empty() { "Its agent" } else { agent };
        format!("{name} paused it")
    } else {
        format!("Failed {failures} times in a row")
    };
    if why.is_empty() {
        who
    } else {
        format!("{who}: {why}")
    }
}

impl Model {
    /// Turn on, on routine `routine`'s card: it runs again with its schedule as it was.
    pub fn turn_routine_on(&mut self, routine: &str) -> Value {
        self.routine_off.errors.remove(routine);
        self.routine_off.sending.insert(routine.to_string());
        json!({"t": "cmd", "id": format!("{ANSWER}{routine}"), "name": "routine.put", "args": {"id": routine, "on": true}})
    }

    /// Turn on's answer: the card's own message.updated says it's on, and a refusal
    /// shows on the card, which still has Turn on.
    pub(super) fn take_routine_on_answer(&mut self, id: &str, f: &Value) -> bool {
        let Some(routine) = id.strip_prefix(ANSWER) else {
            return false;
        };
        self.routine_off.sending.remove(routine);
        if let Some(e) = f.get("error") {
            self.routine_off
                .errors
                .insert(routine.to_string(), s(e, "message"));
        }
        true
    }

    /// Each card in the chat with the name of the agent that paused its routine, and
    /// whether its Turn on is on its way, or was refused.
    pub(super) fn routine_off_cards(&self, shown: &mut [Message]) {
        for m in shown.iter_mut().filter(|m| m.role == "loom") {
            for b in m.blocks.iter_mut() {
                if let Some(CardBody::RoutineOff {
                    routine,
                    paused_by,
                    agent,
                    sending,
                    error,
                    ..
                }) = b.card.as_mut().map(|c| &mut c.body)
                {
                    *agent = self
                        .agent(paused_by)
                        .map(|a| a.name.clone())
                        .unwrap_or_default();
                    *sending = self.routine_off.sending.contains(routine.as_str());
                    *error = self
                        .routine_off
                        .errors
                        .get(routine.as_str())
                        .cloned()
                        .unwrap_or_default();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card_message(card: Value) -> Value {
        json!({"id": "msg_9", "conversation": "c_1", "role": "loom", "at": 5,
               "blocks": [{"kind": "card", "card": card}]})
    }

    fn failed(state: &str) -> Value {
        card_message(
            json!({"id": "msg_9", "type": "routine_off", "title": "Prices", "routine": "rt_1",
                            "why": "the shop was down", "failures": 5, "state": state}),
        )
    }

    /// The agent's chat, open, with `messages` come into it.
    fn open(m: &mut Model, messages: Vec<Value>) {
        m.take(
            &json!({"t": "core.data", "what": "conversations", "items": [
                {"id": "c_1", "agent": "a_1", "at": 1},
            ]}),
        );
        m.select(Some("a_1".into()));
        for v in messages {
            m.take(&json!({"t": "event", "kind": "message.created", "conversation": "c_1", "message": v}));
        }
    }

    fn shown_card(m: &Model) -> Shown {
        let out = m.shown();
        let msg = out
            .iter()
            .find(|x| x.id == "msg_9")
            .expect("the card's message");
        shown(card_of(msg).expect("a routine_off card")).expect("what it says")
    }

    #[test]
    fn off_after_failures_says_why_and_turns_on() {
        let mut m = Model::new();
        open(&mut m, vec![failed("off")]);
        assert_eq!(
            shown_card(&m),
            Shown::Off {
                why: "Failed 5 times in a row: the shop was down".into(),
                sending: false,
                error: String::new()
            }
        );
        let cmd = m.turn_routine_on("rt_1");
        assert_eq!(cmd["name"], "routine.put");
        assert_eq!(cmd["args"], json!({"id": "rt_1", "on": true}));
        assert!(matches!(shown_card(&m), Shown::Off { sending: true, .. }));
        // The instance's answer, then the card as it is now.
        m.take(&json!({"t": "event", "kind": "message.updated", "conversation": "c_1", "message": failed("on")}));
        m.take(&json!({"t": "result", "id": cmd["id"], "ok": true, "value": {}}));
        assert_eq!(shown_card(&m), Shown::On);
    }

    #[test]
    fn paused_by_an_agent_names_it() {
        let mut m = Model::new();
        m.take(&json!({"t": "core.data", "what": "agents", "items": [
            {"id": "a_1", "name": "Main", "acp_agent": "claude"},
        ]}));
        open(
            &mut m,
            vec![card_message(
                json!({"id": "msg_9", "type": "routine_off", "title": "Sale",
                                     "routine": "rt_2", "why": "the sale is over", "agent": "a_1", "state": "off"}),
            )],
        );
        assert!(
            matches!(shown_card(&m), Shown::Off { why, .. } if why == "Main paused it: the sale is over")
        );
    }

    #[test]
    fn a_refused_turn_on_says_why_on_the_card() {
        let mut m = Model::new();
        open(&mut m, vec![failed("off")]);
        let cmd = m.turn_routine_on("rt_1");
        m.take(&json!({"t": "result", "id": cmd["id"], "ok": false,
                       "error": {"code": "not_found", "message": "There's no agent a_1."}}));
        assert_eq!(
            shown_card(&m),
            Shown::Off {
                why: "Failed 5 times in a row: the shop was down".into(),
                sending: false,
                error: "There's no agent a_1.".into()
            }
        );
        // Turn on again clears it.
        m.turn_routine_on("rt_1");
        assert!(
            matches!(shown_card(&m), Shown::Off { sending: true, error, .. } if error.is_empty())
        );
    }

    #[test]
    fn on_elsewhere_or_deleted_goes_quiet() {
        let mut m = Model::new();
        open(&mut m, vec![failed("off")]);
        m.take(&json!({"t": "event", "kind": "message.updated", "conversation": "c_1", "message": failed("on")}));
        assert_eq!(shown_card(&m), Shown::On);
        m.take(&json!({"t": "event", "kind": "message.updated", "conversation": "c_1", "message": failed("deleted")}));
        assert_eq!(shown_card(&m), Shown::Deleted);
    }

    #[test]
    fn show_context_off_keeps_it() {
        let mut m = Model::new();
        open(&mut m, vec![failed("off")]);
        assert!(m.shown_with(false).iter().any(|x| x.id == "msg_9"));
    }

    #[test]
    fn only_a_loom_message_is_one() {
        let mut m = Model::new();
        let mut v = failed("off");
        v["role"] = json!("agent");
        open(&mut m, vec![v]);
        let out = m.shown();
        assert!(out.iter().all(|x| card_of(x).is_none()));
    }
}
