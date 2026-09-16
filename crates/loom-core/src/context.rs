//! Keeps a request inside the model's context window.
//!
//! A message-count history limit cannot do this job: one `read_file` can be a
//! hundred thousand tokens, and a model with a million-token window should get
//! far more than forty messages. Instead the request budget is derived from
//! the model's context window, and the conversation is compressed to fit —
//! bulky tool outputs are elided first, then whole turns are dropped, and the
//! model is told when that happened. Nothing is deleted from the database;
//! this shapes the wire only.
//!
//! Two rules keep the arithmetic honest, because the failure this module
//! exists to prevent is a rejected request, not a degraded one. First, the
//! reply headroom [`input_budget`] reserves is *exactly* the completion budget
//! the adapters declare: [`output_limit`] is the single source of that number,
//! and reserving less than the request declares borrows from the safety margin
//! silently. Second, [`Calibration`] corrects the character-based estimate
//! from the token counts providers report, so the fit converges on models whose
//! tokeniser disagrees with four characters per token instead of failing.

use crate::attachments::{self, AttachmentKind};
use crate::catalog;
use crate::db::{Message, Role};
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
    /// True when anything was elided, dropped, or stripped of its images.
    pub trimmed: bool,
}

/// Fits the conversation into `budget` tokens by compressing and dropping the
/// oldest content. The last user turn and everything after it — what the
/// current request is working from — is kept whenever any part of it fits.
pub fn fit(history: &[Message], budget: u32) -> Vec<Message> {
    fit_report(history, budget, true).messages
}

/// [`fit`], reporting the estimate and whether anything was trimmed, and able
/// to drop the inlined screenshot up front when the caller has already run out
/// of room (the retry after a length rejection).
pub fn fit_report(history: &[Message], budget: u32, allow_images: bool) -> Fitted {
    if history.is_empty() {
        return Fitted {
            messages: Vec::new(),
            estimate: 0,
            trimmed: false,
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
    //    turn keeps its fresh results for as long as possible.
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

    // 2. Still too big: drop whole older turns, keeping the current one. A
    //    synthetic note tells the model that context is missing instead of
    //    leaving a silent gap in the transcript.
    let mut dropped = 0usize;
    if used > budget {
        let tail_cost: u32 = (last_user..kept.len())
            .map(|index| message_tokens(&kept[index], index, &kept, index == last_user))
            .fold(0, u32::saturating_add);
        let mut prefix_cost: u32 = (0..last_user)
            .map(|index| message_tokens(&kept[index], index, &kept, false))
            .fold(0, u32::saturating_add);

        while dropped < last_user && tail_cost.saturating_add(prefix_cost) > budget {
            prefix_cost =
                prefix_cost.saturating_sub(message_tokens(&kept[dropped], dropped, &kept, false));
            dropped += 1;
        }

        // Stop on a turn boundary so a dropped question never leaves its
        // answer behind on its own.
        while dropped < last_user && kept[dropped].role != Role::User {
            dropped += 1;
        }

        if dropped > 0 {
            kept.drain(0..dropped);
            last_user -= dropped;
            used = cost(&kept, last_user);
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

    if dropped > 0 {
        kept.insert(0, omitted_note(dropped));
    }

    let estimate = cost(&kept, last_user_index(&kept));
    Fitted {
        messages: kept,
        estimate,
        trimmed,
    }
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
        let fitted = fit_report(&history, 50_000, true);
        assert_eq!(fitted.messages.len(), 3);
        assert_eq!(fitted.messages[0].content, "hello");
        assert_eq!(fitted.messages[2].content, "write a file");
        assert!(!fitted.trimmed);
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

        let fitted = fit_report(&history, 5_000, true);

        // No turn was dropped; the big output shrank to a note.
        assert_eq!(fitted.messages.len(), history.len());
        let calls = parse_stored_tools(fitted.messages[1].extra.as_deref());
        assert_eq!(calls.len(), 1);
        assert!(calls[0].output.contains("omitted"), "{}", calls[0].output);
        // And the conversation text survives.
        assert_eq!(fitted.messages[3].content, "now the small one");
        assert!(fitted.trimmed);
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

        let fitted = fit_report(&history, 1_000, true);

        // The note stands in for the dropped exchange, and the current turn is
        // intact.
        assert!(fitted.messages[0].content.contains("earlier messages"));
        assert!(fitted
            .messages
            .iter()
            .any(|m| m.content == "current question"));
        assert!(!fitted
            .messages
            .iter()
            .any(|m| m.content == "first answer"));
        assert!(fitted.trimmed);
    }

    #[test]
    fn an_oversized_current_turn_is_elided_rather_than_rejected() {
        let history = vec![
            message(Role::User, "read the enormous log"),
            tool_message(200_000),
        ];

        let fitted = fit_report(&history, 500, true);

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

        let fitted = fit_report(&history, 2_000, true);
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

        let fitted = fit_report(&history, 500_000, false);
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
        let fitted = fit_report(&history, 1_000, true);
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
