//! The card an agent's spool_connect puts in its reply (PLAN.md, "An agent connects a
//! spool"): Spools' card for the entry, loom's knot tied to its logo, its name, address
//! and the agents it's for, with Sign in. Sign in is `spool.connect`; the instance
//! moves the card along in place, and once it waits for the sign-in, the app that sent
//! Sign in opens the person's browser, as Spools does. Connected, it says how many
//! tools, with Change…, which opens the entry in Spools; failed, why, with Try again.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use super::{s, CardBody, Model};

/// A key the entry takes, which the person types on the card.
#[derive(Clone, Debug, PartialEq)]
pub struct Secret {
    pub name: String,
    pub label: String,
    pub placeholder: String,
}

/// The Sign ins on their way, by card id, what the instance said to ones it refused,
/// and the cards whose Sign in this app sent, with their server, whose sign-in opens
/// here once the card waits for it.
#[derive(Clone, Debug, Default)]
pub struct Pending {
    sending: HashSet<String>,
    errors: HashMap<String, String>,
    waiting: HashMap<String, String>,
}

const ANSWER: &str = "app.spool_connect:";

/// The card's body, from the instance's `spool_connect` card.
pub(super) fn body_of(v: &Value) -> CardBody {
    let list = |key: &str| {
        v.get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    CardBody::SpoolConnect {
        entry: s(v, "entry"),
        name: s(v, "name"),
        icon_svg: s(v, "icon_svg"),
        server: s(v, "server"),
        address: s(v, "address"),
        agents: list("agents").iter().map(|a| s(a, "name")).collect(),
        secrets: list("secrets")
            .iter()
            .map(|x| Secret {
                name: s(x, "name"),
                label: s(x, "label"),
                placeholder: s(x, "placeholder"),
            })
            .collect(),
        state: s(v, "state"),
        said: s(v, "said"),
        tools: list("tools")
            .iter()
            .map(|t| t.as_str().unwrap_or_default().to_string())
            .collect(),
        message: String::new(),
        sending: false,
        error: String::new(),
    }
}

/// What the foot of the card says, beside its button.
#[derive(Clone, Debug, PartialEq)]
pub enum Foot {
    /// Asking: whom it's for, with Sign in.
    For(String),
    /// Being added and tried, or waiting for the browser.
    Connecting,
    SigningIn,
    /// Connected, with how many tools, and Change….
    Connected(usize),
    /// Failed, why, and Try again.
    Failed(String),
}

/// The foot of a card in `state`, for `agents`, with `said` and `tools`, and the
/// refusal the instance gave its last Sign in.
pub fn foot(state: &str, agents: &[String], said: &str, tools: usize, error: &str) -> Foot {
    match state {
        "connecting" => Foot::Connecting,
        "sign_in" if said.is_empty() => Foot::SigningIn,
        "sign_in" | "failed" => Foot::Failed(said.to_string()),
        "connected" => Foot::Connected(tools),
        _ if !error.is_empty() => Foot::Failed(error.to_string()),
        _ => Foot::For(format!("For {}", agents.join(", "))),
    }
}

/// "Connected · 12 tools", as the card says it.
pub fn connected_line(tools: usize) -> String {
    match tools {
        0 => "Connected".into(),
        1 => "Connected · 1 tool".into(),
        n => format!("Connected · {n} tools"),
    }
}

/// The address as the card shows it: the server's, without `https://` and the path
/// loom adds (`/mcp`).
pub fn short_address(address: &str) -> String {
    let bare = address
        .strip_prefix("https://")
        .or_else(|| address.strip_prefix("http://"))
        .unwrap_or(address);
    bare.strip_suffix("/mcp").unwrap_or(bare).to_string()
}

impl Model {
    /// Sign in, or Try again, on the spool card `card` in reply `message`, for its
    /// server `server`, with the keys typed on it: the instance connects it, and the
    /// person's browser opens here once it waits for the sign-in.
    pub fn spool_sign_in(
        &mut self,
        message: &str,
        card: &str,
        server: &str,
        secrets: &[(String, String)],
    ) -> Value {
        self.spool.errors.remove(card);
        self.spool.sending.insert(card.to_string());
        self.spool
            .waiting
            .insert(card.to_string(), server.to_string());
        let secrets: serde_json::Map<String, Value> = secrets
            .iter()
            .map(|(name, value)| (name.clone(), json!(value)))
            .collect();
        json!({"t": "cmd", "id": format!("{ANSWER}{card}"), "name": "spool.connect",
               "args": {"message": message, "card": card, "secrets": secrets}})
    }

    /// Sign in's answer: the card's own changes say how it goes, a connect already
    /// waiting for the sign-in opens the browser now, and a refusal shows on the card.
    pub(super) fn take_spool_answer(&mut self, id: &str, f: &Value) -> bool {
        let Some(card) = id.strip_prefix(ANSWER) else {
            return false;
        };
        self.spool.sending.remove(card);
        if let Some(e) = f.get("error") {
            self.spool.waiting.remove(card);
            self.spool.errors.insert(card.to_string(), s(e, "message"));
        } else if f["value"]["connect"]["step"] == "sign_in" {
            if let Some(server) = self.spool.waiting.remove(card) {
                self.spool_sign_ins.push(server);
            }
        }
        true
    }

    /// A spool card as the instance sent it: once one this app signed in from waits for
    /// the sign-in, its server's sign-in opens here, once.
    pub(super) fn spool_seen(&mut self, card: &Value) {
        if s(card, "type") != "spool_connect" {
            return;
        }
        let id = s(card, "id");
        match s(card, "state").as_str() {
            "sign_in" => {
                if let Some(server) = self.spool.waiting.remove(&id) {
                    self.spool_sign_ins.push(server);
                }
            }
            "connected" | "failed" => {
                self.spool.waiting.remove(&id);
                // Spools and an agent's servers show it as it is now.
                self.mcp_stale = true;
            }
            _ => {}
        }
    }

    /// Each spool card in a message the instance sent.
    pub(super) fn spool_cards_in(&mut self, message: &Value) {
        if let Some(blocks) = message.get("blocks").and_then(Value::as_array) {
            for b in blocks {
                if let Some(c) = b.get("card") {
                    self.spool_seen(c);
                }
            }
        }
    }

    /// Whether Sign in on card `card` is on its way, and what the instance said to the
    /// last one it refused.
    pub(super) fn spool_sending(&self, card: &str) -> (bool, String) {
        (
            self.spool.sending.contains(card),
            self.spool.errors.get(card).cloned().unwrap_or_default(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Card;

    fn card(state: &str) -> Value {
        json!({"id": "spool_tc_9", "type": "spool_connect", "title": "Connect T3 Code", "call": "tc_9",
               "entry": "t3", "name": "T3 Code", "server": "t3-vps", "address": "https://vps.example.com:8444/mcp",
               "agents": [{"id": "a_1", "name": "Casper"}], "secrets": [], "state": state, "said": "", "tools": ["a", "b"]})
    }

    #[test]
    fn reads_the_card() {
        let c = Card::of(&card("asking")).unwrap();
        let CardBody::SpoolConnect {
            name,
            server,
            agents,
            tools,
            ..
        } = c.body
        else {
            panic!("a spool card");
        };
        assert_eq!(
            (name.as_str(), server.as_str(), agents, tools.len()),
            ("T3 Code", "t3-vps", vec!["Casper".to_string()], 2)
        );
    }

    #[test]
    fn says_how_it_stands() {
        let agents = vec!["Casper".to_string()];
        assert_eq!(
            foot("asking", &agents, "", 0, ""),
            Foot::For("For Casper".into())
        );
        assert_eq!(foot("connecting", &agents, "", 0, ""), Foot::Connecting);
        assert_eq!(foot("sign_in", &agents, "", 0, ""), Foot::SigningIn);
        assert_eq!(
            foot("failed", &agents, "The address didn't answer.", 0, ""),
            Foot::Failed("The address didn't answer.".into())
        );
        assert_eq!(foot("connected", &agents, "", 12, ""), Foot::Connected(12));
        assert_eq!(connected_line(12), "Connected · 12 tools");
        assert_eq!(
            short_address("https://vps.example.com:8444/mcp"),
            "vps.example.com:8444"
        );
    }

    #[test]
    fn opens_the_sign_in_once_the_card_waits_for_it() {
        let mut m = Model::default();
        let f = m.spool_sign_in("msg_1", "spool_tc_9", "t3-vps", &[]);
        assert_eq!(f["name"], "spool.connect");
        assert_eq!(f["args"]["card"], "spool_tc_9");
        assert!(m.spool_sending("spool_tc_9").0);
        m.take(&json!({"t": "result", "id": f["id"], "ok": true, "value": {"connect": {"step": "adding"}}}));
        assert!(m.spool_sign_ins.is_empty());
        m.take(&json!({"t": "card", "run": "run_1", "message": "msg_1", "block": 0, "card": card("connecting")}));
        assert!(m.spool_sign_ins.is_empty());
        m.take(&json!({"t": "event", "seq": 1, "kind": "message.updated", "conversation": "c_1",
                       "message": {"id": "msg_1", "conversation": "c_1", "role": "agent", "at": 1, "blocks": [{"kind": "card", "card": card("sign_in")}]}}));
        assert_eq!(m.spool_sign_ins, vec!["t3-vps".to_string()]);
        m.take(&json!({"t": "card", "run": "run_1", "message": "msg_1", "block": 0, "card": card("sign_in")}));
        assert_eq!(m.spool_sign_ins.len(), 1);
    }

    #[test]
    fn a_refusal_shows_on_the_card() {
        let mut m = Model::default();
        let f = m.spool_sign_in("msg_1", "spool_tc_9", "t3-vps", &[]);
        m.take(&json!({"t": "result", "id": f["id"], "ok": false, "error": {"code": "bad_request", "message": "Fill in Token first."}}));
        assert_eq!(
            m.spool_sending("spool_tc_9"),
            (false, "Fill in Token first.".to_string())
        );
    }
}
