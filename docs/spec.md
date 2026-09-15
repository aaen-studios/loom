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
| Glass | The app renders its own background (built-in presets + user images/videos) and uses CSS `backdrop-filter` only on small surfaces (composer, popups, cards). |
| Streaming | Rust-owned streams: concurrent across chats, survive window switches and backgrounding. Toast when a reply finishes unfocused. |
| Overlay | Tray icon + `Ctrl+Shift+Space` quick-ask overlay (new chat, then reveals the main window). |
| Providers | Presets include OpenAI, Anthropic, OpenRouter, DeepSeek, Z.ai, Groq, xAI, Google, **OpenCode Go**, **OpenCode Zen**, Ollama, LM Studio, plus custom OpenAI-compatible/Anthropic endpoints. `GET /models` auto-detect merged over a bundled catalog with manual overrides. Keys live in Windows Credential Manager. |
| Chat | Reasoning thinking panel + per-chat variant selector, persona library, lite-model auto titles, attachments (images, text/code files, text-layer PDFs). |
| Tools | `datetime`, `list_dir`, `read_file`, `grep`, `write_file`, `edit_file`, `run_command`, `web_search`, `fetch_url`, `search_workspace`, `generate_image`, `spawn_agent`, plus every tool exposed by MCP servers. Permission modes: Ask / auto read-only / auto run-all, global default with per-chat override. |
| Retrieval | Per-chat workspace index in SQLite: files chunked (1.6k chars, 200 overlap), embedded via the chat provider's `/embeddings` endpoint (default `text-embedding-3-small`), ranked by cosine similarity in-process. |
| Storage | SQLite in `~/.loom/loom.db` (migrations v1→v3), config in `~/.loom/config.json`, attachments/backgrounds/generated/skills/cache under `~/.loom`. |
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

## Done since the first pass

- Markdown/streamdown is code-split (main chunk ~316 kB; the 486 kB markdown
  chunk loads on the first reply).
- Streaming deltas are coalesced in the engine (96 chars for text, 256 for
  reasoning) instead of one event per token.
- Transcript scrolling only follows output while pinned to the bottom, with a
  "jump to latest" affordance.
- Retry on failure drops the failed turn and re-sends, rather than duplicating
  history.
- Chats popup: search, double-click rename, export to markdown, busy dots.
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
