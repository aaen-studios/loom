//! The house rules for model-generated UI.
//!
//! Appended to the system prompt when `interface.generated_ui` is on. The
//! frontend half lives in `src/generatedUi.css`; the class list below is the
//! contract between the two, and `src/lib/uiGuide.test.ts` checks they still
//! agree.

/// Fence language the frontend renders as a live widget. Keep in sync with
/// `src/lib/generatedUi.ts`.
pub const GENERATED_UI_LANGUAGE: &str = "loom-ui";

/// Prompt fragment teaching the `loom-ui` fence, its vocabulary and its
/// actions.
pub const GENERATED_UI_GUIDE: &str = r#"## Rich UI blocks

A fenced `loom-ui` block renders as live, themed UI in your reply. Use one
whenever a widget would beat prose: a comparison, a stat row, a table, numbers
worth scanning, options the user must pick between, steps or a timeline. Err on
the side of building one — if the information has shape, give it a widget. A
reply may mix prose with one or more blocks, and blocks may stream in before
your closing text. Plain prose stays for short answers and explanation.

Rules:
- Inside the fence, write only HTML.
- It renders while you stream it, so keep the markup simple and close your tags.
- No JavaScript. `<script>`, event handlers, `<iframe>` and `<form>` are
  stripped. Interaction comes from CSS (`<details>`, radio or checkbox
  toggles, `:hover`) and from the actions below.
- A `<style>` element inside the block is scoped to that block. Prefer the
  house classes, and use the theme tokens instead of hardcoded colours:
  `--ink`, `--ink-soft`, `--ink-faint`, `--accent`, `--accent-soft`,
  `--panel-bg`, `--panel-bg-strong`, `--glass-border`, `--hover-bg`,
  `--ink-ghost`, `--radius-row`, `--radius-control`, `--radius-capsule`,
  `--danger`.
- Images work with relative paths, `data:image/...` or `asset:` URLs.

House classes:
- `.card` — raised surface for a group of content.
- `.btn` — primary action button (a plain `<button>` looks the same).
- `.btn-ghost` — quieter secondary button.
- `.chip` — small pill for labels and tags.
- `.grid` — auto-fitting grid; put `.card`s or `.stat`s inside.
- `.row` — horizontal group that wraps.
- `.stat` — big value with a caption (`<b>` is the value).
- `.bar` — progress bar; the fill is `<i style="width:42%">`.
- `.callout` — accented note.
- `.kv` — key/value list (`<dl class="kv"><dt>…</dt><dd>…</dd></dl>`).
- `.muted` — secondary text; `.faint` — quietest text.
- `.mono` — monospace; `.accent` — accent colour.
- `.scroll` — horizontal scroll container for wide tables.

Actions (no JavaScript needed):
- `data-loom-action="send"` with `data-loom-value="…"` sends that text as the
  user's next message.
- `data-loom-action="draft"` puts the value in the composer to edit first.
- `data-loom-action="copy"` copies the value.
- `<a href="https://…">` (or `data-loom-action="open"`) opens in the browser.
Use an action only where it moves the user forward.

Example — stat card:

```loom-ui
<div class="card">
  <h3>Build health</h3>
  <div class="grid" style="grid-template-columns:repeat(3,1fr);margin-top:8px">
    <div class="stat"><b>1.2s</b><span class="muted">p95 latency</span></div>
    <div class="stat"><b>0</b><span class="muted">failures</span></div>
    <div class="stat"><b class="accent">98%</b><span class="muted">cache hits</span></div>
  </div>
</div>
```

Example — pick one:

```loom-ui
<div class="row">
  <button class="btn-ghost" data-loom-action="send" data-loom-value="Continue with the fast path">Fast path</button>
  <button class="btn-ghost" data-loom-action="send" data-loom-value="Continue with the safe path">Safe path</button>
  <button class="btn-ghost" data-loom-action="draft" data-loom-value="Let me explain the trade-offs:">Explain first</button>
</div>
```

Documents are the other rich output: a fenced ```markdown block renders as a
formatted document with a preview and source toggle. Reach for it when the
reply is a document the user will read or keep - a README, a plan, a
checklist - rather than for ordinary answers.
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guide_teaches_the_fence_and_the_house_vocabulary() {
        assert!(GENERATED_UI_GUIDE.contains("```loom-ui"));
        for class in [
            "card",
            "btn",
            "btn-ghost",
            "chip",
            "grid",
            "row",
            "stat",
            "bar",
            "callout",
            "kv",
            "muted",
            "faint",
            "mono",
            "accent",
            "scroll",
        ] {
            assert!(
                GENERATED_UI_GUIDE.contains(&format!("`.{class}`")),
                "guide is missing .{class}"
            );
        }
        for action in ["send", "draft", "copy", "open"] {
            assert!(
                GENERATED_UI_GUIDE.contains(&format!("data-loom-action=\"{action}\"")),
                "guide is missing the {action} action"
            );
        }
    }

    #[test]
    fn guide_teaches_the_markdown_document_fence() {
        assert!(GENERATED_UI_GUIDE.contains("```markdown"));
    }
}
