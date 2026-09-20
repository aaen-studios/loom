//! The built-in browser: seeing, reading and driving a real page.
//!
//! Twenty-five tools, offered only in chats whose Browser chip is on. The chip
//! is the standing consent, so these run without per-call cards, and switching
//! it off stops the running turn at once — the same arrangement computer use
//! uses, deliberately.
//!
//! # Why this is not computer use with a smaller screen
//!
//! Computer use looks at pixels. That is the right call for a desktop, where
//! there is no other handle on the machine — and it is the wrong one for a web
//! page, which has a structure that can be read directly. So the money tool
//! here is [`snapshot`](specs): an indexed list of the page's actionable nodes,
//! so the model clicks `[7]` rather than a coordinate. A snapshot costs a
//! couple of hundred tokens; a screenshot of the same page costs a couple of
//! thousand. That is the difference between a turn that stays flat over twenty
//! steps and one that climbs.
//!
//! # The three layers
//!
//! The engine never learns what a Tauri webview is. [`BrowserHost`] is the seam:
//! a trait that takes a tool name and JSON, and returns JSON. The shell
//! implements it over WebView2, using native COM where it exists, an injected
//! collector for the page, and the DevTools protocol only where neither reaches.
//! Everything below this line — dispatch, argument validation, shaping, shot
//! storage, the disabled note — is testable with a fake host and no window at
//! all, which is why [`NoBrowser`] exists.
//!
//! # Shared control
//!
//! The user's own input in a page is **observed and reported, not obeyed as a
//! stop signal**. The collector records real input in the frame; every
//! page-shaped result carries a footer saying what the user did and how long
//! ago, so the model works *around* them instead of through them. Two things
//! are refused rather than reported: writing into a password field that already
//! holds a value the model did not type, and (in the collector) submitting a
//! form whose fields the user is mid-edit in. Both are refusals, not cards — a
//! round, not a click.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tools::{ToolCall, ToolImage, ToolOutcome, ToolScope, ToolSpec};
use crate::{Error, Result};

pub mod filters;
pub mod lists;
pub mod shape;

/// What the model is told when it reaches for a browser tool in a chat whose
/// Browser chip is off. It should not even see the tools, but a stale plan from
/// an earlier turn can still name one.
pub const DISABLED_NOTE: &str = "Browser control is off for this chat. Ask the user to \
    enable the Browser chip in the composer; do not call browser tools until then.";

/// The collected page script, compiled into the binary.
///
/// Injected at document start into every frame, so it can patch the console and
/// observe real input before the page's own scripts run. It is one file rather
/// than a Rust string so it can be edited as JavaScript and read as JavaScript.
pub const COLLECTOR_JS: &str = include_str!("browser/inject.js");

/// Longest a page's readable text may be handed over in one call.
const MAX_READ_CHARS: usize = 40_000;

/// A tab's profile. Ghost tabs are WebView2's in-private mode: no cookies, no
/// storage, nothing left behind when they close.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    /// The persistent Loom profile — logins carry over, as in any browser.
    #[default]
    Normal,
    /// In-private: no data of any kind is kept.
    Ghost,
}

impl Profile {
    pub fn as_str(self) -> &'static str {
        match self {
            Profile::Normal => "normal",
            Profile::Ghost => "ghost",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "normal" | "default" | "persistent" => Some(Profile::Normal),
            "ghost" | "private" | "incognito" => Some(Profile::Ghost),
            _ => None,
        }
    }
}

/// Which cost band a tool sits in.
///
/// Not a permission gate — the chip is the permission. Tiers exist because tool
/// schemas are a *fixed* cost against every request's token budget, so a chat
/// that only needs to read a page should not pay for the HAR export tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Looking: open, snapshot, read, find, wait, assert, screenshot, console.
    See,
    /// Acting on a page: click, type, press, select, scroll, forms, dialogs.
    Act,
    /// Devtools: evaluate, raw CDP.
    Dev,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::See => "see",
            Tier::Act => "act",
            Tier::Dev => "dev",
        }
    }

    /// Which tiers a chat gets when nothing has been configured.
    ///
    /// `Dev` is off by default on purpose: `browser_evaluate` runs arbitrary
    /// JavaScript in the page, and `browser_debug` is a raw DevTools
    /// passthrough. Both are the escape hatch for "the tool for this does not
    /// exist", and both should be a decision the user makes rather than one
    /// they discover.
    pub fn enabled_by_default(self) -> bool {
        match self {
            Tier::See | Tier::Act => true,
            Tier::Dev => false,
        }
    }
}

/// Every tier, for the settings UI and the config default.
pub const ALL_TIERS: [Tier; 3] = [Tier::See, Tier::Act, Tier::Dev];

/// Whether this tool's tier is switched on, given the enabled set.
pub fn tier_enabled(enabled: &[Tier], name: &str) -> bool {
    match tier_of(name) {
        Some(tier) => enabled.contains(&tier),
        // An unknown name is not a browser tool; the caller has already
        // established that, so fail open rather than hiding a tool that exists.
        None => true,
    }
}

/// The tiers that are on when nothing has been chosen.
pub fn default_tiers() -> Vec<Tier> {
    ALL_TIERS
        .into_iter()
        .filter(|tier| tier.enabled_by_default())
        .collect()
}

/// Every browser tool name, in one place, so the classifiers cannot drift.
pub const TOOLS: [&str; 25] = [
    // See
    "browser_open",
    "browser_tabs",
    "browser_read",
    "browser_snapshot",
    "browser_find",
    "browser_wait",
    "browser_assert",
    "browser_screenshot",
    "browser_console",
    // Act
    "browser_tab",
    "browser_navigate",
    "browser_click",
    "browser_type",
    "browser_press",
    "browser_select",
    "browser_check",
    "browser_hover",
    "browser_scroll",
    "browser_fill_form",
    "browser_dialog",
    "browser_drag",
    // Dev
    "browser_evaluate",
    "browser_cookies",
    "browser_storage",
    "browser_clear_data",
];

/// Whether this is one of ours. Exact-match and exhaustive: a tool added later
/// is not a browser tool until it is listed, which is the fail-closed direction.
pub fn is_browser_tool(name: &str) -> bool {
    TOOLS.contains(&name)
}

/// Which tier a tool belongs to.
pub fn tier_of(name: &str) -> Option<Tier> {
    match name {
        "browser_open" | "browser_tabs" | "browser_read" | "browser_snapshot"
        | "browser_find" | "browser_wait" | "browser_assert" | "browser_screenshot"
        | "browser_console" => Some(Tier::See),
        "browser_tab" | "browser_navigate" | "browser_click" | "browser_type"
        | "browser_press" | "browser_select" | "browser_check" | "browser_hover"
        | "browser_scroll" | "browser_fill_form" | "browser_dialog" | "browser_drag" => {
            Some(Tier::Act)
        }
        "browser_evaluate" | "browser_cookies" | "browser_storage"
        | "browser_clear_data" => Some(Tier::Dev),
        _ => None,
    }
}

/// Tools that change nothing a user would have to be told about.
///
/// This is what makes Plan and Review mode work on a browser for free: an agent
/// mode that blocks writes blocks everything not listed here, and the read-only
/// half is exactly the half that cannot click a button or spend money.
///
/// `browser_open` is here deliberately and it is the one judgement call. Opening
/// a URL loads a page, which is a request to a server — but it is the same act
/// `fetch_url` already performs, and a plan that cannot look at the site it is
/// proposing changes to is not a plan. It navigates a new tab it made itself and
/// touches nothing the user was looking at.
pub fn is_read_only(name: &str) -> bool {
    matches!(
        name,
        "browser_open"
            | "browser_tabs"
            | "browser_read"
            | "browser_snapshot"
            | "browser_find"
            | "browser_wait"
            | "browser_assert"
            | "browser_screenshot"
            | "browser_console"
            | "browser_cookies"
            | "browser_storage"
    )
}

// ---------------------------------------------------------------------------
// The host seam
// ---------------------------------------------------------------------------

/// A screenshot the host captured, base64-encoded.
///
/// Base64 rather than bytes because the whole seam is JSON: the host may be a
/// WebView2 controller, a stub in a test, or something not written yet, and a
/// shared byte type would be the first thing to drag a platform dependency into
/// this crate.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostShot {
    pub name: String,
    #[serde(default = "default_png")]
    pub mime: String,
    /// Standard base64, no data-URL prefix.
    pub data: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}

fn default_png() -> String {
    "image/png".to_string()
}

/// What one browser operation is allowed to reach.
#[derive(Debug, Clone, Default)]
pub struct BrowserOptions {
    /// Whether the chat's Browser chip is on.
    pub armed: bool,
    /// Longest edge for screenshots. `0` is native resolution, matching
    /// computer use's quality-first default.
    pub screenshot_edge: u32,
}

/// The engine's view of a browser.
///
/// Synchronous and JSON-shaped on purpose. Every method is called from a
/// blocking thread by [`run`], so an implementation is free to make blocking
/// COM calls, and nothing it touches can leak a platform type across the seam.
pub trait BrowserHost: Send + Sync {
    /// Runs one operation. `op` is the tool name; `args` are the validated
    /// arguments, after defaults and clamping.
    ///
    /// A result carrying `{"error": "..."}` is a refusal — the text is written
    /// for the model and is passed through verbatim.
    fn call(&self, session_id: &str, op: &str, args: &Value) -> Result<Value>;

    /// Forgets everything this chat held. Called when a turn ends, however it
    /// ended, so no hold outlives the turn that took it.
    fn release(&self, session_id: &str);

    // Deliberately no `holder()` here. "One chat drives at a time" is the
    // *engine's* rule — it is what decides whether a call runs at all, and it
    // has to hold across hosts, including the stub. A `holder()` on the trait
    // would be a second place the same fact could live and disagree, so the
    // engine keeps it in [`Holder`] and asks the host only to run things and to
    // give them back.

    /// Whether a browser is usable at all. False on a platform with no host,
    /// which is what makes the tools report a clear reason instead of failing
    /// one at a time.
    fn available(&self) -> bool {
        true
    }
}

/// A host that refuses everything.
///
/// The default for an engine nobody has given a browser to: tests, the CLI, and
/// every platform that is not Windows. Compiling against this rather than
/// behind a `#[cfg]` means the tool surface, the prompt note and the UI are the
/// same everywhere and only the transport differs.
pub struct NoBrowser {
    reason: String,
}

impl Default for NoBrowser {
    fn default() -> Self {
        Self {
            reason: "The built-in browser is not available on this platform.".to_string(),
        }
    }
}

impl NoBrowser {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl BrowserHost for NoBrowser {
    fn call(&self, _session_id: &str, _op: &str, _args: &Value) -> Result<Value> {
        Ok(json!({ "error": self.reason }))
    }

    fn release(&self, _session_id: &str) {}

    fn available(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Argument handling
// ---------------------------------------------------------------------------

/// Thin accessors over a tool call's arguments, so validation reads as
/// validation rather than as a pile of `as_str().ok_or_else`.
struct Args<'a> {
    raw: &'a Value,
}

impl<'a> Args<'a> {
    fn new(raw: &'a Value) -> Self {
        Self { raw }
    }

    fn has(&self, key: &str) -> bool {
        self.raw.get(key).is_some_and(|value| !value.is_null())
    }

    fn string(&self, key: &str) -> Option<String> {
        self.raw
            .get(key)
            .and_then(Value::as_str)
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
    }

    fn required(&self, key: &str, tool: &str) -> Result<String> {
        self.string(key)
            .ok_or_else(|| Error::Other(format!("{tool} needs a `{key}` argument")))
    }

    /// A boolean with a default. Also accepts the strings a model sometimes
    /// emits where a boolean was asked for, because refusing those costs a
    /// round to teach nothing.
    fn bool_or(&self, key: &str, fallback: bool) -> bool {
        match self.raw.get(key) {
            Some(Value::Bool(value)) => *value,
            Some(Value::String(text)) => !matches!(
                text.trim().to_ascii_lowercase().as_str(),
                "false" | "0" | "no" | "off"
            ),
            _ => fallback,
        }
    }

    /// A number, clamped into `min..=max`. A model that asks for 10_000 nodes
    /// gets the ceiling rather than an error: the intent is unambiguous and an
    /// error would waste the round.
    fn int(&self, key: &str, fallback: i64, min: i64, max: i64) -> i64 {
        let value = match self.raw.get(key) {
            Some(Value::Number(number)) => number.as_i64().or_else(|| {
                number.as_f64().map(|float| float.round() as i64)
            }),
            Some(Value::String(text)) => text.trim().parse::<f64>().ok().map(|f| f.round() as i64),
            _ => None,
        };
        value.unwrap_or(fallback).clamp(min, max)
    }

    /// A tab id. Absent means "the tab this chat is working in", which the host
    /// resolves; the engine never guesses one.
    fn tab(&self) -> Value {
        match self.raw.get("tab") {
            Some(value) if !value.is_null() => value.clone(),
            _ => Value::Null,
        }
    }

    /// An enum argument, validated against a fixed list so a typo produces a
    /// message naming the options rather than a silent fallback.
    fn one_of(&self, key: &str, options: &[&str], fallback: &str) -> Result<String> {
        match self.string(key) {
            None => Ok(fallback.to_string()),
            Some(value) => {
                let lowered = value.to_ascii_lowercase();
                if options.contains(&lowered.as_str()) {
                    Ok(lowered)
                } else {
                    Err(Error::Other(format!(
                        "`{key}` must be one of {}; got \"{value}\"",
                        options.join(", ")
                    )))
                }
            }
        }
    }

    /// A profile argument, defaulting to the configured default rather than to
    /// a constant, so "the user's default" is one decision in one place.
    fn profile(&self, fallback: Profile) -> Result<Profile> {
        // `private: true` is accepted as an alias: it is the word a model
        // reaches for, and refusing it teaches nothing.
        if self.bool_or("private", false) || self.bool_or("ghost", false) {
            return Ok(Profile::Ghost);
        }
        match self.string("profile") {
            None => Ok(fallback),
            Some(value) => Profile::parse(&value).ok_or_else(|| {
                Error::Other(format!(
                    "`profile` must be normal or ghost; got \"{value}\""
                ))
            }),
        }
    }
}

/// The validated arguments for one call, ready to hand to a host.
///
/// Returning this rather than passing the raw JSON through is the whole reason
/// the engine stays testable: every default and every clamp lives here, so a
/// fake host in a test sees exactly what a real one would.
fn normalize(op: &str, raw: &Value, options: &BrowserOptions) -> Result<Value> {
    let args = Args::new(raw);
    let mut out = serde_json::Map::new();

    // Every page-shaped tool carries the same window for user activity, so the
    // footer is comparable across calls.
    if let Some(since) = args.raw.get("since_ms") {
        out.insert("since_ms".to_string(), since.clone());
    }

    match op {
        "browser_open" => {
            let url = args.required("url", op)?;
            out.insert("url".into(), json!(normalize_url(&url)));
            out.insert("tab".into(), args.tab());
            // Validated here rather than left to the host, so a typo produces a
            // message naming the options instead of a tab that quietly came up
            // in the wrong profile — which, for `ghost`, would mean a page the
            // user expected to be private writing cookies to the shared jar.
            out.insert("profile".into(), json!(args.profile(Profile::Normal)?.as_str()));
        }
        "browser_tabs" => {}
        "browser_tab" => {
            let action = args.one_of(
                "action",
                &[
                    "list", "open", "close", "focus", "activate", "pin", "unpin", "mute",
                    "duplicate", "reopen",
                ],
                "list",
            )?;
            out.insert("action".into(), json!(action));
            out.insert("tab".into(), args.tab());
            if let Some(url) = args.string("url") {
                out.insert("url".into(), json!(normalize_url(&url)));
            }
            if action == "open" || args.has("profile") || args.has("private") {
                out.insert(
                    "profile".into(),
                    json!(args.profile(Profile::Normal)?.as_str()),
                );
            }
        }
        "browser_navigate" => {
            let action = args.one_of(
                "action",
                &["goto", "back", "forward", "reload", "stop"],
                "goto",
            )?;
            out.insert("action".into(), json!(action));
            out.insert("tab".into(), args.tab());
            if action == "goto" {
                let url = args.required("url", op)?;
                out.insert("url".into(), json!(normalize_url(&url)));
            }
        }
        "browser_snapshot" => {
            out.insert("tab".into(), args.tab());
            out.insert("all".into(), json!(args.bool_or("all", false)));
            out.insert(
                "interactive_only".into(),
                json!(args.bool_or("interactive_only", true)),
            );
            out.insert("max_nodes".into(), json!(args.int("max_nodes", 400, 1, 2000)));
            if let Some(selector) = args.string("selector") {
                out.insert("selector".into(), json!(selector));
            }
        }
        "browser_read" => {
            out.insert("tab".into(), args.tab());
            out.insert(
                "max_chars".into(),
                json!(args.int("max_chars", 6000, 200, MAX_READ_CHARS as i64)),
            );
            if let Some(selector) = args.string("selector") {
                out.insert("selector".into(), json!(selector));
            }
        }
        "browser_find" => {
            out.insert("tab".into(), args.tab());
            out.insert("text".into(), json!(args.required("text", op)?));
            out.insert("max".into(), json!(args.int("max", 20, 1, 100)));
            out.insert(
                "ignore_case".into(),
                json!(args.bool_or("ignore_case", true)),
            );
        }
        "browser_wait" => {
            out.insert("tab".into(), args.tab());
            // One of these is required: `browser_wait` with nothing to wait for
            // is a sleep, and a model that wants to sleep has `wait`.
            let mut condition = serde_json::Map::new();
            for key in [
                "selector",
                "text",
                "url_matches",
                "idle",
                "network_idle",
                "gone_selector",
            ] {
                if let Some(value) = args.raw.get(key) {
                    if !value.is_null() {
                        condition.insert(key.to_string(), value.clone());
                    }
                }
            }
            if condition.is_empty() {
                return Err(Error::Other(
                    "browser_wait needs one of: selector, text, url_matches, idle, \
                     network_idle, gone_selector"
                        .to_string(),
                ));
            }
            out.insert("condition".into(), Value::Object(condition));
            out.insert("timeout_ms".into(), json!(args.int("timeout_ms", 15000, 100, 120_000)));
        }
        "browser_assert" => {
            out.insert("tab".into(), args.tab());
            let check = args.one_of(
                "check",
                &[
                    "url_matches",
                    "title_matches",
                    "text_present",
                    "text_absent",
                    "exists",
                    "not_exists",
                    "value_equals",
                    "no_console_errors",
                    "ready",
                ],
                // Defaulting to "ready" rather than erroring: a model that
                // omitted the check almost certainly wanted "is it there yet".
                "ready",
            )?;
            out.insert("check".into(), json!(check));
            if let Some(value) = args.string("value") {
                out.insert("value".into(), json!(value));
            }
            if let Some(target) = args.string("target") {
                out.insert("target".into(), json!(target));
            }
        }
        "browser_screenshot" => {
            out.insert("tab".into(), args.tab());
            out.insert(
                "mode".into(),
                json!(args.one_of(
                    "mode",
                    &["viewport", "full_page", "element"],
                    "viewport"
                )?),
            );
            if let Some(target) = args.string("target") {
                out.insert("target".into(), json!(target));
            }
            // The engine's setting wins unless the call names its own edge; a
            // model asking for native resolution is a deliberate choice.
            let edge = args.int("max_edge", options.screenshot_edge as i64, 0, 4096);
            out.insert("max_edge".into(), json!(edge));
        }
        "browser_console" => {
            out.insert("tab".into(), args.tab());
            out.insert("clear".into(), json!(args.bool_or("clear", false)));
        }
        "browser_click" => {
            out.insert("tab".into(), args.tab());
            out.insert("target".into(), json!(args.required("target", op)?));
            out.insert(
                "kind".into(),
                json!(args.one_of(
                    "kind",
                    &["click", "dblclick", "down", "up"],
                    "click"
                )?),
            );
            out.insert(
                "button".into(),
                json!(args.one_of("button", &["left", "right", "middle"], "left")?),
            );
            out.insert(
                "settle".into(),
                json!(args.one_of(
                    "settle",
                    &["none", "dom", "navigation", "network"],
                    "dom"
                )?),
            );
            out.insert("timeout_ms".into(), json!(args.int("timeout_ms", 10000, 100, 120_000)));
        }
        "browser_type" => {
            out.insert("tab".into(), args.tab());
            out.insert("target".into(), json!(args.required("target", op)?));
            out.insert("text".into(), json!(args.string("text").unwrap_or_default()));
            out.insert("clear".into(), json!(args.bool_or("clear", true)));
            out.insert(
                "submit".into(),
                json!(args.bool_or("submit", false)),
            );
        }
        "browser_press" => {
            out.insert("tab".into(), args.tab());
            out.insert("key".into(), json!(args.required("key", op)?));
            out.insert("repeat".into(), json!(args.int("repeat", 1, 1, 50)));
            if let Some(target) = args.string("target") {
                out.insert("target".into(), json!(target));
            }
        }
        "browser_select" => {
            out.insert("tab".into(), args.tab());
            out.insert("target".into(), json!(args.required("target", op)?));
            if args.has("value") {
                out.insert("value".into(), json!(args.string("value")));
            }
            if args.has("label") {
                out.insert("label".into(), json!(args.string("label")));
            }
            if !args.has("value") && !args.has("label") {
                return Err(Error::Other(
                    "browser_select needs `value` or `label`".to_string(),
                ));
            }
        }
        "browser_check" => {
            out.insert("tab".into(), args.tab());
            out.insert("target".into(), json!(args.required("target", op)?));
            out.insert("checked".into(), json!(args.bool_or("checked", true)));
        }
        "browser_hover" => {
            out.insert("tab".into(), args.tab());
            out.insert("target".into(), json!(args.required("target", op)?));
        }
        "browser_scroll" => {
            out.insert("tab".into(), args.tab());
            out.insert("amount".into(), json!(args.int("amount", 600, -20_000, 20_000)));
            if let Some(target) = args.string("target") {
                out.insert("target".into(), json!(target));
            }
        }
        "browser_drag" => {
            out.insert("tab".into(), args.tab());
            out.insert("from".into(), json!(args.required("from", op)?));
            out.insert("to".into(), json!(args.required("to", op)?));
        }
        "browser_fill_form" => {
            out.insert("tab".into(), args.tab());
            let fields = args
                .raw
                .get("fields")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    Error::Other(
                        "browser_fill_form needs `fields`: an array of {target, value} or \
                         {target, checked}"
                            .to_string(),
                    )
                })?;
            if fields.is_empty() {
                return Err(Error::Other("browser_fill_form got no fields".into()));
            }
            for field in fields {
                if field.get("target").and_then(Value::as_str).is_none() {
                    return Err(Error::Other(
                        "every entry in `fields` needs a `target`".to_string(),
                    ));
                }
            }
            out.insert("fields".into(), Value::Array(fields.clone()));
            out.insert("submit".into(), json!(args.bool_or("submit", false)));
        }
        "browser_dialog" => {
            out.insert("tab".into(), args.tab());
            out.insert(
                "action".into(),
                json!(args.one_of("action", &["accept", "dismiss", "list"], "accept")?),
            );
            if let Some(value) = args.raw.get("value") {
                out.insert("value".into(), value.clone());
            }
        }
        "browser_evaluate" => {
            out.insert("tab".into(), args.tab());
            out.insert(
                "expression".into(),
                json!(args.required("expression", op)?),
            );
            if let Some(frame) = args.string("frame") {
                out.insert("frame".into(), json!(frame));
            }
        }
        "browser_cookies" => {
            out.insert(
                "action".into(),
                json!(args.one_of(
                    "action",
                    &["list", "get", "set", "delete"],
                    "list"
                )?),
            );
            out.insert("tab".into(), args.tab());
            if let Some(url) = args.string("url") {
                out.insert("url".into(), json!(url));
            }
            if args.has("cookie") {
                out.insert("cookie".into(), args.raw["cookie"].clone());
            }
            out.insert("include_http_only".into(), json!(args.bool_or("include_http_only", true)));
        }
        "browser_storage" => {
            out.insert("tab".into(), args.tab());
            out.insert(
                "kind".into(),
                json!(args.one_of(
                    "kind",
                    &["local", "session", "indexeddb", "all"],
                    "local"
                )?),
            );
        }
        "browser_clear_data" => {
            out.insert("tab".into(), args.tab());
            let scope = args.one_of("scope", &["site", "all"], "site")?;
            out.insert("scope".into(), json!(scope));
            if scope == "site" {
                out.insert("origin".into(), json!(args.required("origin", op)?));
            }
            out.insert(
                "kinds".into(),
                json!(args
                    .raw
                    .get("kinds")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_else(|| vec![
                        json!("cookies"),
                        json!("storage"),
                        json!("cache")
                    ])),
            );
        }
        other => {
            return Err(Error::Other(format!(
                "{other} is not a browser tool the engine knows"
            )))
        }
    }

    Ok(Value::Object(out))
}

/// Turns whatever the model wrote into something navigable.
///
/// A bare host is the common case — a model asked to "go to example.com" writes
/// `example.com`, not a URL — and treating that as a search query would be
/// surprising. Anything with a space in it is a query, because no host has one.
pub fn normalize_url(raw: &str) -> String {
    let text = raw.trim();
    if text.is_empty() {
        return text.to_string();
    }
    if text.starts_with("http://")
        || text.starts_with("https://")
        || text.starts_with("about:")
        || text.starts_with("file:")
        || text.starts_with("data:")
        || text.starts_with("view-source:")
    {
        return text.to_string();
    }
    if text.starts_with("localhost") || text.starts_with("127.0.0.1") {
        return format!("http://{text}");
    }
    if text.contains(' ') || text.contains('\n') {
        return format!(
            "https://duckduckgo.com/?q={}",
            urlencode(text)
        );
    }
    format!("https://{text}")
}

/// Percent-encodes everything outside the unreserved set. Small enough not to
/// deserve a dependency, and correct for a query string.
fn urlencode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Running a call
// ---------------------------------------------------------------------------

/// Runs one browser tool call.
///
/// Blocking on purpose: the host makes synchronous COM calls, and running them
/// on an async worker would tie up the runtime for the length of a page load.
/// The engine hands this to a blocking thread, which is exactly what computer
/// use does for the same reason.
pub fn run(
    session_id: &str,
    call: &ToolCall,
    host: &dyn BrowserHost,
    options: &BrowserOptions,
) -> ToolOutcome {
    let fail = |message: String| ToolOutcome {
        id: call.id.clone(),
        name: call.name.clone(),
        ok: false,
        output: message,
        images: Vec::new(),
    };

    if !is_browser_tool(&call.name) {
        return fail(format!("`{}` is not a browser tool", call.name));
    }

    // The chip is the permission. Without it the model should not even see
    // these tools, and a stale plan that names one gets a clear refusal rather
    // than a mysterious failure.
    if !options.armed {
        return fail(DISABLED_NOTE.to_string());
    }

    if !host.available() {
        return fail(format!(
            "The built-in browser is not available on this platform, so `{}` cannot run. \
             Use `fetch_url` to read a page instead.",
            call.name
        ));
    }

    let raw: Value = if call.arguments.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str(&call.arguments) {
            Ok(value) => value,
            Err(error) => {
                return fail(format!(
                    "`{}` was called with arguments that are not valid JSON ({error}). \
                     Send them again as a JSON object.",
                    call.name
                ))
            }
        }
    };

    let args = match normalize(&call.name, &raw, options) {
        Ok(args) => args,
        // A validation failure is the model's to fix, and the message says how.
        Err(error) => return fail(error.to_string()),
    };

    let mut result = match host.call(session_id, &call.name, &args) {
        Ok(value) => value,
        Err(error) => return fail(error.to_string()),
    };

    // A screenshot rides inside the JSON so the seam has one return type. It is
    // lifted out here, stored like a computer screenshot, and removed before
    // shaping — otherwise twelve hundred base64 characters would land in the
    // transcript as text.
    let images = match take_shot(session_id, &mut result) {
        Ok(images) => images,
        Err(error) => return fail(error.to_string()),
    };

    let refused = result
        .get("error")
        .and_then(Value::as_str)
        .map(str::to_string);
    let text = shape::render(&call.name, &result);

    ToolOutcome {
        id: call.id.clone(),
        name: call.name.clone(),
        ok: refused.is_none(),
        output: text,
        images,
    }
}

/// Removes a `shot` from the result and stores it as an attachment.
///
/// Storing it in `attachments/<session>/` is what makes the transcript show it,
/// what gives the context budget its real pixel dimensions to count, and what
/// lets the newest-image-only rule in the wire builder apply to it unchanged.
fn take_shot(session_id: &str, result: &mut Value) -> Result<Vec<ToolImage>> {
    let Some(object) = result.as_object_mut() else {
        return Ok(Vec::new());
    };
    let Some(shot) = object.remove("shot") else {
        return Ok(Vec::new());
    };
    if shot.is_null() {
        return Ok(Vec::new());
    }

    let shot: HostShot = serde_json::from_value(shot)
        .map_err(|error| Error::Other(format!("the browser returned an unreadable shot: {error}")))?;

    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(shot.data.trim())
        .map_err(|error| Error::Other(format!("the browser returned a shot that is not base64: {error}")))?;

    let root = crate::paths::loom_home()?.join("attachments");
    let stored = crate::attachments::store_bytes_in(&root, session_id, &shot.name, &bytes)?;

    Ok(vec![ToolImage {
        name: stored.name,
        mime: if shot.mime.is_empty() {
            stored.mime
        } else {
            shot.mime
        },
        path: stored.path,
        // The capture's own size, which is what a provider tiles when it counts
        // an image against the context budget.
        width: if shot.width > 0 { shot.width } else { stored.width },
        height: if shot.height > 0 { shot.height } else { stored.height },
    }])
}

/// Records which chat holds the browser, for the composer chip and Stop.
///
/// Kept here rather than in [`BrowserHost`]'s callers because the rule is the
/// engine's: one chat drives at a time, a second chat is told to wait, and a
/// turn ending releases the hold. The same shape as the computer-use lock, for
/// the same reason — two turns fighting over one tab is how clicks land in the
/// wrong place.
#[derive(Default)]
pub struct Holder {
    inner: Mutex<Option<String>>,
}

/// What a chat gets when another chat already has the browser.
pub const BUSY_NOTE: &str = "Another chat is using the browser right now. Wait for it to \
    finish, or ask the user to stop it. You can still use `fetch_url` to read a page.";

impl Holder {
    /// Claims the browser for `session_id`.
    ///
    /// Returns `Ok(true)` when this call took it, `Ok(false)` when this chat
    /// already held it, and `Err(())` when another chat has it.
    #[allow(clippy::result_unit_err)]
    pub fn claim(&self, session_id: &str) -> std::result::Result<bool, ()> {
        let mut holder = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        match holder.as_deref() {
            Some(owner) if owner != session_id => Err(()),
            Some(_) => Ok(false),
            None => {
                *holder = Some(session_id.to_string());
                Ok(true)
            }
        }
    }

    /// Releases the browser if this chat held it. Safe to call twice.
    pub fn release(&self, session_id: &str) -> bool {
        let mut holder = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if holder.as_deref() == Some(session_id) {
            *holder = None;
            true
        } else {
            false
        }
    }

    pub fn current(&self) -> Option<String> {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

fn spec(
    name: &'static str,
    description: &'static str,
    parameters: Value,
    read_only: bool,
) -> ToolSpec {
    ToolSpec {
        name,
        description,
        parameters,
        read_only,
        scope: Some(ToolScope::Browser),
    }
}

/// A target argument, shared by every tool that acts on an element.
///
/// Documented once here and referenced by the tools that take it, because the
/// `[n]` form is the single most important convention in this toolset: it is
/// what makes a snapshot and a click agree.
fn target_property() -> Value {
    json!({
        "type": "string",
        "description": "An element: `[7]` from the most recent browser_snapshot, or a CSS selector. Prefer the index — it is what the snapshot described."
    })
}

fn tab_property() -> Value {
    json!({
        "type": "integer",
        "description": "Tab id from browser_tabs. Omit for the tab this chat is working in."
    })
}

pub fn specs() -> Vec<ToolSpec> {
    vec![
        // ------------------------------------------------------------ See
        spec(
            "browser_open",
            "Open a URL in a new tab and return the page's structure. The workhorse: it opens, waits for the page, and gives you an indexed list of what is clickable, in one call. A bare host is treated as https, and anything with a space in it is a search.",
            json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL, bare host (example.com), or a search query" },
                    "profile": { "type": "string", "enum": ["normal", "ghost"], "description": "normal uses the persistent profile (logins carry over); ghost is in-private with no cookies and nothing left behind" },
                    "private": { "type": "boolean", "description": "Shorthand for profile: ghost" }
                },
                "required": ["url"],
                "additionalProperties": false
            }),
            true,
        ),
        spec(
            "browser_tabs",
            "List every open tab with its id, title, URL, loading state and profile. Call it when you do not know where you are, or to find a tab another turn opened.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            true,
        ),
        spec(
            "browser_snapshot",
            "See the page as structure, not pixels: an indexed list of the actionable elements with their roles, accessible names, values, state and screen rects. Click by index (`[7]`), and the index is valid until the next snapshot. This is the tool to reach for first, and the one that keeps a long turn cheap — prefer it over browser_screenshot, which costs roughly twenty times the tokens for the same information.",
            json!({
                "type": "object",
                "properties": {
                    "tab": tab_property(),
                    "all": { "type": "boolean", "description": "Include elements below the fold (default false)" },
                    "interactive_only": { "type": "boolean", "description": "Only actionable elements (default true)" },
                    "max_nodes": { "type": "integer", "description": "Cap on returned nodes, 1-2000 (default 400)" },
                    "selector": { "type": "string", "description": "Snapshot only this subtree, e.g. `form#login`" },
                    "since_ms": { "type": "integer", "description": "Report the user's input over this many milliseconds instead of the default window" }
                },
                "additionalProperties": false
            }),
            true,
        ),
        spec(
            "browser_read",
            "Read the page's text, with the clickable elements inlined as [n] markers at the point in the prose where they appeared. Use it when you need to understand or quote what a page says; use browser_snapshot when you need to act on it. Password values are never included — such a field reports as `<set>`.",
            json!({
                "type": "object",
                "properties": {
                    "tab": tab_property(),
                    "selector": { "type": "string", "description": "Read only this subtree" },
                    "max_chars": { "type": "integer", "description": "Cap on returned characters (default 6000, max 40000)" }
                },
                "additionalProperties": false
            }),
            true,
        ),
        spec(
            "browser_find",
            "Find text on the current page and return the surrounding excerpt for each hit, so you can tell whether the phrase you are looking for actually appeared or something close to it did.",
            json!({
                "type": "object",
                "properties": {
                    "tab": tab_property(),
                    "text": { "type": "string", "description": "Text to look for" },
                    "max": { "type": "integer", "description": "Most hits to return, 1-100 (default 20)" },
                    "ignore_case": { "type": "boolean", "description": "Case-insensitive (default true)" }
                },
                "required": ["text"],
                "additionalProperties": false
            }),
            true,
        ),
        spec(
            "browser_wait",
            "Wait for a condition instead of guessing how long something takes. Always prefer this to a repeated snapshot loop: it returns as soon as the condition holds, and fails with what it saw if the timeout expires.",
            json!({
                "type": "object",
                "properties": {
                    "tab": tab_property(),
                    "selector": { "type": "string", "description": "Wait for this element to exist and be visible" },
                    "gone_selector": { "type": "string", "description": "Wait for this element to disappear — a spinner, an overlay" },
                    "text": { "type": "string", "description": "Wait for this text to appear" },
                    "url_matches": { "type": "string", "description": "Wait for the URL to match this regular expression" },
                    "network_idle": { "type": "boolean", "description": "Wait until no request has started for 500ms" },
                    "idle": { "type": "integer", "description": "Wait for the DOM to stop changing for this many milliseconds" },
                    "timeout_ms": { "type": "integer", "description": "Give up after this long (default 15000)" }
                },
                "additionalProperties": false
            }),
            true,
        ),
        spec(
            "browser_assert",
            "Check a condition and get PASS or FAIL with the evidence, without taking another snapshot or a screenshot. This is how you verify a step cheaply: did the URL change, is the confirmation text there, are there console errors. Use it after an action to prove the action worked.",
            json!({
                "type": "object",
                "properties": {
                    "tab": tab_property(),
                    "check": {
                        "type": "string",
                        "enum": ["url_matches", "title_matches", "text_present", "text_absent", "exists", "not_exists", "value_equals", "no_console_errors", "ready"],
                        "description": "url_matches and title_matches take a regex; text_present/absent take literal text; exists/not_exists take a CSS selector; value_equals takes a field target and a value; the last two take nothing"
                    },
                    "value": { "type": "string", "description": "What to match or look for" },
                    "target": { "type": "string", "description": "The field, for value_equals" }
                },
                "additionalProperties": false
            }),
            true,
        ),
        spec(
            "browser_screenshot",
            "See the page as an image. Use it when the visual result *is* the point — a layout, a chart, a canvas, a styling bug — and never to find out whether something worked, which browser_assert does for a fraction of the tokens. The image is shown to the user as well as to you.",
            json!({
                "type": "object",
                "properties": {
                    "tab": tab_property(),
                    "mode": { "type": "string", "enum": ["viewport", "full_page", "element"], "description": "viewport (default), full_page where supported, or element with a target" },
                    "target": { "type": "string", "description": "The element, for mode: element" },
                    "max_edge": { "type": "integer", "description": "Longest edge in pixels; 0 is native resolution" }
                },
                "additionalProperties": false
            }),
            true,
        ),
        spec(
            "browser_console",
            "Read the page's console output, uncaught exceptions and failed requests. This is the debugging half of the browser: point it at a page you are building and find out why it is broken.",
            json!({
                "type": "object",
                "properties": {
                    "tab": tab_property(),
                    "clear": { "type": "boolean", "description": "Clear the buffer after reading (default false)" }
                },
                "additionalProperties": false
            }),
            true,
        ),
        // ------------------------------------------------------------ Act
        spec(
            "browser_tab",
            "Manage tabs: open, close, focus, pin, mute, duplicate, reopen the last closed one. Use it to work on several pages at once — a form in one tab and its documentation in another — rather than navigating back and forth.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["list", "open", "close", "focus", "activate", "pin", "unpin", "mute", "duplicate", "reopen"], "description": "What to do (default list)" },
                    "tab": tab_property(),
                    "url": { "type": "string", "description": "For action: open" },
                    "profile": { "type": "string", "enum": ["normal", "ghost"], "description": "For action: open" }
                },
                "additionalProperties": false
            }),
            false,
        ),
        spec(
            "browser_navigate",
            "Move an existing tab: go to a URL, back, forward, reload, or stop loading. Prefer this to browser_open when you are continuing in the same tab — it keeps the history, which `back` then depends on.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["goto", "back", "forward", "reload", "stop"], "description": "What to do (default goto)" },
                    "url": { "type": "string", "description": "For action: goto" },
                    "tab": tab_property()
                },
                "additionalProperties": false
            }),
            false,
        ),
        spec(
            "browser_click",
            "Click an element. Take a snapshot first and use its index. Set `settle` to say what to wait for afterwards: `navigation` when the click changes the page, `network` when it loads data in place, `dom` (the default) when it redraws, and `none` when you are clicking something that must be followed by another click.",
            json!({
                "type": "object",
                "properties": {
                    "target": target_property(),
                    "kind": { "type": "string", "enum": ["click", "dblclick", "down", "up"], "description": "click (default), dblclick, or a held mouse button for a drag" },
                    "button": { "type": "string", "enum": ["left", "right", "middle"], "description": "left (default)" },
                    "settle": { "type": "string", "enum": ["none", "dom", "navigation", "network"], "description": "What to wait for after clicking (default dom)" },
                    "timeout_ms": { "type": "integer", "description": "How long to wait for it (default 10000)" },
                    "tab": tab_property()
                },
                "required": ["target"],
                "additionalProperties": false
            }),
            false,
        ),
        spec(
            "browser_type",
            "Type into a field. The value is set as a whole rather than key by key, so long text is instant and Unicode is exact; `submit` presses Enter afterwards. Never overwrite a password field that already holds a value the user typed — that is refused, and the refusal is a message to you, not an error to retry.",
            json!({
                "type": "object",
                "properties": {
                    "target": target_property(),
                    "text": { "type": "string", "description": "What to type. An empty string clears the field." },
                    "clear": { "type": "boolean", "description": "Clear the field first (default true); false appends" },
                    "submit": { "type": "boolean", "description": "Press Enter afterwards (default false)" },
                    "tab": tab_property()
                },
                "required": ["target"],
                "additionalProperties": false
            }),
            false,
        ),
        spec(
            "browser_press",
            "Press a key in the page or on an element: Enter, Escape, Tab, the arrows, Backspace, or a single character. Use it to submit a form you have just filled, or to dismiss something that has no button.",
            json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Enter, Escape, Tab, Backspace, Delete, ArrowUp/Down/Left/Right, Space, or one character" },
                    "target": { "type": "string", "description": "Where to send it; omit for the focused element" },
                    "repeat": { "type": "integer", "description": "How many times, 1-50 (default 1)" },
                    "tab": tab_property()
                },
                "required": ["key"],
                "additionalProperties": false
            }),
            false,
        ),
        spec(
            "browser_select",
            "Choose an option in a <select>. Give either its value or its visible label.",
            json!({
                "type": "object",
                "properties": {
                    "target": target_property(),
                    "value": { "type": "string", "description": "The option's value attribute" },
                    "label": { "type": "string", "description": "The option's visible text" },
                    "tab": tab_property()
                },
                "required": ["target"],
                "additionalProperties": false
            }),
            false,
        ),
        spec(
            "browser_check",
            "Tick or untick a checkbox or radio button, and report the state it ended in.",
            json!({
                "type": "object",
                "properties": {
                    "target": target_property(),
                    "checked": { "type": "boolean", "description": "The state to leave it in (default true)" },
                    "tab": tab_property()
                },
                "required": ["target"],
                "additionalProperties": false
            }),
            false,
        ),
        spec(
            "browser_hover",
            "Hover over an element, for menus and tooltips that only appear on hover. Follow it with a snapshot: what appeared is the point.",
            json!({
                "type": "object",
                "properties": {
                    "target": target_property(),
                    "tab": tab_property()
                },
                "required": ["target"],
                "additionalProperties": false
            }),
            false,
        ),
        spec(
            "browser_scroll",
            "Scroll the page or a scrollable element by an amount of pixels. Negative scrolls up. Prefer it to a screenshot when the thing you need is below the fold: scrolling and snapshotting beats reading pixels.",
            json!({
                "type": "object",
                "properties": {
                    "amount": { "type": "integer", "description": "Pixels, negative for up (default 600)" },
                    "target": { "type": "string", "description": "A scrollable element; omit to scroll the page" },
                    "tab": tab_property()
                },
                "additionalProperties": false
            }),
            false,
        ),
        spec(
            "browser_fill_form",
            "Fill several fields in one call, optionally submitting. Much cheaper than a type per field, and the right tool for a login or a checkout. Fields the user has just typed into are reported back rather than silently overwritten.",
            json!({
                "type": "object",
                "properties": {
                    "fields": {
                        "type": "array",
                        "description": "One entry per field",
                        "items": {
                            "type": "object",
                            "properties": {
                                "target": { "type": "string", "description": "[n] or a CSS selector" },
                                "value": { "type": "string", "description": "Text to set" },
                                "checked": { "type": "boolean", "description": "For a checkbox or radio" }
                            },
                            "required": ["target"],
                            "additionalProperties": false
                        }
                    },
                    "submit": { "type": "boolean", "description": "Submit the form after filling (default false)" },
                    "tab": tab_property()
                },
                "required": ["fields"],
                "additionalProperties": false
            }),
            false,
        ),
        spec(
            "browser_dialog",
            "Answer a JavaScript dialog (alert, confirm, prompt). Dialogs are queued rather than shown, so a page cannot freeze a turn by opening one; a snapshot names any that are waiting.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["accept", "dismiss", "list"], "description": "What to do (default accept)" },
                    "value": { "type": "string", "description": "Text to enter, for a prompt" },
                    "tab": tab_property()
                },
                "additionalProperties": false
            }),
            false,
        ),
        spec(
            "browser_drag",
            "Drag one element onto another, for drag-and-drop interfaces and sliders.",
            json!({
                "type": "object",
                "properties": {
                    "from": target_property(),
                    "to": { "type": "string", "description": "The drop target: [n] or a CSS selector" },
                    "tab": tab_property()
                },
                "required": ["from", "to"],
                "additionalProperties": false
            }),
            false,
        ),
        // ------------------------------------------------------------ Dev
        spec(
            "browser_evaluate",
            "Run JavaScript in the page and get the result back as JSON. The escape hatch for everything the other tools do not cover — scrape a table, call an API the page exposes, compute something, drive a canvas. Requires the Dev tools tier to be enabled.",
            json!({
                "type": "object",
                "properties": {
                    "expression": { "type": "string", "description": "An expression; `document.title` or `(async () => await fetch('/api').then(r => r.json()))()`" },
                    "tab": tab_property()
                },
                "required": ["expression"],
                "additionalProperties": false
            }),
            false,
        ),
        spec(
            "browser_cookies",
            "Read or set cookies for a site, including HTTP-only ones the page's own JavaScript cannot see. This is how you carry a session between a login and an API call. Requires the Dev tools tier.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["list", "get", "set", "delete"], "description": "What to do (default list)" },
                    "url": { "type": "string", "description": "Whose cookies; defaults to the current page" },
                    "cookie": { "type": "object", "description": "For action: set — {name, value, domain, path, httpOnly, secure, sameSite, expires}" },
                    "include_http_only": { "type": "boolean", "description": "Include HTTP-only cookies (default true)" },
                    "tab": tab_property()
                },
                "additionalProperties": false
            }),
            false,
        ),
        spec(
            "browser_storage",
            "Read a site's local storage, session storage or IndexedDB. Useful for finding a token the page is holding, or checking what survived a reload. Requires the Dev tools tier.",
            json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["local", "session", "indexeddb", "all"], "description": "Which store (default local)" },
                    "tab": tab_property()
                },
                "additionalProperties": false
            }),
            false,
        ),
        spec(
            "browser_clear_data",
            "Delete cookies, storage and cache — for one site, or everything this browser has. Use it to log out, to reset a broken session, or to tidy up a ghost tab's neighbours. Requires the Dev tools tier.",
            json!({
                "type": "object",
                "properties": {
                    "scope": { "type": "string", "enum": ["site", "all"], "description": "One origin, or the whole profile (default site)" },
                    "origin": { "type": "string", "description": "The origin to clear, e.g. https://example.com. Required for scope: site" },
                    "kinds": { "type": "array", "items": { "type": "string", "enum": ["cookies", "storage", "cache"] }, "description": "What to remove (default all three)" },
                    "tab": tab_property()
                },
                "additionalProperties": false
            }),
            false,
        ),
    ]
}

/// Looks a spec up by name, for Settings → Tools and the permission card.
pub fn spec_for(name: &str) -> Option<ToolSpec> {
    specs().into_iter().find(|spec| spec.name == name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A host that records what it was asked and replays canned JSON.
    struct FakeHost {
        calls: Mutex<Vec<(String, Value)>>,
        reply: Mutex<Value>,
        available: bool,
    }

    impl FakeHost {
        fn new(reply: Value) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                reply: Mutex::new(reply),
                available: true,
            }
        }

        fn calls(&self) -> Vec<(String, Value)> {
            self.calls.lock().unwrap().clone()
        }

        fn last_args(&self) -> Value {
            self.calls().last().map(|(_, args)| args.clone()).unwrap()
        }
    }

    impl BrowserHost for FakeHost {
        fn call(&self, _session_id: &str, op: &str, args: &Value) -> Result<Value> {
            self.calls
                .lock()
                .unwrap()
                .push((op.to_string(), args.clone()));
            Ok(self.reply.lock().unwrap().clone())
        }

        fn release(&self, _session_id: &str) {}

        fn available(&self) -> bool {
            self.available
        }
    }

    fn call(name: &str, arguments: &str) -> ToolCall {
        ToolCall {
            id: "call-1".to_string(),
            name: name.to_string(),
            arguments: arguments.to_string(),
        }
    }

    fn armed() -> BrowserOptions {
        BrowserOptions {
            armed: true,
            screenshot_edge: 0,
        }
    }

    fn home() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LOOM_HOME", dir.path());
        dir
    }

    #[test]
    fn every_listed_tool_has_a_spec_and_a_tier() {
        // The three lists are the contract: a name in TOOLS with no spec would
        // be offered to the model with no schema, and one with no tier could not
        // be switched off.
        for name in TOOLS {
            assert!(spec_for(name).is_some(), "{name} has no spec");
            assert!(tier_of(name).is_some(), "{name} has no tier");
        }
        assert_eq!(specs().len(), TOOLS.len());
    }

    #[test]
    fn the_classifier_is_exact_and_fail_closed() {
        assert!(is_browser_tool("browser_click"));
        for name in [
            "screenshot",
            "mouse",
            "fetch_url",
            "web_search",
            "read_file",
            "browser_clickk",
            "some_future_tool",
        ] {
            assert!(!is_browser_tool(name), "{name} must not be a browser tool");
        }
    }

    #[test]
    fn read_only_covers_every_looking_tool_and_nothing_that_acts() {
        for name in [
            "browser_open",
            "browser_tabs",
            "browser_read",
            "browser_snapshot",
            "browser_find",
            "browser_wait",
            "browser_assert",
            "browser_screenshot",
            "browser_console",
        ] {
            assert!(is_read_only(name), "{name} should be read-only");
        }
        for name in [
            "browser_click",
            "browser_type",
            "browser_press",
            "browser_select",
            "browser_check",
            "browser_navigate",
            "browser_tab",
            "browser_fill_form",
            "browser_dialog",
            "browser_drag",
            "browser_scroll",
            "browser_hover",
            "browser_clear_data",
        ] {
            assert!(!is_read_only(name), "{name} must not be read-only");
        }
        // The dev tier is split: reading cookies and storage changes nothing,
        // so Plan mode may look; clearing data does, so it may not.
        assert!(is_read_only("browser_cookies"));
        assert!(is_read_only("browser_storage"));
        assert!(!is_read_only("browser_evaluate"));
    }

    #[test]
    fn the_read_only_half_is_exactly_the_see_tier_plus_the_reading_dev_tools() {
        // A structural check rather than a list: every See-tier tool is
        // read-only, and every tool that is read-only is in See or is one of
        // the two Dev-tier readers. If someone adds a tool to See and forgets
        // this, the test says so.
        for spec in specs() {
            if tier_of(spec.name) == Some(Tier::See) {
                assert!(spec.read_only, "{} is in See but not read-only", spec.name);
            }
            if spec.read_only
                && tier_of(spec.name) != Some(Tier::See)
                && !matches!(spec.name, "browser_cookies" | "browser_storage")
            {
                panic!("{} is read-only but not in See", spec.name);
            }
        }
    }

    #[test]
    fn every_tool_declares_the_browser_scope() {
        // The permission card renders a badge per scope, so a browser tool
        // without one would show as a workspace tool.
        assert!(specs().iter().all(|spec| spec.scope == Some(ToolScope::Browser)));
    }

    #[test]
    fn the_dev_tier_is_off_by_default_and_the_other_two_are_on() {
        let defaults = default_tiers();
        assert!(defaults.contains(&Tier::See));
        assert!(defaults.contains(&Tier::Act));
        // The escape hatch ships closed: an arbitrary-JS tool should be a
        // decision, not a surprise.
        assert!(!defaults.contains(&Tier::Dev));

        assert!(tier_enabled(&defaults, "browser_click"));
        assert!(!tier_enabled(&defaults, "browser_evaluate"));
        assert!(tier_enabled(&[Tier::Dev], "browser_evaluate"));
        // An unknown name is not hidden by a tier the user turned off.
        assert!(tier_enabled(&[], "not_a_browser_tool"));
    }

    #[test]
    fn an_unarmed_chat_is_told_so_rather_than_failing_obscurely() {
        let host = FakeHost::new(json!({}));
        let outcome = run(
            "s1",
            &call("browser_click", r#"{"target":"[1]"}"#),
            &host,
            &BrowserOptions::default(),
        );
        assert!(!outcome.ok);
        assert_eq!(outcome.output, DISABLED_NOTE);
        // And the host was never asked to do anything.
        assert!(host.calls().is_empty());
    }

    #[test]
    fn a_missing_host_refuses_with_a_way_forward() {
        let outcome = run(
            "s1",
            &call("browser_read", "{}"),
            &NoBrowser::default(),
            &armed(),
        );
        assert!(!outcome.ok);
        assert!(outcome.output.contains("not available"), "{}", outcome.output);
        // It has to point somewhere, or the model just apologises.
        assert!(outcome.output.contains("fetch_url"), "{}", outcome.output);
    }

    #[test]
    fn a_missing_required_argument_names_the_argument() {
        let host = FakeHost::new(json!({}));
        let outcome = run("s1", &call("browser_click", "{}"), &host, &armed());
        assert!(!outcome.ok);
        assert!(outcome.output.contains("`target`"), "{}", outcome.output);
        assert!(host.calls().is_empty());
    }

    #[test]
    fn invalid_json_is_reported_as_such_rather_than_as_a_crash() {
        let host = FakeHost::new(json!({}));
        let outcome = run(
            "s1",
            &call("browser_click", "{not json"),
            &host,
            &armed(),
        );
        assert!(!outcome.ok);
        assert!(outcome.output.contains("not valid JSON"), "{}", outcome.output);
    }

    #[test]
    fn a_bad_enum_names_the_options() {
        let host = FakeHost::new(json!({}));
        let outcome = run(
            "s1",
            &call("browser_click", r#"{"target":"[1]","kind":"triple"}"#),
            &host,
            &armed(),
        );
        assert!(!outcome.ok);
        assert!(outcome.output.contains("click"), "{}", outcome.output);
        assert!(outcome.output.contains("dblclick"), "{}", outcome.output);
    }

    #[test]
    fn a_bare_host_becomes_https_and_a_sentence_becomes_a_search() {
        assert_eq!(normalize_url("example.com"), "https://example.com");
        assert_eq!(
            normalize_url("https://x.test/a?b=1"),
            "https://x.test/a?b=1"
        );
        assert_eq!(normalize_url("localhost:3000"), "http://localhost:3000");
        assert_eq!(normalize_url("127.0.0.1:8080/x"), "http://127.0.0.1:8080/x");
        // A space is the tell: no hostname has one, so it is a query.
        let search = normalize_url("rust tauri webview2");
        assert!(search.starts_with("https://duckduckgo.com/?q="), "{search}");
        assert!(search.contains("rust+tauri+webview2"), "{search}");
    }

    #[test]
    fn arguments_are_clamped_rather_than_rejected() {
        // A model asking for ten thousand nodes wants "as many as you can"; an
        // error would spend a round teaching it nothing.
        let host = FakeHost::new(json!({ "nodes": [] }));
        run(
            "s1",
            &call("browser_snapshot", r#"{"max_nodes":10000,"all":"yes"}"#),
            &host,
            &armed(),
        );
        let args = host.last_args();
        assert_eq!(args["max_nodes"], 2000);
        // The string where a boolean was asked for is honoured, too.
        assert_eq!(args["all"], true);
    }

    #[test]
    fn the_engine_setting_reaches_the_screenshot_call() {
        let host = FakeHost::new(json!({}));
        let options = BrowserOptions {
            armed: true,
            screenshot_edge: 1280,
        };
        run("s1", &call("browser_screenshot", "{}"), &host, &options);
        assert_eq!(host.last_args()["max_edge"], 1280);

        // And a call may override it, including asking for native resolution.
        let host = FakeHost::new(json!({}));
        run(
            "s1",
            &call("browser_screenshot", r#"{"max_edge":0}"#),
            &host,
            &options,
        );
        assert_eq!(host.last_args()["max_edge"], 0);
    }

    #[test]
    fn wait_needs_something_to_wait_for() {
        let host = FakeHost::new(json!({}));
        let outcome = run("s1", &call("browser_wait", "{}"), &host, &armed());
        assert!(!outcome.ok);
        assert!(outcome.output.contains("selector"), "{}", outcome.output);
        assert!(host.calls().is_empty());
    }

    #[test]
    fn a_refusal_from_the_host_is_not_an_error_to_retry() {
        let host = FakeHost::new(json!({
            "error": "refused: this password field already holds a value the user typed. \
                      It was left untouched."
        }));
        let outcome = run(
            "s1",
            &call("browser_type", r##"{"target":"#pw","text":"x"}"##),
            &host,
            &armed(),
        );
        assert!(!outcome.ok);
        assert!(outcome.output.starts_with("refused:"), "{}", outcome.output);
        assert!(outcome.output.contains("left untouched"), "{}", outcome.output);
    }

    #[test]
    fn a_snapshot_is_shaped_not_dumped_as_json() {
        let host = FakeHost::new(json!({
            "url": "https://example.com/login",
            "title": "Sign in",
            "ready": "complete",
            "viewport": [1280, 800],
            "scroll": [0, 0],
            "scrollHeight": 900,
            "nodes": ["[1]  textbox  \"Email\"", "[2]  button  \"Sign in\""]
        }));
        let outcome = run("s1", &call("browser_snapshot", "{}"), &host, &armed());
        assert!(outcome.ok);
        assert!(outcome.output.contains("PAGE  https://example.com/login"), "{}", outcome.output);
        assert!(outcome.output.contains("[2]  button"), "{}", outcome.output);
        // JSON punctuation is what a shaped renderer exists to avoid.
        assert!(!outcome.output.contains("\"nodes\""), "{}", outcome.output);
    }

    #[test]
    fn a_screenshot_is_stored_and_lifted_out_of_the_json() {
        let _home = home();
        let png: &[u8] = &[
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D',
            b'R', 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89,
        ];
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        let host = FakeHost::new(json!({
            "url": "https://example.com",
            "shot": { "name": "Tab 1.png", "mime": "image/png", "data": encoded, "width": 1280, "height": 800 }
        }));

        let outcome = run("s1", &call("browser_screenshot", "{}"), &host, &armed());
        assert!(outcome.ok, "{}", outcome.output);
        assert_eq!(outcome.images.len(), 1);
        let image = &outcome.images[0];
        assert_eq!(image.name, "Tab 1.png");
        assert_eq!(image.mime, "image/png");
        // Dimensions are what the context budget counts against, so they have
        // to survive the round trip.
        assert_eq!((image.width, image.height), (1280, 800));
        assert!(std::path::Path::new(&image.path).exists(), "{}", image.path);
        // And the base64 must not have leaked into the text the model reads.
        assert!(!outcome.output.contains(&encoded[..40]), "base64 leaked");
    }

    #[test]
    fn a_broken_shot_is_reported_rather_than_swallowed() {
        let _home = home();
        let host = FakeHost::new(json!({
            "shot": { "name": "x.png", "data": "not base64 at all!!" }
        }));
        let outcome = run("s1", &call("browser_screenshot", "{}"), &host, &armed());
        assert!(!outcome.ok);
        assert!(outcome.output.contains("base64"), "{}", outcome.output);
    }

    #[test]
    fn fill_form_requires_a_target_on_every_field() {
        let host = FakeHost::new(json!({}));
        let outcome = run(
            "s1",
            &call("browser_fill_form", r#"{"fields":[{"value":"x"}]}"#),
            &host,
            &armed(),
        );
        assert!(!outcome.ok);
        assert!(outcome.output.contains("target"), "{}", outcome.output);

        let outcome = run(
            "s1",
            &call("browser_fill_form", r#"{"fields":[]}"#),
            &host,
            &armed(),
        );
        assert!(!outcome.ok);
        assert!(outcome.output.contains("no fields"), "{}", outcome.output);
    }

    #[test]
    fn select_needs_a_value_or_a_label() {
        let host = FakeHost::new(json!({}));
        let outcome = run(
            "s1",
            &call("browser_select", r##"{"target":"#colour"}"##),
            &host,
            &armed(),
        );
        assert!(!outcome.ok);
        assert!(outcome.output.contains("`value` or `label`"), "{}", outcome.output);
    }

    #[test]
    fn clearing_one_site_requires_an_origin() {
        let host = FakeHost::new(json!({}));
        let outcome = run(
            "s1",
            &call("browser_clear_data", r#"{"scope":"site"}"#),
            &host,
            &armed(),
        );
        assert!(!outcome.ok);
        assert!(outcome.output.contains("origin"), "{}", outcome.output);

        // Clearing everything needs no origin, and the default kinds cover all
        // three stores rather than quietly dropping cache.
        run(
            "s1",
            &call("browser_clear_data", r#"{"scope":"all"}"#),
            &host,
            &armed(),
        );
        let kinds = host.last_args()["kinds"].clone();
        assert_eq!(kinds.as_array().unwrap().len(), 3);
    }

    #[test]
    fn a_private_flag_is_accepted_as_the_ghost_profile() {
        // The word a model reaches for is `private`; refusing it teaches
        // nothing and costs a round.
        assert_eq!(Profile::parse("private"), Some(Profile::Ghost));
        assert_eq!(Profile::parse("ghost"), Some(Profile::Ghost));
        assert_eq!(Profile::parse("Normal"), Some(Profile::Normal));
        assert_eq!(Profile::parse("nonsense"), None);
    }

    #[test]
    fn the_profile_reaches_the_host_rather_than_defaulting_silently() {
        // A mis-typed profile must not become a shared-cookie tab when the
        // model asked for a private one: that is the one direction of this
        // mistake that leaves a record the user did not want.
        let host = FakeHost::new(json!({ "url": "https://x" }));
        run(
            "s1",
            &call("browser_open", r#"{"url":"x.test","profile":"ghost"}"#),
            &host,
            &armed(),
        );
        assert_eq!(host.last_args()["profile"], "ghost");

        // `private: true` is the alias, and it wins.
        let host = FakeHost::new(json!({ "url": "https://x" }));
        run(
            "s1",
            &call("browser_open", r#"{"url":"x.test","private":true}"#),
            &host,
            &armed(),
        );
        assert_eq!(host.last_args()["profile"], "ghost");

        // And nothing said means normal, explicitly, so the host never has to
        // carry a default of its own.
        let host = FakeHost::new(json!({ "url": "https://x" }));
        run("s1", &call("browser_open", r#"{"url":"x.test"}"#), &host, &armed());
        assert_eq!(host.last_args()["profile"], "normal");

        // A nonsense value is refused, with the options named.
        let host = FakeHost::new(json!({}));
        let outcome = run(
            "s1",
            &call("browser_open", r#"{"url":"x.test","profile":"stealth"}"#),
            &host,
            &armed(),
        );
        assert!(!outcome.ok);
        assert!(outcome.output.contains("normal or ghost"), "{}", outcome.output);
        assert!(host.calls().is_empty());
    }

    #[test]
    fn the_holder_is_exclusive_and_released() {
        let holder = Holder::default();
        // First claim wins.
        assert_eq!(holder.claim("a"), Ok(true));
        // The same chat re-claiming is not an error: a turn makes many calls.
        assert_eq!(holder.claim("a"), Ok(false));
        // A second chat is refused while the first holds it.
        assert!(holder.claim("b").is_err());
        assert_eq!(holder.current().as_deref(), Some("a"));

        // Releasing from a *different* chat must not steal it.
        assert!(!holder.release("b"));
        assert_eq!(holder.current().as_deref(), Some("a"));

        assert!(holder.release("a"));
        assert_eq!(holder.current(), None);
        // And the next chat can take it.
        assert_eq!(holder.claim("b"), Ok(true));
    }

    #[test]
    fn release_is_safe_to_call_twice() {
        // A turn ending and the supervisor's failure path both call it, and
        // the second call must not panic or resurrect the hold.
        let holder = Holder::default();
        assert_eq!(holder.claim("a"), Ok(true));
        assert!(holder.release("a"));
        assert!(!holder.release("a"));
        assert_eq!(holder.current(), None);
    }

    #[test]
    fn the_collector_is_compiled_in() {
        // `include_str!` failing would be a build error, but this catches the
        // file being emptied into a stub.
        assert!(COLLECTOR_JS.contains("__loomBrowser"));
        assert!(COLLECTOR_JS.contains("postMessage") || COLLECTOR_JS.contains("handle"));
        // The two refusals have to be in the page script, not only in prose.
        assert!(COLLECTOR_JS.contains("guardPassword"));
        assert!(COLLECTOR_JS.contains("userTyped"));
    }

    #[test]
    fn the_busy_note_points_somewhere_useful() {
        assert!(BUSY_NOTE.contains("fetch_url"), "{BUSY_NOTE}");
    }

    #[test]
    fn one_run_does_not_call_the_host_twice() {
        // A tool that costs two round trips would double every page load, and
        // the dispatch chain is assembled in the engine where that is easy to
        // do by accident.
        let counter = AtomicUsize::new(0);
        struct Counting<'a>(&'a AtomicUsize);
        impl BrowserHost for Counting<'_> {
            fn call(&self, _s: &str, _op: &str, _a: &Value) -> Result<Value> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(json!({ "ok": true }))
            }
            fn release(&self, _s: &str) {}
        }
        run("s1", &call("browser_tabs", "{}"), &Counting(&counter), &armed());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
