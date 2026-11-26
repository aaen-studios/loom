//! What the chat shows of its session's context, as Settings → Chat → Show context has
//! it (PLAN.md, "One endless chat"). Off, as it is until the person turns it on, nothing
//! does: no ring in the header, so no context card, and no line where a session was
//! compacted or started fresh, a group chat member's included. On, all of it.

use super::{ContextFill, Message, Model};

/// The marks of a `loom` line that are about a session's context, which show only while
/// Show context is on. A new one goes here.
pub const MARKS: [&str; 2] = ["compacted", "fresh"];

/// Whether `m` is a line across the chat about a session's context.
pub fn is_line(m: &Message) -> bool {
    m.role == "loom"
        && m.blocks
            .iter()
            .any(|b| b.kind == "mark" && MARKS.contains(&b.mark.as_str()))
}

impl Model {
    /// The open chat's context, for the header's ring and its card, while they show.
    pub fn context_shown(&self, show: bool) -> Option<ContextFill> {
        self.context().filter(|_| show)
    }

    /// The chat's messages as it draws them (`shown`), without the lines about its
    /// session's context unless they show.
    pub fn shown_with(&self, show_context: bool) -> Vec<Message> {
        let mut out = self.shown();
        if !show_context {
            out.retain(|m| !is_line(m));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::Prefs;
    use serde_json::{json, Value};

    /// Ledger's chat, its context 82k of 200k, with a compaction, a fresh session, a T3
    /// thread's line and a reply in it.
    fn chat() -> Model {
        let mut m = Model::new();
        m.take(&json!({"t": "core.data", "what": "agents", "items": [
            {"id": "a_1", "name": "Ledger", "acp_agent": "claude"},
        ]}));
        m.take(&json!({"t": "core.data", "what": "conversations", "items": [
            {"id": "c_1", "agent": "a_1", "at": 1, "context": {"used": 82_000, "size": 200_000, "compacts": true}},
        ]}));
        m.select(Some("a_1".into()));
        let line = |id: &str, at: i64, mark: Value| json!({"id": id, "role": "loom", "at": at, "blocks": [mark]});
        let kept = [
            line(
                "m_1",
                1,
                json!({"kind": "mark", "mark": "compacted", "before": 182_000, "after": 16_000}),
            ),
            line("m_2", 2, json!({"kind": "mark", "mark": "fresh"})),
            line(
                "m_3",
                3,
                json!({"kind": "mark", "mark": "t3_thread", "thread": "th_1"}),
            ),
            json!({"id": "m_4", "role": "agent", "agent": "a_1", "at": 4, "blocks": [{"kind": "text", "text": "Done."}]}),
            // A group chat member's.
            line(
                "m_5",
                5,
                json!({"kind": "mark", "mark": "compacted", "agent": "a_1", "before": 120_000, "after": 76_000}),
            ),
            line(
                "m_6",
                6,
                json!({"kind": "mark", "mark": "fresh", "agent": "a_1"}),
            ),
        ];
        m.messages = kept.iter().map(|v| m.message_of(v)).collect();
        m
    }

    fn ids(shown: Vec<Message>) -> Vec<String> {
        shown.into_iter().map(|m| m.id).collect()
    }

    #[test]
    fn context_is_hidden_until_the_person_shows_it() {
        let show = Prefs::default().show_context;
        assert!(!show);
        let m = chat();
        // No ring, so no card, and no line about the context, a member's included.
        assert_eq!(m.context_shown(show), None);
        assert_eq!(ids(m.shown_with(show)), ["m_3", "m_4"]);
    }

    #[test]
    fn shown_the_ring_and_every_line_are_there() {
        let m = chat();
        let fill = m.context_shown(true).expect("its program said");
        assert_eq!(fill.amount(), "82k of 200k");
        assert_eq!(
            ids(m.shown_with(true)),
            ["m_1", "m_2", "m_3", "m_4", "m_5", "m_6"]
        );
        assert_eq!(m.shown_with(true), m.shown());
        assert_eq!(
            m.shown_with(true)[4].blocks[0].text,
            "Ledger's context compacted · 120k → 76k"
        );
    }

    #[test]
    fn only_the_lines_about_context_are_its_lines() {
        let m = chat();
        let lines: Vec<bool> = m.messages.iter().map(is_line).collect();
        assert_eq!(lines, [true, true, false, false, true, true]);
        // A T3 thread's line and a watch's are no session's.
        let update = m.message_of(&json!({"id": "m_7", "role": "loom", "at": 7, "blocks": [
            {"kind": "update", "integration": "acta", "card": "LOO-2", "text": "LOO-2 moved to Done"}
        ]}));
        assert!(!is_line(&update));
        // Nor is an agent's reply that says the words.
        let reply = m.message_of(
            &json!({"id": "m_8", "role": "agent", "agent": "a_1", "at": 8, "blocks": [
                {"kind": "text", "text": "Context compacted"}
            ]}),
        );
        assert!(!is_line(&reply));
    }
}
