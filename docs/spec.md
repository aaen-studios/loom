# Loom — spec

Desktop app for AI chat and agents. Windows 11 first.

**Status: M0–M9 implemented, release tooling in place.** Providers, streaming,
tools, MCP, skills, image generation, subagents, the coding tools, the updater,
and the bespoke Setup app all exist in code with tests; what remains is
day-to-day polish (see "Open work" at the bottom).

## Locked decisions

| Area | Decision |
| --- | --- |
| Stack | Tauri v2 + Vite + React 19 + TS + Tailwind v4, Bun. Rust engine in its own crate (`crates/loom-core`). |
| Layout | Full-bleed background, centered full-width canvas. **No permanent sidebar** — chats live in a popup summoned from the titlebar. No big blurred panel around the workspace. |
| Chrome | Frameless window, two floating pills (navigation left, window controls right), custom-drawn glyphs. Inter font. Dark by default. |
| Corners | Continuous (squircle) corners via `corner-shape`, on a concentric radius scale: window 20px, sheet 18px (composer, popovers, drawer, toast), row 10px (sheet − padding), control 12px, capsule for chrome bars and chips. The model picker also allows adding a model id by hand when `/models` is unavailable. |
| Glass | The app renders its own background (built-in presets + user images/videos) and uses CSS `backdrop-filter` only on small surfaces (composer, popups, cards). |
| Streaming | Rust-owned streams: concurrent across chats, survive window switches and backgrounding. Toast when a reply finishes unfocused. |
| Overlay | Tray icon + `Ctrl+Shift+Space` quick-ask overlay (new chat, then reveals the main window). |
| Providers | Presets include OpenAI, Anthropic, OpenRouter, DeepSeek, Z.ai, Groq, xAI, Google, **OpenCode Go**, **OpenCode Zen**, Ollama, LM Studio, plus custom OpenAI-compatible/Anthropic endpoints. `GET /models` auto-detect merged over a bundled catalog with manual overrides. Keys live in Windows Credential Manager. |
| Usage | Settings → Usage shows live vendor limits for the providers whose vendor exposes an endpoint (OpenCode Go's rolling/weekly/monthly subscription windows, OpenRouter key credits, DeepSeek balance, Z.ai coding-plan quota), resolved from the provider's base URL so custom endpoints work too. The composer shows the active model's tightest window (or remaining credit) as a small badge that deep-links to the section, and a local table totals tokens plus list-price cost estimates per provider from the `loom.db` reply metadata. Readings are fetched on launch, every five minutes, and on demand; nothing is persisted. |
| Chat | Reasoning thinking panel + per-chat variant selector, persona library, lite-model auto titles, attachments (images, text/code files, text-layer PDFs). One panel per thinking spell — each kept in the stream where it happened, between the text and tool calls it produced, and independently collapsible. Messages typed while a reply runs are queued (drag to reorder, arrow to send now and interrupt, X to remove); the queue sends the next one as each turn finishes, and Stop parks it. The `/` menu carries skills, saved prompts, and built-in commands: `/goal <text>` sets a per-chat goal, `/todo <text>` adds a task, `/todos` opens the goal panel, `/plan <text>` switches to Plan and sends, and `/new` starts a chat. |
| Generated UI | A ```loom-ui fenced block renders as live, themed HTML inside the reply. Sanitized against an allowlist (DOMPurify; no scripts, event handlers, forms or iframes) and mounted in a shadow root styled from the app's design tokens, so it tracks dark/light and compact density on its own. The system prompt gains a guide (only when enabled) that names the fence, a small house class vocabulary and declarative actions — `send`, `draft`, `copy`, `open` — which are the only way a widget talks back. Oversized or fully-stripped blocks fall back to source. Toggle: Settings → General. |
| Rich replies | Code blocks render with Shiki highlighting (languages load on demand, github-light/dark-default), a language header, line numbers, and a capped, scrolling body; copy lives in the header and dims while the fence is still streaming. A ```markdown fence renders as a formatted document with a preview/source toggle, and the widget renderers stay off inside it so a fence in a document stays code. Markdown links open in the OS browser instead of navigating the webview. |
| Tools | `datetime`, `list_dir`, `read_file` (with line ranges), `grep` (with an include glob), `find_files`, `create_dir`, `move_path`, `copy_path`, `delete_path` (always confirmed), `git_status`, `git_diff`, `git_log`, `write_file`, `edit_file`, `run_command`, `web_search`, `fetch_url`, `search_workspace`, `generate_image`, `spawn_agent`, `todo_write`/`todo_read`, plus every tool exposed by MCP servers. `todo_write` replaces the chat's live task list, which the goal/task panel above the composer renders and updates as the turn runs (and the model reads back each turn alongside the `/goal` objective); it only edits Loom's own state, so it needs no permission card and works in Plan mode. Permission modes: Ask / auto read-only / auto run-all, global default with per-chat override. Agent modes: Plan / Review / Build, also a global default with per-chat override — the read-only modes refuse the write and command tools outright (no permission card): Plan asks questions, researches, and proposes a plan; Review reports severity-ranked findings with file and line plus proposed fixes, changing nothing. Agent mode, approvals, and computer use share one composer chip (`Build · Auto all`) with one grouped menu. Web tools prefer Jina AI (search + reader) when a key is stored, and fall back to DuckDuckGo's HTML endpoint and a local tag stripper without one. A fourth mode, **Atelier**, runs everything Auto all runs and additionally exposes the harness tools — `list_harness`, `upsert_persona`/`delete_persona`, `upsert_prompt`/`delete_prompt`, `write_skill`/`delete_skill`, `upsert_mcp_server`/`delete_mcp_server`/`test_mcp_server`, `upsert_provider`/`update_model`/`delete_provider`, `update_settings` — which let the model edit Loom itself. Harness tools are invisible in every other mode and refused outright if called; the five deletes still show a permission card (no "Always allow"), and every config write takes a `backups/` snapshot first. Atelier is deliberately per chat and is never accepted as the global default. |
| Computer use | Per-chat **Computer chip** (Windows only, needs a vision model) arms `screenshot`, `mouse`, `keyboard`, `ui` (UI Automation), `clipboard`, `list_windows`, `window`, `list_processes`, `launch_app`, `kill_process`, and `wait`. The chip is the standing consent, so actions run without per-action cards; real user input pauses the whole turn (auto-resumes after 30 s idle, or Resume in the pill), `Ctrl+Alt+Esc` stops it from anywhere, and a pill at the top of the primary monitor shows what is happening. Screenshots default to native resolution (`chat.computerScreenshotEdge` can downscale), are stored in `attachments/<session>/` (newest 200 per chat), shown in the transcript, and inlined into the wire only for the newest capture, so context stays flat across long turns. Mouse coordinates are image pixels from the last screenshot; the engine maps them back through the captured rect and scale. Latency knobs: `chat.computerVariant` (default `low` thinking), an optional `chat.computerModel` fast vision model used only while armed, old-turn computer chatter compacted to one line on the wire, and the context budget reserves image tokens for the inlined screenshot. Single-turn lock: one chat drives the machine at a time; a per-turn guard releases hooks and the lock even if a tool panics. |
| Workspaces | Folders the user adds are kept in `config.json`, not just on the chat that used them. The composer chip switches, renames, and forgets them; the chats popup groups history by workspace and starts new chats in a group. |
| Retrieval | Per-chat workspace index in SQLite: files chunked (1.6k chars, 200 overlap), embedded via the chat provider's `/embeddings` endpoint (default `text-embedding-3-small`), ranked by cosine similarity in-process. |
| Storage | SQLite in `~/.loom/loom.db` (migrations v1→v9), config in `~/.loom/config.json`, attachments/backgrounds/generated/skills/cache/backups under `~/.loom`, provider and Jina API keys in Windows Credential Manager. `backups/` holds the config snapshots taken before every harness edit (the newest ten are kept) and copies of overwritten skill files. |
| Distribution | Bespoke glass **Loom Setup** app (`setup/`): a single portable exe with the payload embedded — no installer framework at all. Extracts the app, writes shortcuts, registers an HKCU uninstall entry, and drops an uninstaller in `%LOCALAPPDATA%\Loom` (outside the install folder, so it can delete it). Supports `--silent --dir <path>` for scripted installs. Ground-up updater: `update.json` on GitHub releases, semver compare, SHA-256 + minisign verification, staging, swap-after-exit script. |

## Roadmap order

MCP → web search → image generation → skills & commands → multi-agent →
coding agent. All of the above are implemented; the coding agent is the
deepest (workspace folder, `read_file`/`write_file`/`edit_file`/`run_command`,
`AGENTS.md` injection, diff preview, per-chat permission modes).

## Repo layout

```
loom/
  src/                 React UI (components/, stores/, lib/)
  src-tauri/           Tauri shell: window, tray, hotkey, overlay, commands
  crates/loom-core/    Engine: config, db, providers, tools, mcp, web, images,
                       skills, attachments, workspace, updater
  setup/               Loom Setup (bespoke glass installer, Tauri app)
  scripts/             icon generation, payload packaging
  docs/spec.md         this file
```

## Open work

- Light theme over bright artwork is usable now (stronger glass + ink) but a
  proper design pass would still help.
- The workspace index is brute-force cosine over stored chunks; fine to a few
  thousand chunks, worth an ANN index beyond that.
- Windows code signing needs a certificate; the workflow is wired and skips
  cleanly without one.

## Streaming fix (the important one)

Rust''s `#[serde(rename_all = "camelCase", tag = "type")]` on an *enum* renames
the **variants**, not the variant **fields**. Events therefore arrived as
`session_id`/`message_id` while the frontend read `sessionId`/`messageId`, so
every `started`/`delta`/`done` was filed under an `"undefined"` key: text only
appeared when the session was reloaded from the database (which is why replies
looked like they arrived in one chunk, and why the composer could stay stuck on
"stop"). Fixed with `rename_all_fields = "camelCase"`, a test that asserts the
serialized wire format, and a frontend guard that drops malformed events instead
of inventing undefined entries.

## Reliability fixes (found by diagnosing the running app)

Three bugs made the app look "broken and white", all confirmed with evidence
rather than inspection alone:

1. **Infinite render loop** — `ToolCallList` selected `state.liveTools[id] ?? []`
   from zustand, allocating a new array per render, so React's snapshot check
   failed forever and the tree died. The selector now returns the stored
   reference. An `ErrorBoundary` and a pre-mount reporter mean a failure like
   this shows a message instead of a blank window.
2. **Process abort on send** — `send_message` was a *synchronous* Tauri command,
   so it ran on the event-loop thread where `tokio::spawn` panics ("there is no
   reactor running"); a panic in an IPC handler aborts the process
   (`0xc0000409`). It is now an async command, and `Engine::send` returns an
   error instead of panicking when called without a runtime — both covered by
   tests.
3. **Gateway rejection** — OpenCode Go requires a stable `x-opencode-session`
   header per conversation (confirmed against the live endpoint: without it,
   `400 MissingSessionID`; with it, `200` and a real completion). Providers can
   declare `sessionHeader`, the preset sets it, and configs written by older
   builds are upgraded on load.

Also: a failed turn now records its reason on the message (visible, retryable,
survives a reload), a development build with no dev server falls back to the
bundled frontend instead of a blank window, and the payload script no longer
claims a false dev/release check.

## Done since the first pass

- Markdown/streamdown is code-split (main chunk ~316 kB; the 486 kB markdown
  chunk loads on the first reply).
- Streaming deltas are coalesced in the engine (96 chars for text, 256 for
  reasoning) instead of one event per token.
- Transcript scrolling only follows output while pinned to the bottom, with a
  "jump to latest" affordance.
- Retry on failure drops the failed turn and re-sends, rather than duplicating
  history.
- Chats popup: search, double-click rename, export to markdown, and per-row
  status marks — a live activity line while a reply streams (Thinking…, the
  running tool, Replying…), a red Failed mark with the reason in the tooltip,
  Question/Approval pills, an unread dot for replies that landed while you were
  in another chat, and an accent edge on the open chat.
  Shortcuts: Ctrl+N, Ctrl+K, Ctrl+,. Model picker: favourites float to the top.
- Sidebar is a popup and the canvas is full-width and centered.
- Token usage is persisted per reply and shown under the message.
- Updater: minisign keypair generated, public key compiled in, CI signs the
  payload when `LOOM_MINISIGN_KEY` is set, and the app offers new versions in a
  toast plus Settings → Updates. Verification is covered by tests that include
  tamper and wrong-key cases.
- Workspace indexing + `search_workspace` semantic tool, with per-model context
  metadata editable in Settings → Providers.
- Frontend tests (vitest): streaming state machine + pure helpers; CI runs
  encoding checks, both typechecks, vitest, engine tests, and both crates.
- Model selection: engine-level tests prove the choice persists on the session
  and survives a reopen; the picker surfaces command errors instead of failing
  silently, preserves the titles model when switching, and can add a model id
  by hand for providers whose `/models` endpoint is unavailable.
- Corners follow Apple's rules: squircle (continuous) corners on a concentric
  radius scale, with capsules for chrome bars, chips and notices.
- Provider usage and subscription limits: OpenCode Go windows, OpenRouter
  credits, DeepSeek balance, and Z.ai coding-plan quota in Settings → Usage,
  plus a composer badge and per-provider local token/cost totals (parser and
  wire-shape tests per vendor).
