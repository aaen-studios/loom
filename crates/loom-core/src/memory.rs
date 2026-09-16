//! Long-term memory: durable facts about the user (global) and the current
//! workspace.
//!
//! Rows live in `memories` (see `db::Memory`). Writes come from the model's
//! `remember_fact` tool, the user, or the per-reply extraction pass; reads are
//! a pinned block in the system prompt plus top-N embedding matches for the
//! current message. This module holds the pure parts: scoring, prompt blocks,
//! extraction parsing.

use serde::Deserialize;

use crate::db::{now_ms, Memory};
use crate::index::cosine;

/// The scope string used for facts that follow the user everywhere.
pub const GLOBAL: &str = "global";

/// A write this similar to an existing memory replaces it instead of adding.
pub const NEAR_DUPLICATE: f32 = 0.9;

/// Retrieval floor: below this the memory is not worth injecting.
pub const RETRIEVAL_FLOOR: f32 = 0.35;

/// How many recalled memories ride along with a turn.
pub const RECALL_LIMIT: usize = 6;

/// Cap on the pinned block, in characters (roughly a thousand tokens).
pub const PINNED_CHARS: usize = 4_000;

/// Facts longer than this are truncated before they are stored.
pub const MAX_FACT_CHARS: usize = 2_048;

/// The scope path for a chat, given its workspace folder.
pub fn scope_for_workdir(workdir: Option<&str>) -> Option<String> {
    workdir
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(|path| path.replace('\\', "/"))
}

/// Longest matching workspace scope: `global` always applies, plus the chat's
/// own workspace when it has one.
pub fn scopes_for(workdir: Option<&str>) -> Vec<String> {
    let mut scopes = vec![GLOBAL.to_string()];
    if let Some(scope) = scope_for_workdir(workdir) {
        scopes.push(scope);
    }
    scopes
}

pub fn new_memory(
    scope: &str,
    content: &str,
    pinned: bool,
    source: &str,
    source_session: Option<&str>,
    source_message: Option<&str>,
    embedding: Option<Vec<u8>>,
) -> Memory {
    let now = now_ms();
    Memory {
        id: uuid::Uuid::new_v4().to_string(),
        scope: scope.to_string(),
        content: content.trim().chars().take(MAX_FACT_CHARS).collect(),
        embedding,
        pinned,
        source_session: source_session.map(str::to_string),
        source_message: source_message.map(str::to_string),
        source: source.to_string(),
        created_at: now,
        updated_at: now,
    }
}

/// Scored memories, best first, filtered by [`RETRIEVAL_FLOOR`].
pub fn rank<'a>(memories: &'a [Memory], query: &[f32], limit: usize) -> Vec<(&'a Memory, f32)> {
    let mut scored: Vec<(&Memory, f32)> = memories
        .iter()
        .filter_map(|memory| {
            let vector = crate::embeddings::decode(memory.embedding.as_deref()?);
            let score = cosine(&vector, query);
            (score >= RETRIEVAL_FLOOR).then_some((memory, score))
        })
        .collect();
    scored.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(limit);
    scored
}

/// The index of an existing memory that is near-identical to `embedding`.
pub fn near_duplicate(existing: &[Memory], embedding: &[f32]) -> Option<usize> {
    existing
        .iter()
        .enumerate()
        .filter_map(|(index, memory)| {
            let vector = crate::embeddings::decode(memory.embedding.as_deref()?);
            Some((index, cosine(&vector, embedding)))
        })
        .filter(|(_, score)| *score >= NEAR_DUPLICATE)
        .max_by(|left, right| left.1.partial_cmp(&right.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(index, _)| index)
}

/// The block injected into the system prompt: pinned facts first, then the
/// retrieved ones. `None` when there is nothing to say.
pub fn prompt_block(pinned: &[Memory], recalled: &[Memory]) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut used = 0usize;

    for memory in pinned {
        let line = format!("- {}", memory.content.trim());
        if used + line.chars().count() > PINNED_CHARS {
            break;
        }
        used += line.chars().count() + 1;
        lines.push(format!("[pinned] {line}"));
    }
    for memory in recalled {
        if pinned.iter().any(|p| p.id == memory.id) {
            continue;
        }
        let line = format!("- {} ({})", memory.content.trim(), scope_label(&memory.scope));
        if used + line.chars().count() > PINNED_CHARS {
            break;
        }
        used += line.chars().count() + 1;
        lines.push(line);
    }

    if lines.is_empty() {
        return None;
    }

    Some(format!(
        "Long-term memory. These are durable facts saved from earlier conversations, in \
         no particular order. Treat them as background knowledge — the user's explicit \
         instructions, the persona, and project files always win if they disagree. If the \
         user corrects a memory, save the correction with `remember_fact`.\n{}",
        lines.join("\n")
    ))
}

fn scope_label(scope: &str) -> &str {
    if scope == GLOBAL {
        "about you"
    } else {
        "this project"
    }
}

/// What the extraction pass is asked to produce.
pub fn extraction_prompt(new_messages: &str, existing: &[Memory]) -> String {
    let known = existing
        .iter()
        .take(40)
        .map(|memory| format!("- [{}] {}", memory.scope, memory.content))
        .collect::<Vec<_>>()
        .join("\n");
    let known = if known.is_empty() {
        "(nothing yet)".to_string()
    } else {
        known
    };

    format!(
        "You maintain the long-term memory of an AI assistant. Read the conversation \
         excerpt below and propose durable facts worth remembering in future \
         conversations.\n\nRules:\n\
         - Only facts that are durable and useful later: who the user is, preferences, \
           standing instructions, decisions, ongoing projects, project conventions and \
           gotchas. Never save one-off requests, transient state, secrets, API keys, or \
           anything from the assistant's own speculation.\n\
         - `scope` is \"global\" for facts about the user, or \"workspace\" for facts about \
           the current project.\n\
         - If a fact updates or contradicts something already known, propose the corrected \
           version (it replaces the old one).\n\
         - At most 3 facts. Quality over quantity. If nothing qualifies, reply with [].\n\
         - Reply with only a JSON array, no prose:\n\
           [{{\"scope\": \"global\"|\"workspace\", \"fact\": \"...\"}}]\n\n\
         Already known:\n{known}\n\nConversation excerpt:\n{new_messages}"
    )
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ProposedFact {
    #[serde(default)]
    pub scope: String,
    #[serde(default, alias = "content", alias = "value")]
    pub fact: String,
}

/// Pulls the JSON array out of a model reply, tolerating prose around it and
/// code fences. Unparseable items are skipped rather than failing the pass.
pub fn parse_extraction(raw: &str) -> Vec<ProposedFact> {
    let start = raw.find('[');
    let end = raw.rfind(']');
    let (Some(start), Some(end)) = (start, end) else {
        return Vec::new();
    };
    if end <= start {
        return Vec::new();
    }

    let mut facts: Vec<ProposedFact> = serde_json::from_str(&raw[start..=end]).unwrap_or_default();
    facts.retain(|fact| {
        let text = fact.fact.trim();
        !text.is_empty() && text.chars().count() >= 4
    });
    facts.truncate(3);
    facts
}

/// Where a proposed fact lands: `global`, or the chat's workspace scope.
pub fn scope_for_fact(proposed: &str, workdir: Option<&str>) -> String {
    if proposed.eq_ignore_ascii_case("global") || workdir.is_none() {
        GLOBAL.to_string()
    } else {
        scope_for_workdir(workdir).unwrap_or_else(|| GLOBAL.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(scope: &str, content: &str, vector: &[f32]) -> Memory {
        let mut memory = new_memory(scope, content, false, "user", None, None, None);
        memory.embedding = Some(crate::embeddings::encode(vector));
        memory
    }

    #[test]
    fn ranking_orders_by_similarity_and_filters_noise() {
        let memories = vec![
            memory(GLOBAL, "likes tabs", &[1.0, 0.0]),
            memory(GLOBAL, "lives in Berlin", &[0.0, 1.0]),
            memory(GLOBAL, "unrelated", &[0.5, 0.5]),
        ];
        let ranked = rank(&memories, &[1.0, 0.05], 2);
        assert_eq!(ranked[0].0.content, "likes tabs");
        assert!(ranked.len() <= 2);
    }

    #[test]
    fn near_duplicates_are_found() {
        let memories = vec![memory(GLOBAL, "likes tabs", &[1.0, 0.0])];
        assert!(near_duplicate(&memories, &[0.99, 0.01]).is_some());
        assert!(near_duplicate(&memories, &[0.0, 1.0]).is_none());
    }

    #[test]
    fn extraction_survives_prose_and_fences() {
        let raw = "Sure!\n```json\n[{\"scope\":\"global\",\"fact\":\"prefers dark mode\"},{\"scope\":\"workspace\",\"fact\":\"uses bun\"}]\n```";
        let facts = parse_extraction(raw);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].fact, "prefers dark mode");
        assert_eq!(parse_extraction("nothing").len(), 0);
    }
}
