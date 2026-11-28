//! A chat its agent program moved to another model after a model's safeguards flagged
//! a request (PLAN.md, "A model fallback"). The conversation keeps `model_now` while it's
//! there, which the brain chip shows in place of the agent's pick, and loom's card under
//! the reply says so, with Switch back (`conversation.model_back`) and New chat
//! (`conversation.create` with `from`). Once either is used, or the person picks a
//! model, the instance says what happened on the card (`message.updated`), and it's one
//! quiet line with no buttons.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use super::{s, Card, CardBody, Message, Model};

/// The model a chat is on in place of the one loom set: its value and name, and the
/// model it was on before, by value and name.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelNow {
    pub value: String,
    pub name: String,
    pub from: String,
    pub from_name: String,
}

/// A conversation's `model_now`, when it's a fallback's.
pub(super) fn model_now_of(v: Option<&Value>) -> Option<ModelNow> {
    let v = v?;
    if v.get("fallback") != Some(&Value::Bool(true)) {
        return None;
    }
    Some(ModelNow {
        value: s(v, "value"),
        name: s(v, "name"),
        from: s(v, "from"),
        from_name: s(v, "from_name"),
    })
}

/// What the brain chip says while a chat is on a fallback: the model's name, and that
/// it's a fallback, whole or, in a narrow header, the name alone.
pub fn chip_words(now: &ModelNow, whole: bool) -> String {
    if whole {
        format!("{} · fallback", now.name)
    } else {
        now.name.clone()
    }
}

/// Which of the card's buttons is on its way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Going {
    Back,
    NewChat,
}

/// What the card says, as the chat draws it.
#[derive(Clone, Debug, PartialEq)]
pub enum Shown {
    /// On the fallback: its title, Switch back to `from`, New chat on `from`, which of
    /// them is on its way, and what the instance said to one it refused.
    On {
        title: String,
        from: String,
        going: Option<Going>,
        error: String,
    },
    /// Done with: what happened, in one line.
    Done(String),
}

/// The buttons on their way, by conversation, and what the instance said to ones it
/// refused.
#[derive(Clone, Debug, Default)]
pub struct Pending {
    back: HashSet<String>,
    new_chat: HashSet<String>,
    errors: HashMap<String, String>,
}

const BACK: &str = "app.model_back:";
const NEW_CHAT: &str = "app.fallback_chat:";

/// The card's body, from the instance's `model_fallback` card.
pub(super) fn body_of(v: &Value) -> CardBody {
    CardBody::ModelFallback {
        agent: s(v, "agent"),
        from_name: s(v, "from_name"),
        state: s(v, "state"),
        said: s(v, "said"),
        going: None,
        error: String::new(),
    }
}

/// The model_fallback card a message is: a `loom` message with that one card.
pub fn card_of(m: &Message) -> Option<&Card> {
    if m.role != "loom" {
        return None;
    }
    m.blocks
        .iter()
        .find_map(|b| b.card.as_ref())
        .filter(|c| matches!(c.body, CardBody::ModelFallback { .. }))
}

/// What a model_fallback card says now.
pub fn shown(card: &Card) -> Option<Shown> {
    let CardBody::ModelFallback {
        from_name,
        state,
        said,
        going,
        error,
        ..
    } = &card.body
    else {
        return None;
    };
    Some(if state == "fallback" {
        Shown::On {
            title: card.title.clone(),
            from: from_name.clone(),
            going: *going,
            error: error.clone(),
        }
    } else {
        Shown::Done(said.clone())
    })
}

impl Model {
    /// The model the open chat is on in place of the agent's pick, while it's on a
    /// fallback.
    pub fn model_now(&self) -> Option<&ModelNow> {
        let id = self.conversation()?;
        self.conversations
            .iter()
            .find(|c| c.id == id)?
            .model_now
            .as_ref()
    }

    /// What the brain chip says for `agent` in the open chat, whole and with the model
    /// alone: the fallback's name while the chat is on one, and otherwise the agent's
    /// pick. None for an agent whose program has nothing to pick.
    pub fn brain_words(&self, agent: &super::Agent) -> (Option<String>, Option<String>) {
        match self.model_now() {
            Some(now) => (Some(chip_words(now, true)), Some(chip_words(now, false))),
            None => (agent.picked(), agent.picked_model()),
        }
    }

    /// Switch back, on chat `conversation`'s card: its session goes back to the model it
    /// was on.
    pub fn switch_model_back(&mut self, conversation: &str) -> Value {
        self.fallback_ui.errors.remove(conversation);
        self.fallback_ui.back.insert(conversation.to_string());
        json!({"t": "cmd", "id": format!("{BACK}{conversation}"), "name": "conversation.model_back", "args": {"conversation": conversation}})
    }

    /// New chat, on chat `conversation`'s card: a new chat with its agent, on the agent's
    /// own pick, as Start fresh makes, which the card says took over.
    pub fn new_chat_from(&mut self, conversation: &str, agent: &str) -> Value {
        self.fallback_ui.errors.remove(conversation);
        self.fallback_ui.new_chat.insert(conversation.to_string());
        json!({"t": "cmd", "id": format!("{NEW_CHAT}{conversation}"), "name": "conversation.create", "args": {"agent": agent, "from": conversation}})
    }

    /// Either button's answer: the card's own message.updated says what happened, and a
    /// refusal shows on the card, which keeps its buttons.
    pub(super) fn take_fallback_answer(&mut self, id: &str, f: &Value) -> bool {
        let Some(conversation) = id.strip_prefix(BACK).or_else(|| id.strip_prefix(NEW_CHAT)) else {
            return false;
        };
        self.fallback_ui.back.remove(conversation);
        self.fallback_ui.new_chat.remove(conversation);
        if let Some(e) = f.get("error") {
            self.fallback_ui
                .errors
                .insert(conversation.to_string(), s(e, "message"));
        }
        true
    }

    /// Each card in the chat with whether one of its buttons is on its way, or was
    /// refused.
    pub(super) fn fallback_cards(&self, shown: &mut [Message]) {
        let conversation = self.conversation().unwrap_or_default();
        for m in shown.iter_mut().filter(|m| m.role == "loom") {
            for b in m.blocks.iter_mut() {
                if let Some(CardBody::ModelFallback { going, error, .. }) =
                    b.card.as_mut().map(|c| &mut c.body)
                {
                    *going = if self.fallback_ui.back.contains(&conversation) {
                        Some(Going::Back)
                    } else if self.fallback_ui.new_chat.contains(&conversation) {
                        Some(Going::NewChat)
                    } else {
                        None
                    };
                    *error = self
                        .fallback_ui
                        .errors
                        .get(&conversation)
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

    fn card(state: &str, said: &str) -> Value {
        json!({"id": "msg_9", "conversation": "c_1", "role": "loom", "at": 5,
               "blocks": [{"kind": "card", "card": {
                   "id": "msg_9", "type": "model_fallback", "title": "This chat is on Sonnet 5.5 now.",
                   "agent": "a_1", "model": "sonnet", "model_name": "Sonnet 5.5",
                   "from": "default", "from_name": "Opus 5.5", "state": state, "said": said}}]})
    }

    fn fallen() -> Value {
        json!({"value": "sonnet", "name": "Sonnet 5.5", "fallback": true, "from": "default", "from_name": "Opus 5.5"})
    }

    /// The agent's chat, open, with `messages` come into it.
    fn open(m: &mut Model, model_now: Option<Value>, messages: Vec<Value>) {
        let mut c = json!({"id": "c_1", "agent": "a_1", "at": 1});
        if let Some(now) = model_now {
            c["model_now"] = now;
        }
        m.take(&json!({"t": "core.data", "what": "agents", "items": [
            {"id": "a_1", "name": "Main", "acp_agent": "claude"},
        ]}));
        m.take(&json!({"t": "core.data", "what": "conversations", "items": [c]}));
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
        shown(card_of(msg).expect("a model_fallback card")).expect("what it says")
    }

    #[test]
    fn the_chip_names_the_fallback_not_the_pick() {
        let mut m = Model::new();
        open(&mut m, Some(fallen()), vec![]);
        let now = m.model_now().expect("on a fallback");
        assert_eq!(chip_words(now, true), "Sonnet 5.5 · fallback");
        assert_eq!(chip_words(now, false), "Sonnet 5.5");
        assert_eq!(now.from_name, "Opus 5.5");
        let agent = m.agent("a_1").expect("the agent").clone();
        assert_eq!(
            m.brain_words(&agent),
            (
                Some("Sonnet 5.5 · fallback".into()),
                Some("Sonnet 5.5".into())
            )
        );
        // Back on its own model, the chip says the agent's pick again.
        m.take(
            &json!({"t": "core.data", "what": "conversations", "items": [
                {"id": "c_1", "agent": "a_1", "at": 1, "model_now": null},
            ]}),
        );
        assert_eq!(m.model_now(), None);
        assert_eq!(
            m.brain_words(&agent),
            (agent.picked(), agent.picked_model())
        );
    }

    #[test]
    fn a_chat_on_its_own_model_has_none() {
        let mut m = Model::new();
        open(&mut m, None, vec![]);
        assert_eq!(m.model_now(), None);
        let mut other = Model::new();
        open(
            &mut other,
            Some(json!({"value": "sonnet", "name": "Sonnet"})),
            vec![],
        );
        assert_eq!(other.model_now(), None);
    }

    #[test]
    fn switch_back_and_new_chat_send_their_commands() {
        let mut m = Model::new();
        open(&mut m, Some(fallen()), vec![card("fallback", "")]);
        assert_eq!(
            shown_card(&m),
            Shown::On {
                title: "This chat is on Sonnet 5.5 now.".into(),
                from: "Opus 5.5".into(),
                going: None,
                error: String::new()
            }
        );
        let back = m.switch_model_back("c_1");
        assert_eq!(back["name"], "conversation.model_back");
        assert_eq!(back["args"], json!({"conversation": "c_1"}));
        assert!(matches!(
            shown_card(&m),
            Shown::On {
                going: Some(Going::Back),
                ..
            }
        ));
        m.take(&json!({"t": "result", "id": back["id"], "ok": false,
                       "error": {"code": "not_found", "message": "There's no conversation c_1."}}));
        assert!(
            matches!(shown_card(&m), Shown::On { going: None, error, .. } if error == "There's no conversation c_1.")
        );
        let new_chat = m.new_chat_from("c_1", "a_1");
        assert_eq!(new_chat["name"], "conversation.create");
        assert_eq!(new_chat["args"], json!({"agent": "a_1", "from": "c_1"}));
        assert!(matches!(
            shown_card(&m),
            Shown::On {
                going: Some(Going::NewChat),
                ..
            }
        ));
    }

    #[test]
    fn once_used_it_says_what_happened() {
        let mut m = Model::new();
        open(&mut m, Some(fallen()), vec![card("fallback", "")]);
        let back = m.switch_model_back("c_1");
        m.take(&json!({"t": "event", "kind": "message.updated", "conversation": "c_1", "message": card("back", "Back on Opus 5.5.")}));
        m.take(&json!({"t": "result", "id": back["id"], "ok": true, "value": {}}));
        assert_eq!(shown_card(&m), Shown::Done("Back on Opus 5.5.".into()));
        m.take(&json!({"t": "event", "kind": "message.updated", "conversation": "c_1", "message": card("moved", "Moved to a new chat.")}));
        assert_eq!(shown_card(&m), Shown::Done("Moved to a new chat.".into()));
    }

    #[test]
    fn show_context_off_keeps_it() {
        let mut m = Model::new();
        open(&mut m, Some(fallen()), vec![card("fallback", "")]);
        assert!(m.shown_with(false).iter().any(|x| x.id == "msg_9"));
    }
}
