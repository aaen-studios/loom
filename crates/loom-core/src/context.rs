//! Keeps a request inside the model's context window.
//!
//! A message-count history limit cannot do this job: one `read_file` can be a
//! hundred thousand tokens, and a model with a million-token window should get
//! far more than forty messages. Instead the request budget is derived from
//! the model's context window, and the conversation is fitted to it — bulky
//! tool outputs are elided first, then older turns are folded into one
//! condensed block ([`crate::condense`]) covering the messages they stood in
//! for, and elision of the current turn is the last resort. Nothing is deleted
//! from the database; this shapes the wire only.
//!
//! The budget is divided tail first: the last user turn and everything after it
//! take what they need verbatim, and the condensed block gets the remainder,
//! capped at [`crate::condense::condensed_budget`]. That ordering is the point —
//! a generous summary must never be able to elide the fresh tool output the
//! live turn depends on.
//!
//! Two rules keep the arithmetic honest, because the failure this module
//! exists to prevent is a rejected request, not a degraded one. First, the
//! reply headroom [`input_budget`] reserves is *exactly* the completion budget
//! the adapters declare: [`output_limit`] is the single source of that number,
//! and reserving less than the request declares borrows from the safety margin
//! silently. Second, [`Calibration`] corrects the character-based estimate
//! from the token counts providers report, so the fit converges on models whose
//! tokeniser disagrees with four characters per token instead of failing.

use serde::{Deserialize, Serialize};

use crate::attachments::{self, AttachmentKind};
use crate::catalog;
use crate::condense;
use crate::db::{Message, Role, SessionSummary, Todo};
use crate::engine::{clear_tool_images, map_tool_outputs, parse_stored_tools, reasoning_echo};
use crate::provider::{ModelSpec, ProviderConfig};

/// Used when neither the provider nor the bundled catalog knows the model's
/// window.
pub const DEFAULT_CONTEXT: u32 = 128_000;

/// Reply headroom assumed when the model's own limit is unknown. Also the cap
/// every adapter falls back to, so no request ever goes out with an undeclared
/// completion budget: a gateway left to choose one reserves far more of the
/// window than Loom does, and rejects the request.
pub const DEFAULT_OUTPUT_TOKENS: u32 = 8_192;

/// Smallest reply budget worth declaring.
const MIN_OUTPUT_TOKENS: u32 = 256;

/// A reply never reserves more than this fraction of the window. Asking for
/// half a 200k window as output is a request no provider will accept.
const OUTPUT_SHARE: u32 = 4;

/// Share of the usable window a request may occupy. Tokenisers disagree with
/// the character estimate below, and the system prompt and tool schemas are
/// only approximated, so some headroom has to stay free.
const BUDGET_PERCENT: u32 = 85;

/// Token estimate for prose and code: roughly four characters per token.
const CHARS_PER_TOKEN: u32 = 4;

/// The same ratio, for `condense` to size a character budget with. A summary
/// and the fit have to agree about what fits, or the block that was written to
/// the budget would not have been.
pub(crate) const CHARS_PER_TOKEN_FOR_CONDENSE: u32 = CHARS_PER_TOKEN;

/// JSON punctuates far more heavily than prose, and tool schemas and call
/// arguments are JSON. Three characters per token is the honest estimate;
/// using the prose figure under-counts every request that offers tools.
const JSON_CHARS_PER_TOKEN: u32 = 3;

/// A tool output above this is the first thing [`fit_report`] elides.
const ELIDE_ABOVE_CHARS: usize = 1_500;

// Image cost. Providers tile an image and bill per tile, so cost follows
// pixels rather than bytes: a 4K PNG of an empty desktop is enormous on disk
// and cheap in reality, while a photograph of the same size is the reverse.
// These follow the tiling providers document.
const IMAGE_TILE_PX: u32 = 512;
const IMAGE_TOKENS_PER_TILE: u32 = 256;
const IMAGE_OVERHEAD: u32 = 85;
const MIN_IMAGE_TOKENS: u32 = 200;
const MAX_IMAGE_TOKENS: u32 = 20_000;
/// Dimensions unknown: an older record written before they were stored, or a
/// format whose header could not be read.
const UNKNOWN_IMAGE_TOKENS: u32 = 1_200;

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

/// The completion budget for a turn — and therefore the number the adapters
/// declare and [`input_budget`] reserves.
///
/// Zero `configured` means "the model's own limit". When nothing knows that
/// limit, the flat default is used rather than a share of the window: sending
/// a `max_tokens` above a model's real limit is rejected outright, which is
/// the very failure this module exists to prevent.
pub fn output_limit(configured: u32, spec: &ModelSpec, context: u32) -> u32 {
    let ceiling = (context / OUTPUT_SHARE).max(MIN_OUTPUT_TOKENS);
    let requested = match (configured, spec.output) {
        (0, Some(known)) => known,
        (0, None) => DEFAULT_OUTPUT_TOKENS,
        (cap, Some(known)) => cap.min(known),
        (cap, None) => cap,
    };
    requested.clamp(MIN_OUTPUT_TOKENS, ceiling)
}

/// How many tokens the conversation may occupy, after reserving the reply
/// (`declared_output`, which must be the `max_tokens` actually sent) and the
/// fixed per-request payload (system prompt, tool schemas).
pub fn input_budget(context: u32, declared_output: u32, fixed: u32) -> u32 {
    let usable = context
        .saturating_sub(declared_output)
        .saturating_sub(fixed);
    (usable as u64 * BUDGET_PERCENT as u64 / 100) as u32
}

/// The budget for one attempt, tightened by the observed estimate error.
pub fn scaled_budget(budget: u32, ratio: f32) -> u32 {
    let ratio = ratio.clamp(1.0, MAX_RATIO) as f64;
    if !ratio.is_finite() || ratio <= 1.0 {
        return budget;
    }
    (budget as f64 / ratio) as u32
}

/// The budget for the single retry after a provider rejected a request for
/// length: half of what was just tried, but never more than the window
/// honestly leaves once the reply and the fixed payload are set aside.
pub fn retry_budget(context: u32, declared_output: u32, fixed: u32, attempted: u32) -> u32 {
    let ceiling = context
        .saturating_sub(declared_output)
        .saturating_sub(fixed);
    (attempted / 2).min(ceiling)
}

/// The largest correction [`Calibration`] will apply.
const MAX_RATIO: f32 = 2.0;

/// How much of each new sample the running ratio takes. Slow enough that one
/// odd round (a provider counting a cached prefix differently) does not swing
/// the budget, fast enough to converge within a couple of tool rounds.
const RATIO_BLEND: f32 = 0.4;

/// Tracks how far the character-based estimate runs below what a provider
/// actually charged, per model.
///
/// The estimate is a heuristic, and heuristics are wrong by different amounts
/// for English prose, stack traces and JSON. Providers report the real count
/// on every round, so the correction is measured rather than guessed. It is
/// one-sided on purpose: the ratio never drops below 1, so an over-estimate
/// can never talk the fit into keeping more history than it should.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calibration {
    ratio: f32,
}

impl Default for Calibration {
    fn default() -> Self {
        Self { ratio: 1.0 }
    }
}

impl Calibration {
    pub fn new() -> Self {
        Self::default()
    }

    /// The current correction factor, always at least 1.
    pub fn ratio(&self) -> f32 {
        self.ratio
    }

    /// Folds one round's reported cost against what was predicted for it.
    /// Ignores nonsense (a zero prediction, a report below the estimate).
    pub fn observe(&mut self, reported: u32, predicted: u32) {
        if predicted == 0 || reported == 0 {
            return;
        }
        let sample = (reported as f32 / predicted as f32).clamp(1.0, MAX_RATIO);
        self.ratio = (self.ratio * (1.0 - RATIO_BLEND) + sample * RATIO_BLEND)
            .clamp(1.0, MAX_RATIO);
    }
}

/// Whether a provider's rejection means "this request is too long".
///
/// Matched on the message because that is all the adapters keep — the status
/// code is folded into the text by `error_message`. The strings are the ones
/// the gateways in use actually send; the fixture from a real failure is in
/// the tests.
pub fn is_context_length_error(message: &str) -> bool {
    let haystack = message.to_ascii_lowercase();
    const PATTERNS: &[&str] = &[
        "maximum context length",
        "context length exceeded",
        "context_length_exceeded",
        "reduce the length of the messages",
        "reduce the length of your messages",
        "prompt is too long",
        "input is too long",
        "too many tokens",
        "exceeds the maximum number of tokens",
        "context window",
        "max_tokens is too large",
        "maximum number of input tokens",
    ];
    PATTERNS.iter().any(|pattern| haystack.contains(pattern))
}

/// Rough token count for one blob of prose or code.
pub fn tokens_for(text: &str) -> u32 {
    (text.chars().count() as u64 / CHARS_PER_TOKEN as u64) as u32 + 1
}

/// Rough token count for one blob of JSON (tool schemas, tool arguments).
pub fn tokens_for_json(text: &str) -> u32 {
    (text.chars().count() as u64 / JSON_CHARS_PER_TOKEN as u64) as u32 + 1
}

/// Token cost of one image of these dimensions. Zero dimensions mean unknown.
pub fn image_tokens(width: u32, height: u32) -> u32 {
    if width == 0 || height == 0 {
        return UNKNOWN_IMAGE_TOKENS;
    }
    let columns = (width + IMAGE_TILE_PX - 1) / IMAGE_TILE_PX;
    let rows = (height + IMAGE_TILE_PX - 1) / IMAGE_TILE_PX;
    let tiles = columns.saturating_mul(rows);
    IMAGE_OVERHEAD
        .saturating_add(tiles.saturating_mul(IMAGE_TOKENS_PER_TILE))
        .clamp(MIN_IMAGE_TOKENS, MAX_IMAGE_TOKENS)
}

/// A fitted conversation and what it costs.
#[derive(Debug, Clone)]
pub struct Fitted {
    pub messages: Vec<Message>,
    /// Estimated tokens for the fitted history alone. The provider charges for
    /// the system prompt and tool schemas too, so a caller comparing this with
    /// a reported count must add those first.
    pub estimate: u32,
    /// True when anything was elided or stripped of its images. Condensing is
    /// reported separately, in `condensed`.
    pub trimmed: bool,
    /// Set when older turns were folded into a condensed block. This is the
    /// one kind of trimming worth telling the user about: eliding a bulky tool
    /// output is invisible, but a reply answered from a summary should say so.
    pub condensed: Option<Condensed>,
}

/// How much of the history was folded away, and what the block was built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Condensed {
    /// How many messages the block stands in for.
    pub covered: usize,
    pub source: Source,
    /// Estimated tokens the block cost, for the line under the reply.
    pub tokens: u32,
}

/// Where a condensed block came from — the in-process digest, or a summary
/// written by the lite model in the background.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Digest,
    Summary,
}

/// What the fit needs beyond the history itself in order to fold older turns:
/// the window that sizes the block, and the stored summary if there is one.
#[derive(Default)]
pub struct Folding<'a> {
    /// The model's context window. Zero disables nothing but sizes the block
    /// from the floor alone.
    pub window: u32,
    /// The chat's stored summary, when the background pass has written one.
    pub summary: Option<&'a SessionSummary>,
    /// The chat's standing goal, so a fresh digest can name it.
    pub goal: Option<&'a str>,
    /// The live task list, likewise.
    pub todos: &'a [Todo],
    /// `chat.condenseShare`: how much of the window the block may take. `None`
    /// uses the default, and `Some(0)` switches condensing off, leaving older
    /// turns to be dropped as they were before.
    pub share: Option<u32>,
}

impl Folding<'_> {
    /// No summary and no goal: a fresh digest is the only block available.
    pub fn none() -> Self {
        Self::default()
    }

    /// The share to size the block with, defaulted and bounded.
    fn share(&self) -> u32 {
        self.share
            .unwrap_or(crate::condense::CONDENSED_SHARE)
            .min(crate::condense::MAX_CONDENSED_SHARE)
    }
}

/// Fits the conversation into `budget` tokens. The last user turn and
/// everything after it — what the current request is working from — is kept
/// whenever any part of it fits; older turns are folded into one condensed
/// block, capped so they can never squeeze the live turn out.
pub fn fit(history: &[Message], budget: u32) -> Vec<Message> {
    fit_report(history, budget, true, Folding::none()).messages
}

/// [`fit`], reporting the estimate and what was folded, and able to drop the
/// inlined screenshot up front when the caller has already run out of room
/// (the retry after a length rejection).
pub fn fit_report(
    history: &[Message],
    budget: u32,
    allow_images: bool,
    folding: Folding<'_>,
) -> Fitted {
    if history.is_empty() {
        return Fitted {
            messages: Vec::new(),
            estimate: 0,
            trimmed: false,
            condensed: None,
        };
    }

    let mut kept = history.to_vec();
    let mut trimmed = false;

    // 0. Asked to, drop the newest screenshot to its text breadcrumb before
    //    anything else. One 4K frame is worth tens of thousands of tokens and
    //    no amount of eliding tool *text* can offset it.
    if !allow_images {
        trimmed |= strip_newest_images(&mut kept);
    }

    let mut last_user = last_user_index(&kept);
    let mut used = cost(&kept, last_user);

    // 1. Elide bulky tool outputs in older turns, oldest first. The current
    //    turn keeps its fresh results for as long as possible. Nothing is
    //    reported for this: it was always invisible, and it is far cheaper
    //    than summarising a turn whose output nobody needs in full.
    if used > budget {
        for index in 0..last_user {
            if used <= budget {
                break;
            }
            let (next, changed) = elide_at(&mut kept, index, false, used);
            used = next;
            trimmed |= changed;
        }
    }

    // 2. Still too big: fold the older turns into one block and keep the
    //    current turn verbatim. This is the step that used to drop them
    //    outright and leave a note saying context was missing.
    let mut condensed = None;
    if used > budget {
        if folding.share() == 0 {
            // Condensing switched off: fall back to dropping whole older turns,
            // which is what the fit did before the block existed. No notice is
            // reported for it either way — a request that was trimmed is still
            // a request that succeeded.
            if let Some(dropped) = drop_older_turns(&mut kept, budget, last_user) {
                last_user = last_user_index(&kept);
                used = cost(&kept, last_user);
                trimmed = true;
                let _ = dropped;
            }
        } else if let Some((cover, block, source)) = fold(&kept, budget, last_user, &folding) {
            kept.drain(0..cover);
            kept.insert(0, block);
            last_user = last_user_index(&kept);
            let tokens = message_tokens(&kept[0], 0, &kept, false);
            used = cost(&kept, last_user);
            condensed = Some(Condensed {
                covered: cover,
                source,
                tokens,
            });
            trimmed = true;
        }
    }

    // 3. Even the current turn alone is over budget: elide its outputs too, so
    //    the provider does not reject the request outright. Tool text first,
    //    then the pixels — the last thing that can go.
    if used > budget {
        for index in last_user + 1..kept.len() {
            if used <= budget {
                break;
            }
            let (next, changed) = elide_at(&mut kept, index, false, used);
            used = next;
            trimmed |= changed;
        }
    }
    if used > budget && strip_newest_images(&mut kept) {
        trimmed = true;
    }

    let estimate = cost(&kept, last_user_index(&kept));
    Fitted {
        messages: kept,
        estimate,
        trimmed,
        condensed,
    }
}

/// The condensed block for this fit: how many messages it covers, the message
/// that carries it, and where its text came from.
///
/// The budget is divided **tail first**. The verbatim tail takes what it needs,
/// and the block gets the remainder, capped at its own ceiling — so a fat
/// summary can never elide the fresh tool output the live turn depends on,
/// which is the failure the whole arrangement exists to remove. When even that
/// will not fit, the fold advances one turn boundary at a time until it does,
/// and only stops when there is no turn left to fold.
fn fold(
    history: &[Message],
    budget: u32,
    last_user: usize,
    folding: &Folding<'_>,
) -> Option<(usize, Message, Source)> {
    // A stored summary fixes the fold point — it covers exactly the messages it
    // was written from, and folding on a boundary it knows about is what makes
    // the block coherent. It must leave the live turn alone, and the message it
    // reaches must still be in the history: an edit or a delete landing on it
    // invalidates the row, and a fresh digest stands in until it is rewritten.
    let from_summary = folding.summary.and_then(|summary| {
        history
            .iter()
            .position(|message| message.id == summary.covers_through_id)
            .filter(|index| *index < last_user)
            .map(|index| index + 1)
    });

    let mut cover = from_summary.unwrap_or(last_user);
    if cover == 0 {
        // Nothing older to fold: this is a first turn whose own output is too
        // large, and eliding it is the only honest move left.
        return None;
    }

    loop {
        let tail_cost: u32 = (cover..history.len())
            .map(|index| message_tokens(&history[index], index, history, index == last_user))
            .fold(0, u32::saturating_add);
        let room = condense::summary_room(folding.share(), folding.window, budget, tail_cost);

        let (text, source) = match (folding.summary, from_summary) {
            (Some(summary), Some(fixed)) if cover == fixed => (
                condense::clamp(&summary.text, room),
                Source::Summary,
            ),
            // Either there is no summary, or the fold has had to reach further
            // back than the one on file describes. Summarising the whole span
            // from the messages is better than keeping a summary of part of it.
            _ => (
                condense::digest_for(&history[..cover], folding.goal, folding.todos, room),
                Source::Digest,
            ),
        };
        let block = condensed_message(cover, &text);
        let block_cost = message_tokens(&block, 0, history, false);

        // The next boundary to try, if this one does not fit. Absent means the
        // tail is down to its last turn and there is nothing further to fold.
        let next = next_turn_start(history, cover);
        if tail_cost.saturating_add(block_cost) <= budget || next >= history.len() {
            return Some((cover, block, source));
        }
        cover = next;
    }
}

/// The synthetic turn that carries a condensed block.
///
/// A user message at the front of the wire, which is the position and role the
/// old "context was dropped" note occupied — so no provider's role-alternation
/// rules are disturbed by it — but saying what the block actually is rather
/// than what is missing.
fn condensed_message(covered: usize, text: &str) -> Message {
    Message {
        id: "context-condensed".to_string(),
        session_id: String::new(),
        role: Role::User,
        content: format!(
            "[{covered} earlier messages were condensed into the summary below to fit the \
             model's context window. It is a record of what happened in this conversation, \
             not a new request.]\n\n{text}"
        ),
        reasoning: None,
        extra: None,
        persona_id: None,
        created_at: 0,
    }
}

/// The next turn boundary after `from`: where a fold can advance to without
/// leaving an answer behind its question.
fn next_turn_start(history: &[Message], from: usize) -> usize {
    (from + 1..history.len())
        .find(|index| history[*index].role == Role::User)
        .unwrap_or(history.len())
}

/// Drops whole older turns from the front until the tail fits, stopping on a
/// turn boundary so a question never loses its answer.
///
/// Only reached when condensing is switched off. Returns how many messages went
/// and whether the request now fits; `None` when there was nothing to drop.
fn drop_older_turns(kept: &mut Vec<Message>, budget: u32, last_user: usize) -> Option<usize> {
    let tail_cost: u32 = (last_user..kept.len())
        .map(|index| message_tokens(&kept[index], index, kept, index == last_user))
        .fold(0, u32::saturating_add);

    let mut dropped = 0usize;
    let mut prefix_cost: u32 = (0..last_user)
        .map(|index| message_tokens(&kept[index], index, kept, false))
        .fold(0, u32::saturating_add);
    while dropped < last_user && tail_cost.saturating_add(prefix_cost) > budget {
        prefix_cost = prefix_cost.saturating_sub(message_tokens(&kept[dropped], dropped, kept, false));
        dropped += 1;
    }
    while dropped < last_user && kept[dropped].role != Role::User {
        dropped += 1;
    }
    if dropped == 0 {
        return None;
    }
    kept.drain(0..dropped);
    Some(dropped)
}

fn last_user_index(history: &[Message]) -> usize {
    history
        .iter()
        .rposition(|message| message.role == Role::User)
        .unwrap_or(0)
}

/// Estimated cost of a fitted conversation: its messages plus the pixels
/// `build_wire` will inline, which are not part of any message's text.
fn cost(history: &[Message], last_user: usize) -> u32 {
    total_tokens(history, last_user).saturating_add(tool_image_cost(history, last_user))
}

fn total_tokens(history: &[Message], last_user: usize) -> u32 {
    (0..history.len())
        .map(|index| message_tokens(&history[index], index, history, index == last_user))
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
        .map(|call| {
            call.images
                .iter()
                .map(|image| image_tokens(image.width, image.height))
                .fold(0, u32::saturating_add)
        })
        .unwrap_or(0)
}

/// Replaces the newest inlined screenshot with the text breadcrumb the wire
/// would otherwise carry. Returns whether anything changed.
fn strip_newest_images(history: &mut [Message]) -> bool {
    let Some(index) = history.iter().rposition(|message| {
        parse_stored_tools(message.extra.as_deref())
            .iter()
            .any(|call| !call.images.is_empty())
    }) else {
        return false;
    };
    let stripped = clear_tool_images(&history[index]);
    if stripped.extra == history[index].extra {
        return false;
    }
    history[index] = stripped;
    true
}

/// Replaces one message's bulky tool outputs with a note, returning the new
/// running total and whether anything changed.
fn elide_at(kept: &mut [Message], index: usize, include_images: bool, used: u32) -> (u32, bool) {
    let before = message_tokens(&kept[index], index, kept, include_images);
    let elided = elide_outputs(&kept[index]);
    if elided.extra == kept[index].extra {
        return (used, false);
    }
    let after = message_tokens(&elided, index, kept, include_images);
    kept[index] = elided;
    (used.saturating_sub(before.saturating_sub(after)), true)
}

/// Token cost of one stored message as it would appear on the wire.
fn message_tokens(
    message: &Message,
    index: usize,
    history: &[Message],
    include_images: bool,
) -> u32 {
    let mut tokens = tokens_for(&message.content) + 4;

    if message.role == Role::Assistant {
        // Only the thinking `build_wire` actually echoes back is charged for.
        // Counting the whole aggregate column over-estimated long reasoning
        // turns badly enough to drop history that would have fitted.
        if let Some(echo) = reasoning_echo(message, index, history) {
            tokens = tokens.saturating_add(tokens_for(&echo));
        }
        for call in parse_stored_tools(message.extra.as_deref()) {
            tokens = tokens
                .saturating_add(tokens_for(&call.name))
                .saturating_add(tokens_for_json(&call.arguments))
                .saturating_add(tokens_for(&call.output));
        }
    }

    for attachment in attachments::parse_extra(message.extra.as_deref()) {
        tokens = tokens.saturating_add(match attachment.kind {
            AttachmentKind::Image if include_images => {
                image_tokens(attachment.width, attachment.height)
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Modality, ModelSpec};
    use crate::tools::ToolImage;

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
        assert_eq!(output_limit(0, &spec, 1_000_000), 64_000);
        // An explicit cap is honoured, but never above what the model accepts.
        assert_eq!(output_limit(32_000, &spec, 1_000_000), 32_000);
        assert_eq!(output_limit(200_000, &spec, 1_000_000), 64_000);
        assert_eq!(output_limit(32_000, &ModelSpec::default(), 1_000_000), 32_000);
    }

    /// The number is always present and always inside the window. This is the
    /// invariant the old code broke: an unknown model reserved 8_192 while the
    /// wire declared nothing, so the gateway reserved 384_000 of a 1M window
    /// and rejected a request Loom believed had 375_000 tokens to spare.
    #[test]
    fn the_declared_reply_always_fits_the_window() {
        assert_eq!(
            output_limit(0, &ModelSpec::default(), 1_048_576),
            DEFAULT_OUTPUT_TOKENS
        );
        // A quarter of a small window, rather than a cap that cannot exist.
        assert_eq!(output_limit(0, &ModelSpec::default(), 8_192), 2_048);
        assert_eq!(output_limit(1_000_000, &ModelSpec::default(), 8_192), 2_048);
        // A huge catalogue limit cannot claim half the window either.
        let spec = ModelSpec {
            output: Some(1_000_000),
            ..Default::default()
        };
        assert_eq!(output_limit(0, &spec, 200_000), 50_000);

        for context in [4_096, 8_192, 32_000, 128_000, 200_000, 1_048_576] {
            for configured in [0, 1_024, 64_000, 200_000] {
                let declared = output_limit(configured, &ModelSpec::default(), context);
                assert!(declared >= MIN_OUTPUT_TOKENS, "{declared}");
                assert!(declared <= context / OUTPUT_SHARE, "{declared} vs {context}");
            }
        }
    }

    #[test]
    fn budget_reserves_the_reply_and_the_fixed_payload() {
        // 85% of (128k - 8k output - 2k system/tools) = 100_136.
        assert_eq!(input_budget(128_000, 8_192, 2_000), 100_136);

        // The reserve is what the request declares, so the pieces add up to no
        // more than the window — the property the gateway checks.
        for (context, configured) in [(128_000u32, 0u32), (200_000, 100_000), (1_048_576, 0)] {
            let spec = ModelSpec::default();
            let declared = output_limit(configured, &spec, context);
            let fixed = 12_000;
            let budget = input_budget(context, declared, fixed);
            assert!(
                budget + declared + fixed <= context,
                "{budget} + {declared} + {fixed} > {context}"
            );
        }
    }

    #[test]
    fn the_retry_budget_is_smaller_and_still_inside_the_window() {
        let context = 200_000;
        let declared = 16_000;
        let fixed = 3_000;
        let first = input_budget(context, declared, fixed);
        let second = retry_budget(context, declared, fixed, first);
        assert!(second < first, "{second} < {first}");
        assert!(second + declared + fixed <= context);

        // Metadata claiming a window wider than reality: the halved budget is
        // still capped by what the window honestly leaves.
        let cramped = retry_budget(8_000, 2_000, 1_000, 40_000);
        assert_eq!(cramped, 5_000);
    }

    #[test]
    fn calibration_learns_only_downward() {
        let mut calibration = Calibration::default();
        assert_eq!(calibration.ratio(), 1.0);
        // An estimate 60% low pulls the ratio up towards 1.6.
        calibration.observe(16_000, 10_000);
        assert!(calibration.ratio() > 1.0);
        assert!(calibration.ratio() <= 1.6);

        // Repeated evidence converges on the true error.
        for _ in 0..40 {
            calibration.observe(16_000, 10_000);
        }
        assert!((calibration.ratio() - 1.6).abs() < 0.01, "{}", calibration.ratio());

        // A provider that charges less than predicted never loosens it, and
        // nonsense is ignored.
        let mut other = Calibration::default();
        other.observe(2_000, 10_000);
        assert_eq!(other.ratio(), 1.0);
        other.observe(0, 10_000);
        other.observe(10_000, 0);
        assert_eq!(other.ratio(), 1.0);

        // A wild overrun is bounded, so one absurd round cannot empty the fit.
        let mut wild = Calibration::default();
        for _ in 0..40 {
            wild.observe(1_000_000, 1_000);
        }
        assert_eq!(wild.ratio(), MAX_RATIO);
    }

    #[test]
    fn scaled_budget_never_grows() {
        assert_eq!(scaled_budget(100_000, 1.0), 100_000);
        assert_eq!(scaled_budget(100_000, 2.0), 50_000);
        assert_eq!(scaled_budget(100_000, 0.5), 100_000);
        assert_eq!(scaled_budget(100_000, f32::NAN), 100_000);
    }

    /// The literal message from the failure this module's invariant exists to
    /// prevent. A gateway's wording is the only signal available.
    #[test]
    fn length_rejections_are_recognised_from_their_wording() {
        let real = "Error from provider (Console Go): Upstream request failed: \
                    [invalid_request_error] This model's maximum context length is 1048576 \
                    tokens. However, you requested 1113093 tokens (729093 in the messages, \
                    384000 in the completion). Please reduce the length of the messages or \
                    completion.";
        assert!(is_context_length_error(real));

        for message in [
            "context_length_exceeded",
            "This model's maximum context length is 8192 tokens",
            "prompt is too long: 300000 tokens > 200000 maximum",
            "input is too long for requested model",
            "too many tokens",
            "You requested 400000 tokens, which exceeds the maximum number of tokens",
            "max_tokens is too large: 200000",
        ] {
            assert!(is_context_length_error(message), "{message}");
        }

        for message in [
            "insufficient credits",
            "invalid api key",
            "rate limit reached for gpt-4o",
            "HTTP 500",
            "the model returned malformed JSON",
        ] {
            assert!(!is_context_length_error(message), "{message}");
        }
    }

    #[test]
    fn image_cost_follows_pixels_not_bytes() {
        // A native 4K frame: around ten thousand tokens, not the old flat 1_200.
        assert!(image_tokens(3_840, 2_160) > 8_000);
        // The provider-honoured 1568 edge, a couple of thousand.
        assert!((1_000..4_000).contains(&image_tokens(1_568, 882)));
        // Even a 16x16 image occupies one tile, and a tile costs its full
        // price plus the per-request overhead. That is why the floor below is
        // unreachable: one tile already exceeds it. Kept as a floor in case
        // the tile cost is ever revised downwards, but asserted here as it
        // actually behaves rather than as it was hoped to.
        assert_eq!(image_tokens(16, 16), IMAGE_OVERHEAD + IMAGE_TOKENS_PER_TILE);
        assert!(
            image_tokens(16, 16) > MIN_IMAGE_TOKENS,
            "the floor cannot bind for any real image"
        );
        // Unknown dimensions fall back rather than costing nothing.
        assert_eq!(image_tokens(0, 0), UNKNOWN_IMAGE_TOKENS);
        // Absurd dimensions are bounded.
        assert_eq!(image_tokens(100_000, 100_000), MAX_IMAGE_TOKENS);
    }

    #[test]
    fn json_is_estimated_more_densely_than_prose() {
        let schema = r#"{"type":"object","properties":{"path":{"type":"string"}}}"#;
        assert!(tokens_for_json(schema) > tokens_for(schema));
    }

    #[test]
    fn small_histories_pass_through_untouched() {
        let history = vec![
            message(Role::User, "hello"),
            message(Role::Assistant, "hi"),
            message(Role::User, "write a file"),
        ];
        let fitted = fit_report(&history, 50_000, true, Folding::none());
        assert_eq!(fitted.messages.len(), 3);
        assert_eq!(fitted.messages[0].content, "hello");
        assert_eq!(fitted.messages[2].content, "write a file");
        assert!(!fitted.trimmed);
        assert!(fitted.condensed.is_none());
        assert!(fitted.estimate > 0);
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

        let fitted = fit_report(&history, 5_000, true, Folding::none());

        // No turn was condensed and nothing was dropped; the big output shrank
        // to a note, which has never been reported.
        assert_eq!(fitted.messages.len(), history.len());
        assert!(fitted.condensed.is_none());
        let calls = parse_stored_tools(fitted.messages[1].extra.as_deref());
        assert_eq!(calls.len(), 1);
        assert!(calls[0].output.contains("omitted"), "{}", calls[0].output);
        // And the conversation text survives.
        assert_eq!(fitted.messages[3].content, "now the small one");
        assert!(fitted.trimmed);
    }

    /// The whole point of the module: older turns become a block that says what
    /// happened, instead of a note saying something is missing.
    #[test]
    fn old_turns_are_condensed_rather_than_dropped_when_eliding_is_not_enough() {
        let history = vec![
            message(Role::User, &"a".repeat(40_000)),
            message(Role::Assistant, "first answer"),
            message(Role::User, "second question"),
            message(Role::Assistant, "second answer"),
            message(Role::User, "current question"),
            message(Role::Assistant, "working"),
        ];

        let fitted = fit_report(&history, 1_000, true, Folding::none());

        // The block stands in for the older turns, and the current turn is
        // intact and still the last one.
        assert!(
            fitted.messages[0].content.contains("condensed into the summary"),
            "{}",
            fitted.messages[0].content
        );
        // It carries what was asked and concluded, not just a count.
        assert!(fitted.messages[0].content.contains("first answer"));
        assert!(fitted.messages[0].content.contains("second question"));
        assert!(!fitted.messages[0].content.contains("current question"));
        assert!(fitted
            .messages
            .iter()
            .any(|m| m.content == "current question"));

        let condensed = fitted.condensed.expect("the fold is reported");
        assert_eq!(condensed.source, Source::Digest);
        assert!(condensed.tokens > 0);
        assert!(
            condensed.covered >= 2,
            "covered {} of the older turns",
            condensed.covered
        );
        assert!(fitted.trimmed);
    }

    /// A stored summary is used verbatim when it still describes the history,
    /// and its fold point is honoured even when the budget could afford more.
    #[test]
    fn a_stored_summary_fixes_the_fold_point() {
        // The first exchange is enormous, so a fold is genuinely needed; the
        // second is small enough to survive verbatim beside the block. That is
        // the situation the fold point exists for.
        let history = vec![
            message(Role::User, &"a".repeat(40_000)),
            message(Role::Assistant, "first answer"),
            message(Role::User, "second question"),
            message(Role::Assistant, "second answer"),
            message(Role::User, "current question"),
            message(Role::Assistant, "working"),
        ];
        // Covers the first exchange only, so the second stays verbatim.
        let covers = history[1].id.clone();
        let summary = SessionSummary {
            session_id: "s".into(),
            covers_through_id: covers,
            covers_through_at: 0,
            covered_count: 2,
            text: "Goal:\n- a stored summary of the first exchange".into(),
            tokens: 20,
            model: Some("gpt-4o-mini".into()),
            updated_at: 0,
        };

        let fitted = fit_report(
            &history,
            1_000,
            true,
            Folding {
                window: 8_000,
                summary: Some(&summary),
                goal: None,
                todos: &[],
                share: None,
            },
        );

        assert!(
            fitted.messages[0]
                .content
                .contains("a stored summary of the first exchange"),
            "{}",
            fitted.messages[0].content
        );
        assert_eq!(fitted.condensed.expect("folded").source, Source::Summary);
        // The second exchange is untouched, because the summary does not
        // describe it.
        assert!(fitted
            .messages
            .iter()
            .any(|m| m.content == "second question"));
        assert!(fitted
            .messages
            .iter()
            .any(|m| m.content == "second answer"));
    }

    /// An edit or a delete can take the message a summary reaches. The row is
    /// then stale, and a fresh digest must stand in rather than the fold
    /// landing somewhere the summary knows nothing about.
    #[test]
    fn a_summary_whose_fold_point_is_gone_is_replaced_by_a_digest() {
        let history = vec![
            message(Role::User, &"a".repeat(40_000)),
            message(Role::Assistant, "first answer"),
            message(Role::User, "current question"),
            message(Role::Assistant, "working"),
        ];
        let summary = SessionSummary {
            session_id: "s".into(),
            covers_through_id: "a message that no longer exists".into(),
            covers_through_at: 0,
            covered_count: 2,
            text: "Goal:\n- a summary of something that is gone".into(),
            tokens: 20,
            model: None,
            updated_at: 0,
        };

        let fitted = fit_report(
            &history,
            1_000,
            true,
            Folding {
                window: 8_000,
                summary: Some(&summary),
                goal: Some("the real goal"),
                todos: &[],
                share: None,
            },
        );

        assert_eq!(fitted.condensed.expect("folded").source, Source::Digest);
        assert!(fitted.messages[0].content.contains("the real goal"));
        assert!(!fitted.messages[0].content.contains("something that is gone"));
    }

    /// `chat.condenseShare = 0` restores the old behaviour: older turns go,
    /// no block stands in for them, and nothing is reported as a stop.
    #[test]
    fn a_zero_share_drops_older_turns_instead_of_condensing() {
        let history = vec![
            message(Role::User, &"a".repeat(40_000)),
            message(Role::Assistant, "first answer"),
            message(Role::User, "second question"),
            message(Role::Assistant, "second answer"),
            message(Role::User, "current question"),
            message(Role::Assistant, "working"),
        ];

        let fitted = fit_report(
            &history,
            1_000,
            true,
            Folding {
                share: Some(0),
                ..Default::default()
            },
        );

        assert!(fitted.condensed.is_none(), "nothing was condensed");
        assert!(!fitted
            .messages
            .iter()
            .any(|m| m.content == "first answer"));
        assert!(fitted
            .messages
            .iter()
            .any(|m| m.content == "current question"));
        assert!(fitted.trimmed);
    }

    /// A summary that is already in force must not cover the current turn: the
    /// live request is never folded into a description of itself.
    #[test]
    fn the_current_turn_is_never_folded_away() {
        let history = vec![
            message(Role::User, "old question"),
            message(Role::Assistant, "old answer"),
            message(Role::User, &"b".repeat(40_000)),
            message(Role::Assistant, "working"),
        ];
        // A summary claiming to reach the live user turn.
        let summary = SessionSummary {
            session_id: "s".into(),
            covers_through_id: history[2].id.clone(),
            covers_through_at: 0,
            covered_count: 3,
            text: "Goal:\n- covers too much".into(),
            tokens: 10,
            model: None,
            updated_at: 0,
        };

        let fitted = fit_report(
            &history,
            1_000,
            true,
            Folding {
                window: 8_000,
                summary: Some(&summary),
                goal: None,
                todos: &[],
                share: None,
            },
        );

        assert!(!fitted.messages[0].content.contains("covers too much"));
        assert!(fitted
            .messages
            .iter()
            .any(|m| m.content.chars().all(|c| c == 'b')));
    }

    #[test]
    fn an_oversized_current_turn_is_elided_rather_than_rejected() {
        let history = vec![
            message(Role::User, "read the enormous log"),
            tool_message(200_000),
        ];

        let fitted = fit_report(&history, 500, true, Folding::none());

        let calls = parse_stored_tools(fitted.messages[1].extra.as_deref());
        assert!(calls[0].output.contains("omitted"), "{}", calls[0].output);
        assert_eq!(fitted.messages[0].content, "read the enormous log");
    }

    /// A screenshot the eliding cannot touch: one native-resolution frame on a
    /// small-window model is over budget on its own, so the pixels have to be
    /// droppable or the request can never fit.
    #[test]
    fn a_screenshot_too_large_to_keep_is_dropped_for_its_breadcrumb() {
        let extra = format!(
            r#"{{"toolCalls":[{{"id":"c1","name":"screenshot","arguments":"{{}}","status":"ok","output":"Screenshot 3840x2160","images":[{}]}}]}}"#,
            serde_json::to_string(&ToolImage {
                name: "Computer Screen 1.png".into(),
                mime: "image/png".into(),
                path: "/tmp/x.png".into(),
                width: 3_840,
                height: 2_160,
            })
            .unwrap()
        );
        let history = vec![
            message(Role::User, "click the button"),
            Message {
                extra: Some(extra),
                ..message(Role::Assistant, "looking")
            },
        ];

        // The image is visible to the budget before the fit.
        assert!(tool_image_cost(&history, 0) > 8_000);

        let fitted = fit_report(&history, 2_000, true, Folding::none());
        let calls = parse_stored_tools(fitted.messages[1].extra.as_deref());
        assert!(calls[0].images.is_empty(), "images should be dropped");
        assert!(
            calls[0].output.contains("n/a") || calls[0].output.contains("omitted"),
            "{}",
            calls[0].output
        );
        assert_eq!(
            tool_image_cost(&fitted.messages, 0),
            0,
            "the budget must see the pixels gone"
        );
        assert!(fitted.trimmed);
    }

    #[test]
    fn asking_for_no_images_strips_them_before_anything_else() {
        let extra = format!(
            r#"{{"toolCalls":[{{"id":"c1","name":"screenshot","arguments":"{{}}","status":"ok","output":"Screenshot 1568x882","images":[{}]}}]}}"#,
            serde_json::to_string(&ToolImage {
                name: "Computer Screen 1.png".into(),
                mime: "image/png".into(),
                path: "/tmp/x.png".into(),
                width: 1_568,
                height: 882,
            })
            .unwrap()
        );
        let history = vec![
            message(Role::User, "look"),
            Message {
                extra: Some(extra),
                ..message(Role::Assistant, "looking")
            },
        ];

        let fitted = fit_report(&history, 500_000, false, Folding::none());
        let calls = parse_stored_tools(fitted.messages[1].extra.as_deref());
        assert!(calls[0].images.is_empty());
        // Nothing else was touched: the budget was never tight.
        assert!(calls[0].output.contains("Screenshot 1568x882"));
        assert!(fitted.trimmed);
    }

    #[test]
    fn tool_pairing_keeps_its_shape_when_compressed() {
        let history = vec![
            message(Role::User, "go"),
            tool_message(100_000),
            tool_message(100_000),
        ];
        let fitted = fit_report(&history, 1_000, true, Folding::none());
        // Every remaining assistant turn still carries its calls, so the wire
        // never has a tool result without a preceding call.
        for message in &fitted.messages {
            if message.role == Role::Assistant {
                assert!(!parse_stored_tools(message.extra.as_deref()).is_empty());
            }
        }
    }

    /// Only the thinking the wire echoes back is charged for. Counting the
    /// aggregate over-estimated long reasoning turns and dropped history that
    /// would have fitted.
    #[test]
    fn thinking_is_counted_the_way_the_wire_sends_it() {
        let finished = vec![
            message(Role::User, "why is the sky blue"),
            Message {
                reasoning: Some("x".repeat(40_000)),
                ..message(Role::Assistant, "Rayleigh scattering.")
            },
            message(Role::User, "and at sunset"),
        ];
        // The completed turn's thinking is not sent back, so it must not be
        // counted: the estimate stays small.
        let small = total_tokens(&finished, 2);

        // The turn in progress does echo its last thinking spell.
        let current = vec![
            message(Role::User, "why is the sky blue"),
            Message {
                reasoning: Some("x".repeat(40_000)),
                ..message(Role::Assistant, "thinking")
            },
        ];
        let large = total_tokens(&current, 0);
        assert!(large > small, "{large} vs {small}");
    }
}
