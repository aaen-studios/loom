//! Keeps a request inside the model's context window.
//!
//! A message-count history limit cannot do this job: one `read_file` can be a
//! hundred thousand tokens, and a model with a million-token window should get
//! far more than forty messages. Instead the request budget is derived from
//! the model's context window, and the conversation is compressed to fit —
//! bulky tool outputs are elided first, then whole turns are dropped, and the
//! model is told when that happened. Nothing is deleted from the database;
//! this shapes the wire only.

use crate::attachments::{self, AttachmentKind};
use crate::catalog;
use crate::db::{Message, Role};
use crate::engine::{map_tool_outputs, parse_stored_tools};
use crate::provider::{ModelSpec, ProviderConfig};

/// Used when neither the provider nor the bundled catalog knows the model's
/// window.
pub const DEFAULT_CONTEXT: u32 = 128_000;

/// Output headroom assumed when the model's own limit is unknown.
const DEFAULT_OUTPUT: u32 = 8_192;

/// Share of the usable window a request may occupy. Tokenisers disagree with
/// the character estimate below, and the system prompt and tool schemas are
/// only approximated, so some headroom has to stay free.
const BUDGET_PERCENT: u32 = 85;

/// Token estimate for prose and code: roughly four characters per token.
const CHARS_PER_TOKEN: u32 = 4;

/// Flat cost for an inlined image. Provider tiling makes this approximate,
/// and a budget only needs to be close.
const IMAGE_TOKENS: u32 = 1_200;

/// Tool outputs above this are the first thing [`fit`] elides.
const ELIDE_ABOVE_CHARS: usize = 1_500;

/// Best metadata for a model: the provider's entry, else the bundled catalog,
/// else the empty fallback.
pub fn model_spec(provider: &ProviderConfig, model_id: &str) -> ModelSpec {
    provider
        .models
        .get(model_id)
        .cloned()
        .unwrap_or_else(|| catalog::lookup(model_id).unwrap_or_else(catalog::fallback))
}

/// The model's context window in tokens.
pub fn context_window(provider: &ProviderConfig, model_id: &str) -> u32 {
    model_spec(provider, model_id)
        .context
        .unwrap_or(DEFAULT_CONTEXT)
}

/// The `max_tokens` to send. Zero means "the model's own limit"; otherwise the
/// user's cap, never above what the model accepts.
pub fn output_limit(configured: u32, spec: &ModelSpec) -> Option<u32> {
    match (configured, spec.output) {
        (0, known) => known,
        (cap, Some(known)) => Some(cap.min(known)),
        (cap, None) => Some(cap),
    }
}

/// How many tokens the conversation may occupy, after reserving the reply and
/// the fixed per-request payload (system prompt, tool schemas).
pub fn input_budget(context: u32, max_output: Option<u32>, fixed: u32) -> u32 {
    let reserve = max_output.unwrap_or(DEFAULT_OUTPUT).min(context / 3);
    let usable = context.saturating_sub(reserve).saturating_sub(fixed);
    (usable as u64 * BUDGET_PERCENT as u64 / 100) as u32
}

/// Rough token count for one text blob.
pub fn tokens_for(text: &str) -> u32 {
    (text.chars().count() as u64 / CHARS_PER_TOKEN as u64) as u32 + 1
}

/// Fits the conversation into `budget` tokens by compressing and dropping the
/// oldest content. The last user turn and everything after it — what the
/// current request is working from — is kept whenever any part of it fits.
pub fn fit(history: &[Message], budget: u32) -> Vec<Message> {
    if history.is_empty() {
        return Vec::new();
    }

    let last_user = history
        .iter()
        .rposition(|message| message.role == Role::User)
        .unwrap_or(0);

    let mut kept = history.to_vec();
    let mut used = total_tokens(&kept, last_user).saturating_add(tool_image_cost(&kept, last_user));

    // 1. Elide bulky tool outputs in older turns, oldest first. The current
    //    turn keeps its fresh results for as long as possible.
    if used > budget {
        for index in 0..last_user {
            if used <= budget {
                break;
            }
            used = elide_at(&mut kept, index, false, used);
        }
    }

    // 2. Still too big: drop whole older turns, keeping the current one. A
    //    synthetic note tells the model that context is missing instead of
    //    leaving a silent gap in the transcript.
    let mut dropped = 0usize;
    if used > budget {
        let tail_cost: u32 = kept[last_user..]
            .iter()
            .enumerate()
            .map(|(offset, message)| message_tokens(message, offset == 0))
            .fold(0, u32::saturating_add);
        let mut prefix_cost: u32 = kept[..last_user]
            .iter()
            .map(|message| message_tokens(message, false))
            .fold(0, u32::saturating_add);

        while dropped < last_user && tail_cost.saturating_add(prefix_cost) > budget {
            prefix_cost = prefix_cost.saturating_sub(message_tokens(&kept[dropped], false));
            dropped += 1;
        }

        // Stop on a turn boundary so a dropped question never leaves its
        // answer behind on its own.
        while dropped < last_user && kept[dropped].role != Role::User {
            dropped += 1;
        }

        if dropped > 0 {
            kept.drain(0..dropped);
            used = tail_cost
                .saturating_add(
                    kept[..last_user - dropped]
                        .iter()
                        .map(|message| message_tokens(message, false))
                        .fold(0, u32::saturating_add),
                )
                .saturating_add(tool_image_cost(&kept, last_user - dropped));
        }
    }

    // 3. Even the current turn alone is over budget: elide its outputs too,
    //    so the provider does not reject the request outright.
    if used > budget {
        let boundary = last_user - dropped;
        for index in boundary + 1..kept.len() {
            if used <= budget {
                break;
            }
            used = elide_at(&mut kept, index, false, used);
        }
    }

    if dropped > 0 {
        kept.insert(0, omitted_note(dropped));
    }

    kept
}

fn total_tokens(history: &[Message], last_user: usize) -> u32 {
    history
        .iter()
        .enumerate()
        .map(|(index, message)| message_tokens(message, index == last_user))
        .fold(0, u32::saturating_add)
}

/// Token cost of the one tool result whose images will be inlined on the wire
/// (`build_wire` keeps only the newest image-bearing call). Without this the
/// budget cannot see computer screenshots at all, and a native-resolution
/// frame could push the request past the model's window unnoticed.
fn tool_image_cost(history: &[Message], last_user: usize) -> u32 {
    history
        .iter()
        .enumerate()
        .rev()
        .filter(|(index, _)| *index >= last_user)
        .flat_map(|(_, message)| parse_stored_tools(message.extra.as_deref()))
        .find(|call| !call.images.is_empty())
        .map(|call| IMAGE_TOKENS.saturating_mul(call.images.len() as u32))
        .unwrap_or(0)
}

/// Replaces one message's bulky tool outputs with a note, returning the new
/// running total.
fn elide_at(kept: &mut [Message], index: usize, include_images: bool, used: u32) -> u32 {
    let before = message_tokens(&kept[index], include_images);
    let elided = elide_outputs(&kept[index]);
    if elided.extra == kept[index].extra {
        return used;
    }
    let after = message_tokens(&elided, include_images);
    kept[index] = elided;
    used.saturating_sub(before.saturating_sub(after))
}

/// Token cost of one stored message as it would appear on the wire.
fn message_tokens(message: &Message, include_images: bool) -> u32 {
    let mut tokens = tokens_for(&message.content) + 4;

    if message.role == Role::Assistant {
        if let Some(reasoning) = message.reasoning.as_deref() {
            tokens = tokens.saturating_add(tokens_for(reasoning));
        }
        for call in parse_stored_tools(message.extra.as_deref()) {
            tokens = tokens
                .saturating_add(tokens_for(&call.name))
                .saturating_add(tokens_for(&call.arguments))
                .saturating_add(tokens_for(&call.output));
        }
    }

    for attachment in attachments::parse_extra(message.extra.as_deref()) {
        tokens = tokens.saturating_add(match attachment.kind {
            AttachmentKind::Image if include_images => IMAGE_TOKENS,
            AttachmentKind::Image => tokens_for(&attachment.name),
            // Text and PDFs are extracted on send and clamped by the
            // attachments module, so the file size is an upper bound.
            _ => tokens_for_size(attachment.size),
        });
    }

    tokens
}

fn tokens_for_size(bytes: u64) -> u32 {
    let chars = bytes.min(attachments::MAX_TEXT_CHARS as u64) as u32;
    chars / CHARS_PER_TOKEN + 1
}

fn elide_outputs(message: &Message) -> Message {
    map_tool_outputs(message, |output| {
        let chars = output.chars().count();
        if chars <= ELIDE_ABOVE_CHARS {
            return output.to_string();
        }
        format!("[tool output omitted to fit the model's context window: {chars} characters]")
    })
}

/// A synthetic user turn telling the model that older context was dropped.
fn omitted_note(count: usize) -> Message {
    Message {
        id: "context-omitted".to_string(),
        session_id: String::new(),
        role: Role::User,
        content: format!(
            "[{count} earlier messages were left out to fit the model's context window. \
             Ask the user if you need context from them.]"
        ),
        reasoning: None,
        extra: None,
        persona_id: None,
        created_at: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Modality, ModelSpec};

    fn message(role: Role, content: &str) -> Message {
        Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: "s".into(),
            role,
            content: content.into(),
            reasoning: None,
            extra: None,
            persona_id: None,
            created_at: 0,
        }
    }

    /// An assistant turn with one tool call whose output is `chars` long.
    fn tool_message(chars: usize) -> Message {
        let output = "x".repeat(chars);
        let extra = format!(
            r#"{{"toolCalls":[{{"id":"c1","name":"read_file","arguments":"{{}}","status":"ok","output":{}}}]}}"#,
            serde_json::to_string(&output).unwrap()
        );
        Message {
            extra: Some(extra),
            ..message(Role::Assistant, "reading")
        }
    }

    fn provider(context: Option<u32>, output: Option<u32>) -> ProviderConfig {
        let mut provider = ProviderConfig {
            name: "Test".into(),
            base_url: "https://example.com/v1".into(),
            ..Default::default()
        };
        provider.models.insert(
            "test-model".into(),
            ModelSpec {
                context,
                output,
                input_modalities: vec![Modality::Text],
                ..Default::default()
            },
        );
        provider
    }

    #[test]
    fn window_prefers_the_provider_then_the_catalog_then_a_default() {
        assert_eq!(
            context_window(&provider(Some(32_000), None), "test-model"),
            32_000
        );
        // No provider entry: the bundled catalog knows gpt-4o.
        assert_eq!(context_window(&provider(None, None), "gpt-4o"), 128_000);
        // Nothing knows this one.
        assert_eq!(
            context_window(&provider(None, None), "totally-custom-9000"),
            DEFAULT_CONTEXT
        );
    }

    #[test]
    fn zero_output_means_the_models_own_limit() {
        let spec = ModelSpec {
            output: Some(64_000),
            ..Default::default()
        };
        assert_eq!(output_limit(0, &spec), Some(64_000));
        // Unknown model: let the provider decide.
        assert_eq!(output_limit(0, &ModelSpec::default()), None);
        // An explicit cap is honoured, but never above what the model accepts.
        assert_eq!(output_limit(32_000, &spec), Some(32_000));
        assert_eq!(output_limit(200_000, &spec), Some(64_000));
        assert_eq!(output_limit(32_000, &ModelSpec::default()), Some(32_000));
    }

    #[test]
    fn budget_reserves_the_reply_and_the_fixed_payload() {
        // 85% of (128k - 8k output - 2k system/tools) = 100_136.
        assert_eq!(input_budget(128_000, Some(8_192), 2_000), 100_136);

        // A huge output limit cannot eat more than a third of the window.
        let budget = input_budget(32_000, Some(100_000), 0);
        assert_eq!(budget, (32_000 - 32_000 / 3) as u32 * 85 / 100);
    }

    #[test]
    fn small_histories_pass_through_untouched() {
        let history = vec![
            message(Role::User, "hello"),
            message(Role::Assistant, "hi"),
            message(Role::User, "write a file"),
        ];
        let fitted = fit(&history, 50_000);
        assert_eq!(fitted.len(), 3);
        assert_eq!(fitted[0].content, "hello");
        assert_eq!(fitted[2].content, "write a file");
    }

    #[test]
    fn bulky_old_outputs_are_elided_before_anything_is_dropped() {
        let history = vec![
            message(Role::User, "read the big file"),
            tool_message(400_000),
            message(Role::Assistant, "done"),
            message(Role::User, "now the small one"),
            message(Role::Assistant, "ok"),
        ];

        let fitted = fit(&history, 5_000);

        // No turn was dropped; the big output shrank to a note.
        assert_eq!(fitted.len(), history.len());
        let calls = parse_stored_tools(fitted[1].extra.as_deref());
        assert_eq!(calls.len(), 1);
        assert!(calls[0].output.contains("omitted"), "{}", calls[0].output);
        // And the conversation text survives.
        assert_eq!(fitted[3].content, "now the small one");
    }

    #[test]
    fn old_turns_are_dropped_with_a_note_when_eliding_is_not_enough() {
        let history = vec![
            message(Role::User, &"a".repeat(40_000)),
            message(Role::Assistant, "first answer"),
            message(Role::User, "second question"),
            message(Role::Assistant, "second answer"),
            message(Role::User, "current question"),
            message(Role::Assistant, "working"),
        ];

        let fitted = fit(&history, 1_000);

        // The note stands in for the dropped exchange, and the current turn is
        // intact.
        assert!(fitted[0].content.contains("earlier messages"));
        assert!(fitted.iter().any(|m| m.content == "current question"));
        assert!(!fitted.iter().any(|m| m.content == "first answer"));
    }

    #[test]
    fn an_oversized_current_turn_is_elided_rather_than_rejected() {
        let history = vec![
            message(Role::User, "read the enormous log"),
            tool_message(200_000),
        ];

        let fitted = fit(&history, 500);

        let calls = parse_stored_tools(fitted[1].extra.as_deref());
        assert!(calls[0].output.contains("omitted"), "{}", calls[0].output);
        assert_eq!(fitted[0].content, "read the enormous log");
    }

    #[test]
    fn tool_pairing_keeps_its_shape_when_compressed() {
        let history = vec![
            message(Role::User, "go"),
            tool_message(100_000),
            tool_message(100_000),
        ];
        let fitted = fit(&history, 1_000);
        // Every remaining assistant turn still carries its calls, so the wire
        // never has a tool result without a preceding call.
        for message in &fitted {
            if message.role == Role::Assistant {
                assert!(!parse_stored_tools(message.extra.as_deref()).is_empty());
            }
        }
    }
}
