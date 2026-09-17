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
| Chrome | Frameless window, floating pills — **Settings** then navigation on the left, app surfaces (panels, workspace, persona) and window controls on the right — over an invisible drag strip, with custom-drawn glyphs. Inter font. Light by default, with a full dark palette one toggle away. Settings lives here rather than at the foot of the chats list, where it used to be: that row was permanently reserved space in a column whose job is showing as many chats as possible, and it was only reachable while that panel happened to be open. Voice mode **has no button** — it is `Ctrl+Shift+V` and a row in the shortcut sheet, which makes the sheet the only place its keys are discoverable. |
| Corners | Continuous (squircle) corners via `corner-shape`, on a concentric radius scale: window 20px, sheet 18px (composer, popovers, drawer, toast), row 10px (sheet − padding), control 12px, capsule for chrome bars and chips. The model picker also allows adding a model id by hand when `/models` is unavailable. |
| Glass | The app renders its own background (built-in presets + user images/videos) and uses CSS `backdrop-filter` on real surfaces: **pill, panel, panel-strong, blob, and a thinner `.glass-thin` tier** for chrome rows like the chat header and the dock's tab strip. The built-in presets are painted entirely in CSS — a faint woven texture over soft radial washes — so **no artwork is bundled** and nothing has to be cleared for redistribution. Each preset is authored to stay legible as-is, which is why the dim veil defaults to 0; it is for the user's own artwork. Two user-facing multipliers, `--glass-tint` (50–100) and `--glass-blur` (0–200), are written onto `<html>` by `useGlassVars` and applied by **unlayered** rules placed *below every `loom-site:` marker* — unlayered because `@utility` compiles into `@layer utilities` and an unlayered rule has to beat it, and below the markers because `sync-site-tokens.mjs` records each region's source line range in the generated file's header, so inserting anything above one shifts it and `tokens:check` fails. Ranges run one way only: `color-mix` percentages clamp at 100, so a "120% tint" would be silently identical to 100%. `.terminal-surface` is deliberately excluded from both. |
| Liquid glass | Three surfaces **refract** their backdrop — the four title-bar pills, the composer, and torn-off panel windows — via `liquid-glass-react`, behind Loom's own `LiquidSurface` wrapper. The wrapper exists because the library refracts what is painted *behind* it, and Loom's surfaces are already 82%-opaque 38px-blur glass: glass on glass shows a blurred blur and no refraction. So a liquid surface is three layers — `.lg-stage` owns layout, `.lg-shell` holds the library's warp layer alone, `.lg-tint` paints the tint and border *above* the warp (a tint on the shell would become part of the warp's own input), and the real content is a **sibling** of the shell, because the library's box is `overflow: hidden` and these surfaces hold the composer's slash menu and the pills' three menus. The library's four decorative nodes are hidden by our CSS, which is also what removes any need for a Tailwind shim. Off by default nowhere: `enabled`, `pills`, `composer` and `panels` are typed in `config.interface.glass` and resolved *inside* the wrapper, so a call site cannot opt itself in past the master switch. **The parameters are in pixels**, not the library's own `blurAmount` units, which are 32× apart — passing its scale a plausible `5` produced 164px of backdrop blur, smearing the backdrop so flat the displacement map had nothing left to bend, and the effect rendered *nothing* while every computed style looked correct. `scripts/probe-glass.mjs` is what catches that class of failure: it renders the same element twice, once with the filter and once without, and compares the pixels. `shader` mode is not offered — it rasterises its map pixel by pixel in a nested loop on mount and on every resize. Measured: the SVG filter's own cost is 0.1–0.5ms per frame on top of the `backdrop-filter` Loom already had; the cost is the frosted area, not the refraction. |
| Streaming | Rust-owned streams: concurrent across chats, survive window switches and backgrounding. Toast when a reply finishes unfocused. |
| Overlay | Tray icon + `Ctrl+Shift+Space` quick-ask overlay (new chat, then reveals the main window). |
| Dock | Any window edge holds a **zone**: a resizable area with a stack of panels as tabs. Zones are **closed by default** — Loom opens on the conversation. There is **no rail and no hover zone**: both were permanent chrome for a usually-closed surface, and the hover band fought the native window resize. One **Panels menu** in the title bar is the way in, alongside `Ctrl+``; it lists every panel with its state, so it stays correct as panels are added where a fixed pair of buttons could not. Panels register in `src/dock/registry.tsx`: **Terminal, Runs, Chats, Files, Goal, Browser**. Goal is in no default layout, so the composer keeps it. |
| Docking | **Bespoke** — own zones, splitters, tab stacks and drag state, behind a registry seam. **Dragging is the way to rearrange**: press a tab and pull, and a ghost follows the cursor while the drop target is computed from what is under it — onto another tab strip to move there, onto a window edge to dock against that edge (creating one if the edge has none), or **out of the window to give the panel its own**. `Escape` cancels. The tab menu (`⋯`) carries the same moves for anyone who would rather not drag. Drag state lives in its own store (`stores/dockDrag.ts`) rather than in the layout store, because a drag updates at pointer rate and a layout changes on release; only the *target* is React state, and the ghost is moved by writing a transform, so a drag across the window re-renders a handful of times rather than once per frame. Hit-testing is coalesced to one `requestAnimationFrame`, since `elementFromPoint` forces layout. Any panel tears off into its own window via `index.html?panel=<id>`, reusing the query routing `?computer` and `?ask` already use; the window holds the panel and nothing else, and *Dock it back* sends it home. Layout truth lives in **Rust config** (`config.dock`, keyed by workspace folder, with `dockDefault` as the fallback) and is broadcast on `loom://dock`, because two webviews cannot share a `zustand` store and a torn-off window has to agree with the main window. `dock.rs` enforces only structural invariants — a panel appears in exactly one zone, the active index is in range, sizes are sane, zone order is kept — and deliberately never invents, drops or reorders panel ids, so a downgrade is not destructive. Reads resolve through `dock_layout`; writes go through `set_dock_layout` and the store settles on the value the broadcast carries. |
| Terminal | A **real pty**, not a pipe: `portable-pty` (ConPTY on Windows, `openpty` elsewhere), so colours, interactive prompts, arrow keys, resize and full-screen programs all work. `crates/loom-core/src/pty.rs` — one session per workspace folder plus as many more as `+` asks for, tabs named after the profile, a per-session blocking reader thread, and **one pump thread that coalesces a burst into a single event** (a shell repainting a progress bar writes thousands of tiny chunks a second; one event per read is the obvious implementation and a performance trap). Output is base64 over `loom://pty`, on its own channel, because terminal traffic must not be multiplexed with engine events or routed through the chat store. xterm receives **bytes**, never text: a chunk can end mid-escape-sequence or mid-codepoint, and the parser is xterm's job. The xterm instances live outside React in `lib/terminals.ts` keyed by session id, so a tab switch or a `StrictMode` remount reattaches the same terminal with its scrollback intact, and output arriving before its terminal exists is buffered rather than dropped. **All sixteen ANSI colours are Loom's own**, defined in `styles.css` per theme and read out of the live stylesheet — xterm's stock palette is pure-hue (#ff0000, #0000ff) and sat wrong against the app's indigo, and leaving fourteen slots unset meant exactly that. The background is **heavy glass rather than a hole**: a terminal is dense 13px monospace read for minutes, so it gets a stronger blur and higher opacity than any other surface, with the artwork left as a tint. Appearance (font, size, line height) is `config.terminal`; JetBrains Mono is bundled, with a picker. Profiles are auto-detected (pwsh → powershell → cmd on Windows, `$SHELL` elsewhere) plus Git Bash and every WSL distro from `wsl.exe -l -q` — which writes **UTF-16LE**, hence `decode_wsl_listing` with its own tests. A folder remembers its shell. Closing the last shell closes the panel rather than leaving a frame its own start-a-shell effect would immediately refill; closing a session kills the process **tree**, and `RunEvent::Exit` kills them all. When the terminal is **alone in its zone, its shell tabs are the zone's tabs** — one row instead of two saying almost the same thing — via the registry's optional `tabStrip`, which comes back to the normal row the moment a second panel shares the zone. **The terminal is the user's, and nothing else may use it**: `pty_write` is not a tool, is not in `list_tools`, and no engine code path calls it — a live shell has no permission card in front of it, so the only safe arrangement is that the agent cannot type. |
| Providers | Presets include OpenAI, Anthropic, OpenRouter, DeepSeek, Z.ai, Groq, xAI, Google, **OpenCode Go**, **OpenCode Zen**, Ollama, LM Studio, plus custom OpenAI-compatible/Anthropic endpoints. `GET /models` auto-detect merged over a bundled catalog with manual overrides. Keys live in Windows Credential Manager. A preset can be added more than once — two OpenCode Go plans, each with its own key — and **Duplicate** copies an instance's endpoint, headers, gateway session header, catalogue and model selection but never its key. Each provider card lists every model with a checkbox, a per-card filter and a section-wide search; the choice is stored as a denylist (`disabledModels`), so an untouched config and any newly discovered model are selected by default, and `autoSelectModels` makes a large gateway opt-in instead. Unselecting hides a model from every picker but does not stop a chat already using it, so rows still referenced by the app default, the lite model, the image or embedding model, or recents carry an "in use" chip. Image and embedding models are `AuxModelRef`s — a bare id (what an older config parses to, meaning "whichever provider serves it") or `{providerId, modelId}` to pin one when the same id is configured twice. |
| Usage | Settings → Usage shows live vendor limits for the providers whose vendor exposes an endpoint (OpenCode Go's rolling/weekly/monthly subscription windows, OpenRouter key credits, DeepSeek balance, Z.ai coding-plan quota), resolved from the provider's base URL so custom endpoints work too. The composer shows the active model's tightest window (or remaining credit) as a small badge that deep-links to the section, and a local table totals tokens plus list-price cost estimates per provider from the `loom.db` reply metadata. Readings are fetched on launch, every five minutes, and on demand; nothing is persisted. |
| Chat | Reasoning thinking panel + per-chat variant selector, persona library, lite-model auto titles, attachments (images, text/code files, text-layer PDFs). One panel per thinking spell — each kept in the stream where it happened, between the text and tool calls it produced, and independently collapsible. Messages typed while a reply runs are queued (drag to reorder, arrow to send now and interrupt, X to remove); the queue sends the next one as each turn finishes, and Stop parks it. **Titles are written in two passes.** The first fires the instant you send, from your own message alone, so the sidebar row has a real name immediately rather than sitting on "New chat" for the length of a turn; the second runs once the first reply has landed, shows the model the existing title *and* the exchange, and asks whether the name still fits — replying `KEEP` when it does, so the common case costs no write. Names are never taken from the user: only a title the auto pass wrote may be replaced, tracked per chat, and a name that no longer matches what was recorded was typed by hand and is left alone. `chat.autoTitle` actually gates all of this now — it was written by the settings UI and read by nothing. The `/` menu carries skills, saved prompts, and built-in commands as **badged rows with arrow-key navigation** — a `/id` keycap, a kind badge (command / skill / prompt), and Enter accepts the highlighted row. `/goal <text>` and `/todo <text>` set the state *and* start a turn, sending `Goal: …` or `Task: …` so the model actually works on it — a command that only recorded a goal left the text unread, which is what the prefix fixes; when a reply is already running they queue rather than drop. `/todos` opens the goal panel, `/plan <text>` and `/chat <text>` switch mode and send, and `/new` starts a chat. The composer also takes **`#` for another chat and `@` for a file in this workspace**, both with autocomplete: `#` is resolved on send to `#Title (id: abc12345)` so the model has the id `read_chat` needs while the text you wrote stays readable, and `@` inserts the relative path. A `#` trigger is deliberately strict about prose — it opens only when a chat matches, so "issue #42" stays a sentence. |
| Generated UI | A ```loom-ui fenced block renders as live, themed HTML inside the reply. Sanitized against an allowlist (DOMPurify; no scripts, event handlers, forms or iframes) and mounted in a shadow root styled from the app's design tokens, so it tracks dark/light and compact density on its own. The system prompt gains a guide (only when enabled) that names the fence, a small house class vocabulary and declarative actions — `send`, `draft`, `copy`, `open` — which are the only way a widget talks back. Oversized or fully-stripped blocks fall back to source. Toggle: Settings → General. |
| Rich replies | Code blocks render with Shiki highlighting (languages load on demand, github-light/dark-default), a language header, line numbers, and a capped, scrolling body; copy lives in the header and dims while the fence is still streaming. A ```markdown fence renders as a formatted document with a preview/source toggle, and the widget renderers stay off inside it so a fence in a document stays code. Markdown **links render as host badges** — `github.com` in a pill with an external-link glyph, the full URL in `title` — because a wall of underlined `https://` reads as noise. A shell command in the transcript is a **code chip with a status pill** (`running` / `exit N` / `failed` / `denied`), and one of your own messages carries a **badge for the command that produced it** (`Goal` / `Task` / `/plan`), so what a turn was asked to do is legible at a glance. Markdown links open in the OS browser instead of navigating the webview. |
| Tools | `datetime`, `list_dir`, `read_file` (with line ranges), `grep` (with an include glob), `find_files`, `create_dir`, `move_path`, `copy_path`, `delete_path` (always confirmed), `git_status`, `git_diff`, `git_log`, `write_file`, `edit_file`, `run_command`, `web_search`, `fetch_url`, `search_workspace`, `generate_image`, `spawn_agent`, `todo_write`/`todo_read`, `list_commands`/`command_output`/`stop_command`, plus every tool exposed by MCP servers. **No tool opens a console window**: every process Loom spawns — commands, `git`, MCP servers, the updater's swap script — is flagged `CREATE_NO_WINDOW` through one helper (`loom_core::process`), so `npm test` or a `git log` never throws a black rectangle over the desktop. `run_command` also tracks what it starts: it is given 120s, and a command still going after that is **not killed** — it keeps running and is adopted as a tracked command with an id, a log in `~/.loom/logs/`, and a Stop button in the Runs panel's **Shell** tab. Pass `background: true` for anything long-lived (a dev server, a watcher) to get an id back immediately; those survive the turn and a Loom restart (a row left `running` by a restart reads `orphaned`, since the process may well still be alive). At most 8 run at once, `stop_command` ends a process **tree** (so an `npm test`'s workers go too), and deleting a running command stops it first. `list_chats` and `read_chat` let the model reach the user's *other* conversations: the first lists them newest-first with id, title, workspace and last activity, and the second reads one as markdown, addressed by id, the eight-character id a `#mention` carries, an id prefix, or an exact title — reusing the same renderer as the Export button, and trimming a very long chat to head plus tail with the omission stated rather than silent. Both are read-only, so Plan and Review may consult them; **both are refused in Chat mode**, because "answer from the model and the web" must not quietly become "and anything else we have ever discussed". Hidden `kind = 'task'` sessions are unreachable, so a background subagent's transcript is not a chat the model can read. `todo_write` replaces the chat's live task list, which the goal/task panel above the composer renders and updates as the turn runs (and the model reads back each turn alongside the `/goal` objective); it only edits Loom's own state, so it needs no permission card and works in Plan mode. Permission modes: Ask / auto read-only / auto run-all, global default with per-chat override. Agent modes: Plan / Review / Build, also a global default with per-chat override — the read-only modes refuse the write and command tools outright (no permission card): Plan asks questions, researches, and proposes a plan; Review reports severity-ranked findings with file and line plus proposed fixes, changing nothing. Agent mode, approvals, and computer use share one composer chip (`Build · Auto all`) with one grouped menu. Web tools prefer Jina AI (search + reader) when a key is stored, and fall back to DuckDuckGo's HTML endpoint and a local tag stripper without one. A fourth mode, **Atelier**, runs everything Auto all runs and additionally exposes the harness tools — `list_harness`, `upsert_persona`/`delete_persona`, `upsert_prompt`/`delete_prompt`, `write_skill`/`delete_skill`, `upsert_mcp_server`/`delete_mcp_server`/`test_mcp_server`, `upsert_provider`/`update_model`/`delete_provider`, `update_settings` — which let the model edit Loom itself. Harness tools are invisible in every other mode and refused outright if called, and every config write takes a `backups/` snapshot first. **Atelier does not card its deletes**: the five harness removals, a workspace path, and a scheduled job all run without asking, because the mode is a deliberate per-chat handover and a card is a question it has already answered. The one thing still carded is `schedule_job` — not a removal but a standing commitment to act while nobody is watching. Note what that exemption costs: the harness deletes are recoverable from `backups/`, but a deleted workspace path and a deleted job are not backed up by anything, so Atelier is the single place in Loom where a model can destroy work irreversibly and unprompted. `Auto all` deliberately does *not* inherit the exemption: its `delete_path` risk check still fires. Atelier is deliberately per chat and is never accepted as the global default. |
| Computer use | Per-chat **Computer chip** (Windows only, needs a vision model) arms `screenshot`, `mouse`, `keyboard`, `ui` (UI Automation), `clipboard`, `list_windows`, `window`, `list_processes`, `launch_app`, `kill_process`, and `wait`. The chip is the standing consent, so actions run without per-action cards, and switching it off stops the running turn at once. **Takeover**: a real click, scroll or key press pauses the turn — bare mouse movement does not, and neither does anything landing on one of Loom's own windows (the main window, the quick-ask overlay, the control pill), so reading the transcript or pressing Resume is not mistaken for taking the machine back. The trip latch is edge-triggered and consumed by the bridge, and `resume_computer` clears any pending trip, so a Resume is not undone by the click that asked for it. A paused turn resumes after 30 s of input silence (shown as a countdown in the pill) or on Resume, and gives up after 15 minutes with a Notice. `Ctrl+Alt+Esc` and the pill's Stop end only the chat that is driving the machine; other chats and detached runs are untouched, and the stopped turn records why. When the input hooks cannot be installed, the failure is reported rather than swallowed and the pill says takeover detection is off. A read-only agent mode (Plan/Review) narrows the chip to the read-only computer tools — screenshots and listings — and the prompt says so instead of advertising a mouse it will refuse. Screenshots default to native resolution (`chat.computerScreenshotEdge` can downscale), are stored in `attachments/<session>/` (newest 200 per chat), shown in the transcript, and inlined into the wire only for the newest capture, so context stays flat across long turns. Mouse coordinates are image pixels from the last screenshot; the engine maps them back through the captured rect and scale, and `scroll` aims the pointer at the coordinates given. `ui` ids expire by time as well as by re-read, are refused when disabled or off screen, and are verified against the element's name before acting. `keyboard type` pastes long text through the clipboard and restores it as an OLE data object — images and copied files included — falling back to typing when the clipboard cannot be read. Latency knobs: `chat.computerVariant` (default `low` thinking), an optional `chat.computerModel` fast vision model used only while armed, old-turn computer chatter compacted to one line on the wire, and the context budget reserves image tokens for the inlined screenshot. Single-turn lock: one chat drives the machine at a time, and the hooks go up only for the chat holding it; a per-turn guard releases hooks, the lock, the cached shot and the lock even if a tool panics. All synchronous Win32 work (captures, input, UI Automation, `launch_app`) runs on blocking threads rather than the async runtime. |
| Workspaces | Folders the user adds are kept in `config.json`, not just on the chat that used them. The composer chip switches, renames, and forgets them; the chats popup groups history by workspace and starts new chats in a group. **Grouped mode is ordered by hand**: a workspace header or a chat row is dragged (native HTML5 DnD — no dependency), and the order is the user's from then on. A chat you have never dragged still sorts newest-first *above* the ones you placed, so a chat started a moment ago lands on top of a group arranged last week; a saved folder with no chats in it is still listed, ranked by when you added it, which is what makes "the folder I just picked is at the top" visible. Chat rows live in a `position` column on `sessions` (migration v12, `reorder_sessions`); group order is `interface.sidebarWorkspaceOrder` and the collapsed set is `interface.sidebarCollapsedGroups` in `config.json` — **a fold survives a restart**, because collapsing the four folders you are not working in is how the list is made navigable, and redoing it every launch would defeat the point. Opening the popup reveals the group holding the open chat as a courtesy for that visit, kept in memory rather than written back, so the reveal can never quietly undo a fold you chose. The Sort menu therefore governs the **flat list only** — in grouped mode it says so rather than pretending. Each group shows **5 rows** with a `Show N more`, except that the open chat is always among them; the cap resets when the popup closes, and a search ignores it. |
| Retrieval | Per-chat workspace index in SQLite: files chunked (1.6k chars, 200 overlap), embedded via the chat provider's `/embeddings` endpoint (default `text-embedding-3-small`), ranked by cosine similarity in-process. |
| Condensing | A chat too long for the model's window is **condensed, never truncated with a note**. The last user turn and everything after it stay verbatim; older turns fold into one synthetic block at the head of the wire, capped at `chat.condenseShare` (default 20%) of the model's window and at a third of the request budget. The budget is divided **tail first**, so a generous summary can never elide the fresh tool output the live turn is working from. The block is built two ways: a deterministic in-process digest (the goal, what was asked, files changed, commands run, what was concluded — free, instant, needs no key) goes out immediately, while a lite-model summary is written in the background once a request passes 70% of its budget and is used from the next turn on. The stored summary records the message it covers *through*; a fold whose message has since been edited away is ignored and the digest stands in. Each reply answered from a folded view carries one **faint line** in the slot the token-usage line occupies — `Condensed · 24 earlier messages`, expandable to the exact text the model was given — and `interface.showCondensing` turns it off. A condensed turn ends with `Done` like any other: it is not a stop, and it must never offer a Retry. Setting the share to `0` restores the old drop-the-oldest behaviour. |
| Storage | SQLite in `~/.loom/loom.db` (migrations v1→v12), config in `~/.loom/config.json`, attachments/backgrounds/generated/skills/cache/backups/logs under `~/.loom`, provider and Jina API keys in Windows Credential Manager. `backups/` holds the config snapshots taken before every harness edit (the newest ten are kept) and copies of overwritten skill files. `logs/` holds one `cmd-<id>.log` per tracked shell command (capped at 5 MB). |
| Distribution | Bespoke glass **Loom Setup** app (`setup/`): a single portable exe with the payload embedded — no installer framework at all. Extracts the app, writes shortcuts, registers an HKCU uninstall entry, and drops an uninstaller in `%LOCALAPPDATA%\Loom` (outside the install folder, so it can delete it). Supports `--silent --dir <path>` for scripted installs. Ground-up updater: `update.json` on GitHub releases, semver compare, SHA-256 + minisign verification, staging, swap-after-exit script. |

## Roadmap order

MCP → web search → image generation → skills & commands → multi-agent →
coding agent. All of the above are implemented; the coding agent is the
deepest (workspace folder, `read_file`/`write_file`/`edit_file`/`run_command`,
`AGENTS.md` injection, diff preview, per-chat permission modes).

## Repo layout

```
loom/
  src/                 React UI (components/, stores/, lib/, dock/)
  src/dock/            The dock: zones, splitters, tab strips, panel registry
  src-tauri/           Tauri shell: window, tray, hotkey, overlay, commands,
                       panels (the dock's commands, the pty sink, tear-off)
  crates/loom-core/    Engine: config, db, providers, tools, mcp, web, images,
                       skills, attachments, workspace, updater, dock, pty
  setup/               Loom Setup (bespoke glass installer, Tauri app)
  scripts/             icon generation, payload packaging
  docs/spec.md         this file
```

## Open work

- Light theme over bright artwork is usable now (stronger glass + ink) but a
  proper design pass would still help.
- **Refraction is quiet over Loom's own background presets.** Measured on the
  pills: 0% of pixels bend over a built-in preset (max channel delta 2), against
  87.75% and a delta of 111 over a hard-edged pattern. That is the backgrounds'
  doing rather than the wrapper's — they are deliberately smooth radial washes
  by design (see `lib/background.ts`), and a displacement map has nothing to bend
  in a smooth gradient. It reads properly over a user's own photograph or video.
  Three ways out if it matters: add a faint high-frequency texture to the
  presets, raise the default refraction, or accept that this is an effect for
  people who supply their own artwork. Not decided.
- The composer is the one surface whose cost is worth watching in a real window:
  it is the largest frosted area and the only one that resizes as you type. The
  isolated measurement says the filter adds ~0.5ms there, but the whole-app
  frame p50 sits on a 48fps boundary (20.8ms) with the pills and composer both
  live, against 60fps with the pills alone. If that turns out to be visible, the
  Composer toggle in Settings → Appearance is the first thing to turn off.
- The workspace index is brute-force cosine over stored chunks; fine to a few
  thousand chunks, worth an ANN index beyond that.
- Windows code signing needs a certificate; the workflow is wired and skips
  cleanly without one.
- `stop_command` kills the process tree with `taskkill /T /F`, which is best
  effort: a process that deliberately reparents its workers can escape it, and
  the row then reads `orphaned` on the next launch.
- Hiding the updater's swap script means a failed swap is silent in the UI; it
  writes `%TEMP%\loom-update.log` instead, which is diagnosable but easy to
  miss.
- The dock has not been driven by hand in a running app. The layout model, the
  PTY and the geometry clamps all have tests — including one that opens, writes
  to and closes a real shell — but the splitter feel, the drag ghost, the
  tear-off gesture and the look of the terminal at various font sizes need a
  real window with a real pointer.
- A panel's width is stored per workspace, but the *region* the clamp measures is
  a `ResizeObserver` value, so the first frame after a window resize can clamp
  against a stale extent. It corrects on the next frame; a sub-pixel jump on a
  window resize is the visible cost.
- Tearing off by dragging out of the window is the only gesture that cannot be
  undone with `Escape` once released — the panel is already in its own window,
  and *Dock it back* is the return path.
- `Files` shows the workspace file list, not a change set. There is no git diff
  command, and deriving one from a list of paths would be inventing it.
- `Browser` is a registered placeholder, not a surface.
- The chunk coalescing in `pty.rs` is deliberate but unmeasured. If a large
  paste ever feels slow, that is the first place to look.

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
- Launch at login: a registry-backed toggle in Settings → General (the app's
  autostart plugin) and a "Start Loom when you sign in" checkbox in Loom Setup;
  both write the same per-user Run entry, and uninstalling removes it.
- Every launch starts on a blank new chat. Sessions are still listed in the
  chats popup, but the canvas no longer reopens the last conversation; the new
  session is created on the first send (or picked from the popup), and every
  composer chip that writes to a chat — mode, approvals, persona, computer use
  — starts one too instead of dropping the click.
- Loom Setup polish: a plain white window in the app's light palette (Inter,
  squircle corners, switches, the dark control buttons) instead of the old
  gradient; byte-level progress from Rust during extraction; an async install
  command so the window keeps painting; a running Loom is closed before an
  in-place update; console windows stay hidden; and the update screen shows the
  real version instead of a derived one. The payload builder copies only the
  app (`loom.exe` + runtime DLLs, never `loom_lib.dll` or anything else in
  `target/release`), and the dev-only payload sidecar is ignored by release
  builds, so a stray zip can no longer shadow what an installer ships.
- Loom Setup follows the install to the end: phases (closing a running app,
  extracting each file, shortcuts, registry) are reported to the UI, the bar
  sweeps while a phase has no byte count, and the window refuses to close
  mid-install. Enter runs the primary action and Esc closes. An update finds the
  app through its uninstall entry wherever it was installed — custom folders
  included — says "Reinstall" when the version already matches, shows the
  installed size in Settings → Apps, offers Try again after a failure, and says
  why a launch could not start.
- Opening screen: the blank-chat hero weaves its mark in (the warp threads,
  then the weft) and the greeting, subline and composer card follow on a short
  stagger. The mark had been dropping the class names every caller passed it, so
  it now wears the accent color and spacing they already asked for.
- The quick-ask overlay follows the app theme instead of forcing dark: user
  prompts are glass slabs with a thread edge, replies are the same loose text
  (on a pane of fog in light mode so dark ink stays readable over artwork),
  and the column hangs off a single warp thread with a knot per reply. Its
  colors come from `--thread-*` tokens in both palettes; the whole thing stays
  transparent and minimal. Waiting is three weft threads with light passing
  along them, and the composer's hairline is one thread that sweeps while a
  turn runs.
