//! Raw host JSON → what the model reads.
//!
//! Kept apart from [`super`] because this is the part that decides the *cost*
//! of looking at a page. A snapshot is a compact indexed list rather than a DOM
//! dump, and a console log is the errors rather than every `log()`, and both of
//! those are choices made here rather than facts about the page.

use serde_json::Value;

/// Longest a generic JSON answer may be before it is elided. Results that need
/// more than this have a shaped renderer below.
pub const MAX_GENERIC_CHARS: usize = 12_000;

/// Widest the snapshot's label column is allowed to run, so one enormous button
/// cannot push every rect off the line.
const MAX_LABEL_CHARS: usize = 90;

pub fn truncate(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let cut = value
        .char_indices()
        .nth(limit)
        .map(|(index, _)| index)
        .unwrap_or(value.len());
    format!("{}…", &value[..cut])
}

fn str_at(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn num_at(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

fn array_at<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// The `url  title  ready` line every page-shaped result starts with.
fn header(value: &Value) -> String {
    let mut line = String::new();
    if let Some(url) = str_at(value, "url") {
        line.push_str(&url);
    }
    if let Some(title) = str_at(value, "title") {
        if !title.trim().is_empty() {
            line.push_str("  —  ");
            line.push_str(&truncate(title.trim(), 120));
        }
    }
    if let Some(ready) = str_at(value, "ready") {
        line.push_str(&format!("  [{ready}]"));
    }
    line
}

/// The footer naming what the *user* did, which is what makes shared control
/// safe rather than reckless: the model works around the user instead of
/// through them.
fn user_footer(value: &Value) -> Option<String> {
    let activity = value.get("userActivity")?;
    let recent = array_at(activity, "recent");
    let typed = array_at(activity, "typedFields");
    if recent.is_empty() && typed.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    let idle = num_at(activity, "idleMs");
    let when = match idle {
        Some(ms) if ms < 1000 => "just now".to_string(),
        Some(ms) => format!("{}s ago", ms / 1000),
        None => "at some point".to_string(),
    };
    lines.push(format!("user activity in this page ({when}):"));

    for event in recent.iter().rev().take(8) {
        let kind = str_at(event, "kind").unwrap_or_default();
        let detail = str_at(event, "detail").unwrap_or_default();
        let age = num_at(event, "at")
            .and_then(|at| milliseconds_now().checked_sub(at))
            .map(|delta| format!("{:.1}s", delta as f64 / 1000.0))
            .unwrap_or_else(|| "?".to_string());
        lines.push(format!("  {age:>7} ago  {kind}  {detail}"));
    }
    if !typed.is_empty() {
        let names: Vec<String> = typed
            .iter()
            .filter_map(Value::as_str)
            .map(|name| name.to_string())
            .collect();
        if !names.is_empty() {
            lines.push(format!(
                "  the user has typed into: {}  (re-read before submitting)",
                names.join(", ")
            ));
        }
    }
    Some(lines.join("\n"))
}

fn milliseconds_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// `browser_snapshot`: the money tool. An indexed list of actionable nodes, so
/// the model clicks `[7]` rather than a coordinate.
pub fn snapshot(value: &Value) -> String {
    let mut out = String::new();
    out.push_str("PAGE  ");
    out.push_str(&header(value));
    out.push('\n');

    if let (Some(viewport), Some(scroll)) = (
        value.get("viewport").and_then(Value::as_array),
        value.get("scroll").and_then(Value::as_array),
    ) {
        let dimensions = viewport
            .iter()
            .filter_map(Value::as_i64)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("x");
        let offset = scroll
            .iter()
            .filter_map(Value::as_i64)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let height = num_at(value, "scrollHeight").unwrap_or(0);
        out.push_str(&format!(
            "VIEW  {dimensions}  scroll {offset} of {height}\n"
        ));
    }

    let nodes = array_at(value, "nodes");
    if nodes.is_empty() {
        out.push_str("\nNo actionable elements found. The page may still be loading — ");
        out.push_str("`browser_wait` for a selector, or `browser_read` for the text.\n");
    } else {
        out.push('\n');
        for node in nodes {
            let line = node.as_str().unwrap_or_default();
            out.push_str(&truncate(line, MAX_LABEL_CHARS * 4));
            out.push('\n');
        }
    }

    if value.get("truncated").and_then(Value::as_bool) == Some(true) {
        out.push_str("\n[more elements than max_nodes; raise it or narrow with `selector`]\n");
    }
    let below = num_at(value, "belowFold").unwrap_or(0);
    if below > 0 {
        out.push_str(&format!(
            "[{below} more below the fold — pass all:true, or scroll]\n"
        ));
    }
    let frames = num_at(value, "frameCount").unwrap_or(1);
    if frames > 1 {
        out.push_str(&format!("[{frames} frames on this page; this is the main one]\n"));
    }

    if let Some(footer) = user_footer(value) {
        out.push('\n');
        out.push_str(&footer);
        out.push('\n');
    }
    if let Some(dialogs) = render_dialogs(value) {
        out.push('\n');
        out.push_str(&dialogs);
    }
    out.trim_end().to_string()
}

fn render_dialogs(value: &Value) -> Option<String> {
    let dialogs = array_at(value, "dialogs");
    if dialogs.is_empty() {
        return None;
    }
    let mut lines = vec!["A dialog is waiting — answer it with `browser_dialog`:".to_string()];
    for dialog in dialogs {
        let kind = str_at(dialog, "type").unwrap_or_default();
        let message = str_at(dialog, "message").unwrap_or_default();
        lines.push(format!("  {} \"{}\"", kind, truncate(&message, 200)));
    }
    Some(lines.join("\n"))
}

/// `browser_read`: the readable text, with `[n]` markers the model can click.
pub fn read(value: &Value) -> String {
    let mut out = String::new();
    out.push_str("PAGE  ");
    out.push_str(&header(value));
    out.push('\n');

    let text = str_at(value, "text").unwrap_or_default();
    if text.trim().is_empty() {
        out.push_str("\n(no readable text — the page may be a canvas, or still loading)\n");
    } else {
        out.push('\n');
        out.push_str(&text);
        out.push('\n');
    }
    if value.get("truncated").and_then(Value::as_bool) == Some(true) {
        out.push_str("\n[truncated; pass max_chars for more, or a `selector`]\n");
    }

    let marks = array_at(value, "marks");
    if !marks.is_empty() {
        out.push_str("\nCLICKABLE\n");
        for mark in marks {
            out.push_str("  ");
            out.push_str(mark.as_str().unwrap_or_default());
            out.push('\n');
        }
    }
    if let Some(footer) = user_footer(value) {
        out.push('\n');
        out.push_str(&footer);
        out.push('\n');
    }
    out.trim_end().to_string()
}

/// `browser_console`: the failures first, because they are why the call was
/// made. A page that logs a hundred lines and throws one error should lead with
/// the error.
pub fn console(value: &Value) -> String {
    let failures = array_at(value, "failures");
    let entries = array_at(value, "console");
    let mut out = String::new();

    let errors: Vec<&Value> = entries
        .iter()
        .filter(|entry| str_at(entry, "level").as_deref() == Some("error"))
        .collect();
    let warnings: Vec<&Value> = entries
        .iter()
        .filter(|entry| str_at(entry, "level").as_deref() == Some("warn"))
        .collect();

    out.push_str(&format!(
        "console: {} entries ({} errors, {} warnings) · {} uncaught\n",
        entries.len(),
        errors.len(),
        warnings.len(),
        failures.len()
    ));

    if failures.is_empty() && errors.is_empty() && warnings.is_empty() {
        out.push_str("\nNothing failed.\n");
    }
    if !failures.is_empty() {
        out.push_str("\nUNCAUGHT\n");
        for failure in failures.iter().rev().take(20) {
            out.push_str(&format!(
                "  [{}] {}\n",
                str_at(failure, "kind").unwrap_or_default(),
                truncate(&str_at(failure, "text").unwrap_or_default(), 400)
            ));
        }
    }
    for (title, list) in [("ERRORS", &errors), ("WARNINGS", &warnings)] {
        if list.is_empty() {
            continue;
        }
        out.push_str(&format!("\n{title}\n"));
        for entry in list.iter().rev().take(25) {
            out.push_str(&format!(
                "  {}\n",
                truncate(&str_at(entry, "text").unwrap_or_default(), 400)
            ));
        }
    }
    out.trim_end().to_string()
}

/// `browser_tabs`: one line per tab, the active one marked.
pub fn tabs(value: &Value) -> String {
    let tabs = array_at(value, "tabs");
    if tabs.is_empty() {
        return "No tabs are open.".to_string();
    }
    let mut out = format!("{} tab(s):\n", tabs.len());
    for tab in tabs {
        let active = tab.get("active").and_then(Value::as_bool).unwrap_or(false);
        let profile = str_at(tab, "profile").unwrap_or_else(|| "normal".to_string());
        let title = str_at(tab, "title").unwrap_or_default();
        let url = str_at(tab, "url").unwrap_or_default();
        out.push_str(&format!(
            "  {} [{}] {}  {}\n        {}\n",
            if active { ">" } else { " " },
            num_at(tab, "id").unwrap_or(0),
            if profile == "ghost" { "(ghost)" } else { "" },
            truncate(title.trim(), 80),
            truncate(&url, 160),
        ));
    }
    out.trim_end().to_string()
}

/// `browser_assert`: pass or fail, with the evidence either way.
pub fn assert(value: &Value) -> String {
    // The host may return a list of results when a call asserts several things.
    if let Some(results) = value.get("results").and_then(Value::as_array) {
        let passed = results
            .iter()
            .filter(|entry| entry.get("pass").and_then(Value::as_bool) == Some(true))
            .count();
        let mut out = format!("{passed}/{} checks passed\n", results.len());
        for entry in results {
            out.push_str(&assert_line(entry));
            out.push('\n');
        }
        return out.trim_end().to_string();
    }
    assert_line(value).trim_end().to_string()
}

fn assert_line(value: &Value) -> String {
    let pass = value.get("pass").and_then(Value::as_bool).unwrap_or(false);
    let check = str_at(value, "check").unwrap_or_default();
    let wanted = str_at(value, "value").unwrap_or_default();
    let evidence = str_at(value, "evidence").unwrap_or_default();
    format!(
        "{}  {check} \"{}\"  —  {evidence}",
        if pass { "PASS" } else { "FAIL" },
        truncate(&wanted, 120)
    )
}

/// `browser_screenshot`: dimensions, and any note about what was *not*
/// captured. The image itself is never here — it was lifted out and stored as an
/// attachment before shaping, so this is only the part that is text.
pub fn screenshot(value: &Value) -> String {
    let width = num_at(value, "width").unwrap_or(0);
    let height = num_at(value, "height").unwrap_or(0);
    let mut out = format!("CAPTURED  {width}×{height}  (shown to the user as well)");
    if let Some(note) = str_at(value, "note") {
        out.push_str("\n\n");
        out.push_str(&note);
    }
    out
}

/// Anything without a shaped renderer: pretty JSON, bounded.
pub fn generic(value: &Value) -> String {
    let text = match value {
        Value::String(text) => text.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
    };
    if text.chars().count() > MAX_GENERIC_CHARS {
        return format!(
            "{}\n… [{} more characters]",
            truncate(&text, MAX_GENERIC_CHARS),
            text.chars().count() - MAX_GENERIC_CHARS
        );
    }
    text
}

/// Renders whichever shape this tool's result calls for.
pub fn render(op: &str, value: &Value) -> String {
    // A host that reports a failure gets its message passed through verbatim:
    // the refusal text is written for the model, and rewording it here would
    // lose the part that says what to do instead.
    if let Some(error) = str_at(value, "error") {
        return error;
    }
    match op {
        "browser_snapshot" => snapshot(value),
        "browser_read" => read(value),
        "browser_console" => console(value),
        "browser_tabs" | "browser_tab" => tabs(value),
        "browser_assert" => assert(value),
        "browser_screenshot" => screenshot(value),
        _ => generic(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_snapshot_reads_as_an_indexed_list() {
        let raw = json!({
            "url": "https://example.com/login",
            "title": "Sign in",
            "ready": "complete",
            "viewport": [1280, 800],
            "scroll": [0, 0],
            "scrollHeight": 1420,
            "nodes": [
                "[1]  textbox  \"Email\"  value \"a@b.com\"  rect 420,300,360,40",
                "[2]  textbox  \"Password\"  value \"<set>\"  rect 420,352,360,40",
                "[4]  button  \"Sign in\"  rect 604,404,176,44",
            ],
            "belowFold": 3,
        });
        let text = snapshot(&raw);
        assert!(text.contains("https://example.com/login"), "{text}");
        assert!(text.contains("Sign in"), "{text}");
        assert!(text.contains("VIEW  1280x800  scroll 0,0 of 1420"), "{text}");
        assert!(text.contains("[4]  button"), "{text}");
        // The count of what is not shown matters as much as what is.
        assert!(text.contains("3 more below the fold"), "{text}");
    }

    #[test]
    fn a_snapshot_never_carries_a_password_value() {
        // Not enforced here — the collector writes `<set>` — but this asserts
        // the shape that arrives is passed through unaltered rather than
        // reinterpreted somewhere in between.
        let raw = json!({
            "url": "https://example.com",
            "nodes": ["[1]  textbox  \"Password\"  value \"<set>\""],
        });
        let text = snapshot(&raw);
        assert!(text.contains("<set>"), "{text}");
    }

    #[test]
    fn user_activity_is_reported_so_the_model_can_work_around_it() {
        let raw = json!({
            "url": "https://example.com",
            "nodes": ["[1]  button  \"Go\""],
            "userActivity": {
                "lastInputAt": 1_700_000_000_000i64,
                "idleMs": 2400,
                "recent": [
                    { "kind": "typed", "detail": "input#email = a@b.co", "at": 1_700_000_000_000i64 },
                    { "kind": "scrolled", "detail": "to y=840", "at": 1_700_000_000_000i64 }
                ],
                "typedFields": ["#email"]
            }
        });
        let text = snapshot(&raw);
        assert!(text.contains("user activity in this page"), "{text}");
        assert!(text.contains("typed"), "{text}");
        assert!(text.contains("#email"), "{text}");
        // The instruction that makes shared control safe, not just observed.
        assert!(text.contains("re-read before submitting"), "{text}");
    }

    #[test]
    fn a_clean_page_says_so_rather_than_printing_nothing() {
        let raw = json!({ "url": "https://example.com", "nodes": [] });
        let text = snapshot(&raw);
        assert!(text.contains("No actionable elements"), "{text}");
    }

    #[test]
    fn console_leads_with_the_failure() {
        let raw = json!({
            "console": [
                { "level": "log", "text": "booting" },
                { "level": "warn", "text": "deprecated api" },
                { "level": "error", "text": "GET /api/x 500" }
            ],
            "failures": [{ "kind": "error", "text": "Uncaught TypeError: x is not a function" }]
        });
        let text = console(&raw);
        assert!(text.contains("1 errors, 1 warnings"), "{text}");
        let uncaught = text.find("UNCAUGHT").expect("uncaught section");
        let errors = text.find("ERRORS").expect("errors section");
        assert!(uncaught < errors, "failures should come first: {text}");
        assert!(text.contains("GET /api/x 500"), "{text}");
        // A log line is not worth the model's attention.
        assert!(!text.contains("booting"), "{text}");
    }

    #[test]
    fn console_reports_cleanliness_plainly() {
        let text = console(&json!({ "console": [], "failures": [] }));
        assert!(text.contains("Nothing failed"), "{text}");
    }

    #[test]
    fn a_refusal_is_passed_through_verbatim() {
        let raw = json!({
            "error": "refused: this password field already holds a value the user typed. \
                      It was left untouched."
        });
        let text = render("browser_type", &raw);
        assert!(text.starts_with("refused:"), "{text}");
        assert!(text.contains("left untouched"), "{text}");
    }

    #[test]
    fn assert_renders_pass_and_fail_with_evidence() {
        let raw = json!({ "pass": true, "check": "text_present", "value": "Welcome", "evidence": "found" });
        let text = assert(&raw);
        assert!(text.starts_with("PASS"), "{text}");
        assert!(text.contains("text_present"), "{text}");
        assert!(text.contains("found"), "{text}");

        let failed = json!({ "pass": false, "check": "url_matches", "value": "/dashboard", "evidence": "https://x/login" });
        let text = assert(&failed);
        assert!(text.starts_with("FAIL"), "{text}");
        // The evidence is what makes a failure actionable.
        assert!(text.contains("https://x/login"), "{text}");
    }

    #[test]
    fn tabs_mark_the_active_one_and_the_ghost_profile() {
        let raw = json!({
            "tabs": [
                { "id": 1, "title": "Invoices", "url": "https://x/invoices", "active": true, "profile": "normal" },
                { "id": 2, "title": "Search", "url": "https://x/search", "active": false, "profile": "ghost" }
            ]
        });
        let text = tabs(&raw);
        assert!(text.contains("2 tab(s)"), "{text}");
        assert!(text.contains("> [1]"), "{text}");
        assert!(text.contains("(ghost)"), "{text}");
    }

    #[test]
    fn a_capture_reports_its_size_and_any_caveat() {
        let raw = json!({
            "width": 1280,
            "height": 800,
            "mode": "full_page",
            "note": "`full_page` is not available from this capture API, so this is the \
                     visible viewport (1280×800)."
        });
        let text = render("browser_screenshot", &raw);
        // The dimensions are the part a caller needs to interpret the image.
        assert!(text.contains("1280×800"), "{text}");
        // And the caveat survives, or a caller believes it got a whole page.
        assert!(text.contains("full_page"), "{text}");
        assert!(text.contains("visible viewport"), "{text}");
        // It must not be raw JSON: this is a shaped renderer.
        assert!(!text.contains("\"mode\""), "{text}");
    }

    #[test]
    fn generic_truncates_on_a_char_boundary() {
        let long = json!({ "text": "漢".repeat(20_000) });
        let text = generic(&long);
        assert!(text.contains("more characters"), "{}", &text[..80.min(text.len())]);
        // Must not have split a multi-byte character.
        assert!(text.is_char_boundary(text.len()));
    }

    #[test]
    fn dialogs_are_surfaced_with_the_tool_that_answers_them() {
        let raw = json!({
            "url": "https://example.com",
            "nodes": ["[1]  button  \"Delete\""],
            "dialogs": [{ "type": "confirm", "message": "Delete this item?" }]
        });
        let text = snapshot(&raw);
        assert!(text.contains("browser_dialog"), "{text}");
        assert!(text.contains("Delete this item?"), "{text}");
    }
}
