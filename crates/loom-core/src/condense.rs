//! Folding a long chat's older turns into one block, so the wire fits the
//! model's window without simply dropping the context.
//!
//! Two producers, one shape. [`digest`] builds a block in-process from the
//! stored messages, deterministically and with no network call, so a turn that
//! first hits the wall never waits on anything. The background pass asks the
//! lite model for a proper summary and stores it, and that is used from the
//! next turn on. Both are rendered as the same `Heading:`-plus-bullets layout,
//! which is what lets one clamping rule serve either: when the block does not
//! fit, whole sections are dropped from the bottom of [`SECTIONS`] rather than
//! the text being cut mid-sentence.
//!
//! Nothing here touches the database or the network. The budget arithmetic
//! lives in `context`, the storage in `db`, and the scheduling in `engine`.

use crate::context;
use crate::db::{Message, Role, Todo};
use crate::engine::parse_stored_tools;

/// Share of the model's window the condensed block may occupy. 20% is enough
/// for real detail rather than headlines, and the `/ 3` cap below keeps it from
/// crowding the live turn out on a small window.
pub const CONDENSED_SHARE: u32 = 20;

/// Floor for the block, so an 8k local model still gets a usable summary
/// rather than a truncated shrug.
pub const MIN_CONDENSED_TOKENS: u32 = 1_000;

/// How full the request must be before the background pass is worth kicking
/// off. Well short of the wall on purpose: the point is that a summary is
/// already written when the fold first becomes necessary.
pub const CONDENSE_TRIGGER: u32 = 70;

/// Sections in the order they are *kept*. When a block is too long the ones at
/// the bottom go first: a command line is re-derivable from the transcript,
/// whereas the goal is the thing the whole chat is anchored to.
pub const SECTIONS: &[&str] = &[
    "Goal",
    "Open threads",
    "What was asked",
    "Files changed",
    "Established",
    "Commands",
];

/// Per-line caps, so one sprawling tool call cannot fill a section.
const ASKED_CHARS: usize = 200;
const CONCLUDED_CHARS: usize = 200;
const CHANGE_CHARS: usize = 100;
const COMMAND_CHARS: usize = 100;
/// How many lines each section will carry before it stops.
const ASKED_LINES: usize = 12;
const CONCLUDED_LINES: usize = 8;
const CHANGE_LINES: usize = 20;
const COMMAND_LINES: usize = 12;

/// The tool calls that change a file, and the field each one names it in.
/// Reading is deliberately absent: a long chat reads hundreds of files, and
/// listing them crowds out what actually changed.
const FILE_TOOLS: &[(&str, &str)] = &[
    ("write_file", "path"),
    ("edit_file", "path"),
    ("create_dir", "path"),
    ("delete_path", "path"),
];

/// The largest share a block may claim, whatever the config says. Past this it
/// stops being a summary of the past and starts crowding out the live turn it
/// exists to protect.
pub const MAX_CONDENSED_SHARE: u32 = 50;

/// The ceiling for the condensed block, in tokens.
///
/// `share` comes from `chat.condenseShare`. The floor and the cap are ordered
/// rather than clamped twice, so this cannot panic on a window small enough
/// that the floor exceeds the cap — which is what a 4k local model with
/// several MCP servers genuinely looks like.
pub fn condensed_budget(share: u32, window: u32, root_budget: u32) -> u32 {
    let share = share.min(MAX_CONDENSED_SHARE);
    let portion = (window as u64 * share as u64 / 100) as u32;
    let cap = (root_budget / 3).max(1);
    let floor = MIN_CONDENSED_TOKENS.min(cap);
    portion.clamp(floor, cap)
}

/// How much room the summary may have, once the verbatim tail has taken what
/// it needs.
///
/// The tail is divided first on purpose: a fat summary that squeezed the live
/// tool output out of the request would be the exact failure this module
/// exists to remove.
pub fn summary_room(share: u32, window: u32, root_budget: u32, tail_cost: u32) -> u32 {
    let ceiling = condensed_budget(share, window, root_budget);
    let left = root_budget.saturating_sub(tail_cost);
    ceiling.min(left).max(MIN_CONDENSED_TOKENS.min(ceiling))
}

/// Whether a request came close enough to the budget to be worth summarising
/// ahead of time.
pub fn should_condense(estimate: u32, root_budget: u32) -> bool {
    root_budget > 0
        && (estimate as u64 * 100) >= (root_budget as u64 * CONDENSE_TRIGGER as u64)
}

/// Tokens of budget as a character limit. The same four-characters-per-token
/// estimate `context` budgets with, so the two agree about what fits.
fn chars_for(tokens: u32) -> usize {
    (tokens as u64 * context::CHARS_PER_TOKEN_FOR_CONDENSE as u64) as usize
}

/// The in-process digest: what was asked, what changed, what ran, what was
/// concluded, from the stored messages alone.
///
/// Deterministic, free, and works with no API key — which is what makes it
/// safe to call from inside the fit, where no model call can go. Filled
/// newest-first within the span, because the turns nearest the verbatim tail
/// are the ones the model is most likely to be building on.
pub fn digest(span: &[Message], goal: Option<&str>, todos: &[Todo]) -> String {
    let mut asked: Vec<String> = Vec::new();
    let mut concluded: Vec<String> = Vec::new();
    let mut changes: Vec<String> = Vec::new();
    let mut commands: Vec<String> = Vec::new();

    for message in span.iter().rev() {
        // An assistant turn may carry both a reply and the calls that produced
        // it, so the two are collected independently rather than by branch.
        if message.role == Role::User {
            let text = first_line(message.content.trim());
            if !text.is_empty() && asked.len() < ASKED_LINES {
                asked.push(clip(&text, ASKED_CHARS));
            }
        } else if !message.content.trim().is_empty() && concluded.len() < CONCLUDED_LINES {
            concluded.push(clip(&first_line(message.content.trim()), CONCLUDED_CHARS));
        }

        for call in parse_stored_tools(message.extra.as_deref()) {
            if let Some(change) = file_change(&call.name, &call.arguments) {
                let entry = format!("{} {}", call.name, clip(&change, CHANGE_CHARS));
                if changes.len() < CHANGE_LINES && !changes.contains(&entry) {
                    changes.push(entry);
                }
            }
            if call.name == "run_command" && commands.len() < COMMAND_LINES {
                if let Some(command) = argument_str(&call.arguments, "command") {
                    commands.push(format!(
                        "{} — {}",
                        clip(&first_line(&command), COMMAND_CHARS),
                        call.status
                    ));
                }
            }
        }
    }

    let mut sections: Vec<(usize, String)> = Vec::new();
    if let Some(goal) = goal.map(str::trim).filter(|goal| !goal.is_empty()) {
        if let Some(section) = bullet_section("Goal", &[goal.to_string()]) {
            sections.push((rank_of("Goal"), section));
        }
    }
    let open: Vec<String> = todos
        .iter()
        .filter(|todo| todo.status != "completed")
        .map(|todo| format!("[{}] {}", todo.status.replace('_', " "), todo.content))
        .collect();
    if let Some(section) = bullet_section("Open threads", &open) {
        sections.push((rank_of("Open threads"), section));
    }
    for (name, lines) in [
        ("What was asked", asked),
        ("Files changed", changes),
        ("Established", concluded),
        ("Commands", commands),
    ] {
        if let Some(section) = bullet_section(name, &lines) {
            sections.push((rank_of(name), section));
        }
    }
    sections.sort_by_key(|(rank, _)| *rank);
    sections
        .into_iter()
        .map(|(_, text)| text)
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// [`digest`], clamped to fit `budget` tokens.
pub fn digest_for(
    span: &[Message],
    goal: Option<&str>,
    todos: &[Todo],
    budget: u32,
) -> String {
    clamp(&digest(span, goal, todos), budget)
}

/// The prompt the lite model is asked for a summary with.
///
/// The previous summary is carried in explicitly rather than re-deriving the
/// whole history each time: each rewrite folds the old block plus whatever has
/// newly aged out, so the compression ratio rises as a chat grows instead of
/// the block growing linearly.
pub fn summary_prompt(previous: Option<&str>, transcript: &str) -> String {
    let shape = SECTIONS
        .iter()
        .map(|name| format!("{name}:"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut prompt = String::from(
        "You are maintaining a running summary of an AI coding chat, so that the older part \
         of the conversation can be replaced by it without losing what matters.\n\n\
         Write the summary in exactly these sections, in this order, each heading on its own \
         line followed by `- ` bullets:\n",
    );
    prompt.push_str(&shape);
    prompt.push_str(
        "\n\nRules:\n\
         - Facts, not narration. Names, paths, decisions, and what was tried.\n\
         - Carry forward anything still unresolved: unfinished work, known bugs, and \
           questions the user has not answered yet.\n\
         - Keep the user's own wording where it states a requirement or a preference.\n\
         - Commands: give the command and whether it worked. Keep failures.\n\
         - Omit a section entirely if you have nothing for it.\n",
    );
    if let Some(previous) = previous.map(str::trim).filter(|text| !text.is_empty()) {
        prompt.push_str("\nThe summary so far, which you are extending:\n\n");
        prompt.push_str(previous);
        prompt.push_str("\n\n");
    }
    prompt.push_str("The newly aged-out part of the conversation:\n\n");
    prompt.push_str(transcript);
    prompt.push_str("\n\nWrite the updated summary. Output only the summary itself.");
    prompt
}

/// A transcript for the summariser: role, reply text, and one line per tool
/// call. Capped so the pass itself cannot be the thing that overflows a window.
pub fn transcript_excerpt(span: &[Message], max_chars: usize) -> String {
    let mut lines: Vec<String> = Vec::new();
    for message in span {
        let role = if message.role == Role::User {
            "User"
        } else {
            "Assistant"
        };
        let text = message.content.trim();
        if !text.is_empty() {
            lines.push(format!("{role}: {}", clip(text, 900)));
        }
        for call in parse_stored_tools(message.extra.as_deref()) {
            lines.push(format!(
                "  [{} {} -> {}]",
                call.name,
                clip(&first_line(&call.arguments), 90),
                call.status
            ));
        }
    }
    // Newest content is the most useful, so an over-long transcript loses its
    // oldest lines rather than its newest.
    let mut total = 0usize;
    let mut kept: Vec<&str> = Vec::new();
    for line in lines.iter().rev() {
        total += line.chars().count() + 1;
        if total > max_chars && !kept.is_empty() {
            break;
        }
        kept.push(line);
    }
    kept.reverse();
    kept.join("\n")
}

/// The model's summary, clamped to the budget. `None` when there is nothing
/// usable, so the caller can keep the digest it already had.
pub fn parse_summary(raw: &str, budget: u32) -> Option<(String, u32)> {
    let text = strip_fence(raw);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let clamped = clamp(trimmed, budget);
    let tokens = context::tokens_for(&clamped);
    Some((clamped, tokens))
}

/// Fits a block to `budget` tokens by dropping whole sections, lowest priority
/// first. Never splits a section down the middle of a sentence unless a single
/// remaining section is itself over budget, where there is nothing else to do.
pub fn clamp(text: &str, budget: u32) -> String {
    let limit = chars_for(budget).max(1);
    if text.chars().count() <= limit {
        return text.to_string();
    }

    // Unknown leading prose outranks every named section: it is usually the
    // one sentence that says what the chat is about. It is rank 0, and named
    // sections are ranked from 1, so a document that opens straight into a
    // heading cannot borrow the preamble's rank and be dropped with it.
    let mut sections: Vec<(usize, String)> = vec![(0, String::new())];
    for line in text.lines() {
        match heading_rank(line) {
            Some(rank) => sections.push((rank + 1, format!("{line}\n"))),
            None => {
                let last = sections.len() - 1;
                sections[last].1.push_str(line);
                sections[last].1.push('\n');
            }
        }
    }

    // Drop whole sections from the lowest priority upwards, stopping when the
    // highest-priority section that carries anything is all that is left. A
    // block that lost every section would say nothing at all, which is worse
    // than a block that is briefly too long and then clipped.
    loop {
        let total: usize = sections
            .iter()
            .map(|(_, body)| body.chars().count() + 2)
            .sum();
        if total <= limit {
            break;
        }
        let highest = sections
            .iter()
            .filter(|(_, body)| !body.trim().is_empty())
            .map(|(rank, _)| *rank)
            .max()
            .unwrap_or(0);
        let remaining: Vec<(usize, String)> = sections
            .iter()
            .filter(|(rank, _)| *rank != highest)
            .cloned()
            .collect();
        if highest == 0 || !remaining.iter().any(|(_, body)| !body.trim().is_empty()) {
            break;
        }
        sections = remaining;
    }

    let joined = sections
        .into_iter()
        .map(|(_, body)| body.trim_end().to_string())
        .filter(|body| !body.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if joined.chars().count() <= limit {
        return joined;
    }
    // One section alone is still too long: cut it, but say so rather than
    // presenting a half-sentence as a complete thought. The marker is dropped
    // when the budget cannot hold it, since a note about the truncation would
    // then be the whole of what the model was given.
    const MARKER: &str = "\n- [summary truncated to fit the context budget]";
    if limit > MARKER.chars().count() + 40 {
        format!("{}{MARKER}", clip(&joined, limit - MARKER.chars().count()))
    } else {
        clip(&joined, limit)
    }
}

/// Strips a whole-block markdown fence, which models add out of habit.
fn strip_fence(raw: &str) -> String {
    let text = raw.trim();
    let Some(rest) = text.strip_prefix("```") else {
        return text.to_string();
    };
    // Drop the language tag on the fence line.
    let body = match rest.find('\n') {
        Some(index) => &rest[index + 1..],
        None => "",
    };
    body.trim_end()
        .trim_end_matches("```")
        .trim_end()
        .to_string()
}

/// The rank of a line that introduces a section, if it is one.
///
/// Deliberately strict about what counts: a bullet that happens to begin with
/// the word "Goal" is content, not a heading, and a lenient rule here would
/// split sections in the wrong place.
fn heading_rank(line: &str) -> Option<usize> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        return None;
    }
    let bare = trimmed
        .trim_start_matches('#')
        .trim()
        .trim_end_matches(':')
        .trim()
        .to_ascii_lowercase();
    SECTIONS
        .iter()
        .position(|name| name.to_ascii_lowercase() == bare)
}

fn rank_of(name: &str) -> usize {
    SECTIONS
        .iter()
        .position(|section| *section == name)
        .unwrap_or(SECTIONS.len())
}

fn bullet_section(name: &str, lines: &[String]) -> Option<String> {
    if lines.is_empty() {
        return None;
    }
    Some(format!(
        "{name}:\n{}",
        lines
            .iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

/// The path a file-changing call touched, if that is what it is.
fn file_change(name: &str, arguments: &str) -> Option<String> {
    if name == "move_path" || name == "copy_path" {
        let from = argument_str(arguments, "from")?;
        let to = argument_str(arguments, "to")?;
        return Some(format!("{} -> {}", clip(&from, CHANGE_CHARS), clip(&to, CHANGE_CHARS)));
    }
    let field = FILE_TOOLS
        .iter()
        .find(|(tool, _)| *tool == name)
        .map(|(_, field)| *field)?;
    argument_str(arguments, field).map(|path| clip(&path, CHANGE_CHARS))
}

fn argument_str(arguments: &str, key: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(arguments).ok()?;
    let value = parsed.get(key)?.as_str()?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

/// Truncates on a character boundary, ellipsis included in the budget.
fn clip(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Todo;

    fn message(role: Role, content: &str, extra: Option<&str>) -> Message {
        Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: "s".into(),
            role,
            content: content.into(),
            reasoning: None,
            extra: extra.map(str::to_string),
            persona_id: None,
            created_at: 0,
        }
    }

    fn call(name: &str, arguments: &str, status: &str) -> String {
        let output = "done";
        format!(
            r#"{{"toolCalls":[{{"id":"c1","name":"{name}","arguments":{},"status":"{status}","output":{}}}]}}"#,
            serde_json::to_string(arguments).unwrap(),
            serde_json::to_string(output).unwrap()
        )
    }

    fn todo(content: &str, status: &str) -> Todo {
        Todo {
            id: content.into(),
            content: content.into(),
            status: status.into(),
            position: 0,
        }
    }

    /// A 128k window with a 91_636 budget: 20% of the window is 25_600, which
    /// is under the `/ 3` cap, so the share binds.
    #[test]
    fn the_budget_is_the_share_until_the_cap_bites() {
        assert_eq!(condensed_budget(20, 128_000, 91_636), 25_600);
        assert_eq!(condensed_budget(20, 200_000, 152_836), 40_000);
        // A million-token window: 20% is 209_715, still under a third of the
        // budget, so the share is what binds.
        assert_eq!(condensed_budget(20, 1_048_576, 832_836), 209_715);
        // A 32k local model: 6_400 is over a third of a 10_036 budget, so the
        // cap holds it near 3_345 instead of a quarter of the window.
        assert_eq!(condensed_budget(20, 32_000, 10_036), 3_345);
        // The knob moves it, and a wider share is capped harder.
        assert_eq!(condensed_budget(10, 128_000, 91_636), 12_800);
        assert_eq!(condensed_budget(50, 128_000, 91_636), 30_545);
        // Absurd settings cannot claim the whole budget.
        for share in [51u32, 100, 10_000] {
            assert_eq!(
                condensed_budget(share, 128_000, 91_636),
                condensed_budget(MAX_CONDENSED_SHARE, 128_000, 91_636)
            );
        }
    }

    #[test]
    fn a_tiny_window_still_gets_a_usable_block() {
        // The floor cannot exceed the cap, whatever the numbers say.
        for (window, budget) in [(4_096u32, 600u32), (1_000, 1), (0, 0), (8_192, 100)] {
            let value = condensed_budget(20, window, budget);
            assert!(value <= (budget / 3).max(1), "{value} vs {budget}");
        }
        // 20% of a 4k window is 819 tokens, under the floor; the floor takes
        // it up to a thousand, which its budget can still afford.
        assert_eq!(condensed_budget(20, 4_096, 4_096), 1_000);
    }

    /// The division that keeps the present intact: a summary never takes the
    /// room the live turn needs.
    #[test]
    fn the_tail_is_paid_for_first() {
        // Room to spare: the ceiling is the block's size.
        assert_eq!(summary_room(20, 128_000, 91_636, 10_000), 25_600);
        // A tail that nearly fills the budget leaves the block its floor.
        assert_eq!(summary_room(20, 128_000, 91_636, 91_000), 1_000);
        // A tail over budget does not make the room go negative.
        assert_eq!(summary_room(20, 128_000, 91_636, 500_000), 1_000);
    }

    #[test]
    fn the_trigger_fires_well_before_the_wall() {
        assert!(!should_condense(1_000, 91_636));
        assert!(!should_condense(64_000, 91_636));
        assert!(should_condense(64_146, 91_636));
        assert!(should_condense(91_636, 91_636));
        // A zero budget is not "70% used".
        assert!(!should_condense(0, 0));
    }

    #[test]
    fn the_digest_carries_the_goal_the_asks_and_what_changed() {
        let span = vec![
            message(Role::User, "Rename presetById everywhere", None),
            message(
                Role::Assistant,
                "Six call sites, two of them copy-paste.",
                Some(&call("edit_file", r#"{"path":"src/lib/background.ts"}"#, "ok")),
            ),
            message(
                Role::Assistant,
                "Running the tests.",
                Some(&call("run_command", r#"{"command":"bun run test"}"#, "error")),
            ),
        ];
        let todos = vec![todo("fix the fallback", "in_progress"), todo("done thing", "completed")];

        let digest = digest(&span, Some("Rename presetById"), &todos);

        assert!(digest.contains("Goal:\n- Rename presetById"), "{digest}");
        // Only the unfinished task is an open thread.
        assert!(digest.contains("Open threads:\n- [in progress] fix the fallback"), "{digest}");
        assert!(!digest.contains("done thing"), "{digest}");
        assert!(digest.contains("- Rename presetById everywhere"), "{digest}");
        assert!(digest.contains("- edit_file src/lib/background.ts"), "{digest}");
        assert!(digest.contains("- bun run test — error"), "{digest}");
        // Sections come out in the declared order, not the collection order.
        let order: Vec<usize> = SECTIONS
            .iter()
            .filter_map(|name| digest.find(&format!("{name}:")))
            .collect();
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{digest}");
    }

    #[test]
    fn the_digest_deduplicates_repeated_file_edits() {
        let same = call("edit_file", r#"{"path":"src/a.rs"}"#, "ok");
        let span = vec![
            message(Role::Assistant, "one", Some(&same)),
            message(Role::Assistant, "two", Some(&same)),
        ];
        let digest = digest(&span, None, &[]);
        assert_eq!(digest.matches("edit_file src/a.rs").count(), 1, "{digest}");
    }

    #[test]
    fn moves_and_deletes_are_named_with_both_ends() {
        let span = vec![message(
            Role::Assistant,
            "moving it",
            Some(&call(
                "move_path",
                r#"{"from":"src/a.ts","to":"src/lib/a.ts"}"#,
                "ok",
            )),
        )];
        let digest = digest(&span, None, &[]);
        assert!(digest.contains("- move_path src/a.ts -> src/lib/a.ts"), "{digest}");
    }

    #[test]
    fn an_empty_span_produces_an_empty_digest() {
        assert_eq!(digest(&[], None, &[]), "");
        assert_eq!(digest_for(&[], None, &[], 1_000), "");
    }

    #[test]
    fn a_digest_over_budget_loses_whole_sections_from_the_bottom() {
        let span = vec![
            message(Role::User, &format!("ask about {}", "x".repeat(600)), None),
            message(
                Role::Assistant,
                &format!("answer {}", "y".repeat(600)),
                Some(&call("run_command", r#"{"command":"cargo test"}"#, "ok")),
            ),
        ];
        let full = digest(&span, Some("the goal"), &[]);
        // Comfortably fits: nothing is dropped.
        assert_eq!(clamp(&full, 10_000), full);

        // Tight: commands go first, then the conclusion, and the goal — the
        // thing the whole chat is anchored to — survives.
        let tight = clamp(&full, 100);
        assert!(tight.contains("Goal:"), "{tight}");
        assert!(!tight.contains("Commands:"), "{tight}");
        assert!(!tight.contains("Established:"), "{tight}");
        assert!(tight.chars().count() <= chars_for(100), "{}", tight.chars().count());
    }

    /// The last resort must still fit, or the fit it feeds would be defeated.
    #[test]
    fn clamping_always_lands_inside_the_budget() {
        let huge = format!("Goal:\n- {}", "z".repeat(50_000));
        for budget in [1u32, 10, 60, 500, 2_000] {
            let clamped = clamp(&huge, budget);
            assert!(
                clamped.chars().count() <= chars_for(budget),
                "{budget}: {} chars",
                clamped.chars().count()
            );
        }
        // Where the budget can hold the heading it is kept, and it says that it
        // was cut rather than presenting a half-line as a complete thought.
        for budget in [60u32, 500, 2_000] {
            let clamped = clamp(&huge, budget);
            assert!(clamped.starts_with("Goal:"), "{budget}: {clamped}");
            assert!(clamped.contains("truncated"), "{budget}: {clamped}");
        }
    }

    /// A summary that opens straight into a heading has no preamble to protect
    /// it. Ranking the preamble and `Goal` both at zero used to let the drop
    /// loop take the whole block down to nothing.
    #[test]
    fn unknown_prose_is_kept_ahead_of_every_named_section() {
        let text = "This chat is about the rename.\n\nCommands:\n- cargo test — ok\n";
        let clamped = clamp(text, 10);
        assert!(clamped.starts_with("This chat is about the rename."), "{clamped}");
        assert!(!clamped.contains("Commands"), "{clamped}");

        // No preamble at all: the highest-priority section is never dropped,
        // however small the budget is.
        let bare = clamp("Goal:\n- ship it", 60);
        assert!(bare.starts_with("Goal:"), "{bare}");
    }

    #[test]
    fn a_model_fence_is_stripped_and_a_useless_answer_is_refused() {
        let fenced = "```markdown\nGoal:\n- ship it\n```";
        let (text, tokens) = parse_summary(fenced, 1_000).expect("a usable summary");
        assert_eq!(text, "Goal:\n- ship it");
        assert!(tokens > 0);

        assert!(parse_summary("", 1_000).is_none());
        assert!(parse_summary("   \n  ", 1_000).is_none());
        assert!(parse_summary("```\n\n```", 1_000).is_none());
    }

    #[test]
    fn the_prompt_carries_the_previous_summary_and_the_new_material() {
        let prompt = summary_prompt(Some("Goal:\n- the old goal"), "User: hello");
        assert!(prompt.contains("Goal:\n- the old goal"), "the fold point is carried");
        assert!(prompt.contains("User: hello"));
        assert!(prompt.contains("Carry forward anything still unresolved"));
        // Every section is named, so the model cannot invent its own shape.
        for name in SECTIONS {
            assert!(prompt.contains(&format!("{name}:")), "{name} missing");
        }

        // First fold: no previous summary, and no empty preamble either.
        let fresh = summary_prompt(None, "User: hi");
        assert!(!fresh.contains("which you are extending"));
    }

    #[test]
    fn the_excerpt_keeps_the_newest_end_of_a_long_history() {
        let span: Vec<Message> = (0..400)
            .map(|index| message(Role::User, &format!("message {index} {}", "x".repeat(200)), None))
            .collect();
        let excerpt = transcript_excerpt(&span, 2_000);
        assert!(excerpt.chars().count() <= 2_400, "{}", excerpt.chars().count());
        assert!(excerpt.contains("message 399"), "the newest turn is the one kept");
        assert!(!excerpt.contains("message 0 "), "the oldest lines are the ones dropped");
    }

    #[test]
    fn a_tool_call_is_one_line_in_the_excerpt() {
        let span = vec![message(
            Role::Assistant,
            "reading",
            Some(&call("read_file", r#"{"path":"src/a.rs"}"#, "ok")),
        )];
        let excerpt = transcript_excerpt(&span, 5_000);
        assert!(excerpt.contains("Assistant: reading"), "{excerpt}");
        assert!(excerpt.contains("[read_file "), "{excerpt}");
        assert!(excerpt.contains("-> ok]"), "{excerpt}");
        // The bulky output never travels: only the call and its status.
        assert!(!excerpt.contains("\"output\""), "{excerpt}");
    }

    #[test]
    fn long_lines_are_clipped_on_a_character_boundary() {
        let wide = "é".repeat(500);
        let clipped = clip(&wide, 100);
        assert_eq!(clipped.chars().count(), 100);
        assert!(clipped.ends_with('…'));
        // Shorter text is returned untouched.
        assert_eq!(clip("short", 100), "short");
    }

    /// A heading is a heading; a bullet that begins with the word is not.
    #[test]
    fn only_real_headings_split_sections() {
        assert_eq!(heading_rank("Goal:"), Some(rank_of("Goal")));
        assert_eq!(heading_rank("## Files changed"), Some(rank_of("Files changed")));
        // Ranked in the declared order, so lower priority means easier to drop.
        assert!(rank_of("Goal") < rank_of("Commands"));
        assert_eq!(heading_rank("- Goal: the thing"), None);
        assert_eq!(heading_rank("Something else entirely"), None);
        assert_eq!(heading_rank(""), None);
    }
}
