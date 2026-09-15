# Loom — spec

Desktop app for AI chat and agents. Starts as a beautiful, fast chat client
(text + images) and grows into a full agent workspace (tools, MCP, skills,
coding agent). Windows 11 first.

Status: **M0 (shell)** — window, glass, theme, config persistence. Engine
(providers, streaming, sessions) is M1.

## Locked decisions

| Area | Decision |
| --- | --- |
| Stack | Tauri v2 + Vite + React 19 + TS + Tailwind v4, Bun. Rust engine in its own crate (`crates/loom-core`). |
| Glass | Frameless opaque window; the app renders its own background layer (built-in presets now; image/video in M2) and glass surfaces are CSS `backdrop-filter`. No OS mica. |
| Chrome | Custom-drawn window controls (Windows-style glyphs) on a floating glass titlebar. Inter font. Light theme by default, dark available. |
| Shell | Collapsible sidebar + canvas. History list is task-ready (status indicators when agents land). |
| Streaming | Rust-owned streams: concurrent across chats, survives window switches and app backgrounding. Toast when a response completes unfocused. |
| Overlay | Tray icon + `Ctrl+Shift+Space` quick-ask overlay (defaults to a new chat; chip can target an existing one). |
| Providers | ZCode-shaped config: presets + custom (`kind`: openai / openai-compatible / anthropic), base URL, custom headers, API keys in Windows Credential Manager. `GET /models` auto-detect merged over a models.dev-style catalog with manual overrides. Ollama/LM Studio local presets with autodetect. Manual setup (no import from other tools). |
| Chat | Reasoning thinking panel + per-model variant/effort selector (persisted per chat). Persona library (UI-managed config entries). Lite model auto-titles chats. |
| Attachments | Images, text/code files, text-layer PDFs (no OCR; scans are rejected with a clear message). |
| Storage | SQLite (`rusqlite`, migrations) in `~/.loom/loom.db`. Config in `~/.loom/config.json`. Keys in the OS keyring. |
| Tools (M3) | Rust tool loop; starter tools `read_file` (scoped to the chat's chosen folder) and `datetime`; workspace + git-branch chips in the composer; permission modes Ask / auto read-only / auto-run-all, with a global default and per-chat override. |
| Distribution | Bespoke glass **Loom Setup** app (payload archive, install dir, shortcuts, uninstall entry — no NSIS skinner plugin). Ground-up payload-swap updater: `update.json` on GitHub releases, semver check, download with progress, minisign verification, staged swap on restart. Unsigned; fresh minisign keypair. |
| Repo | `aaen-studios/loom`, private. Identifier `com.ellio.loom`. |

## Roadmap order

M4 MCP support → M5 web search tool → M6 image generation → M7 skills &
commands → M8 multi-agent → M9 coding agent (file edits with diff approval,
terminal, @-file refs, AGENTS.md).

## Repo layout

```
loom/
  src/                 React UI (components/, stores/, lib/)
  src-tauri/           Tauri shell: window, commands, capabilities
  crates/loom-core/    Engine: config, paths, storage, providers, tools, updater
  setup/               Loom Setup app (bespoke installer) — added at release milestone
  scripts/             icon generation, packaging helpers
  docs/spec.md         this file
```

## Milestones

- **M0** Scaffold, frameless glass shell, custom chrome, sidebar/canvas,
  settings (theme/background), config persistence, placeholder icons.
- **M1** `loom-core` engine: providers + keyring + SSE streaming → Tauri
  events; provider settings with test + `/models` fetch; model picker;
  reasoning panel + variant selector; personas; SQLite history; concurrent
  background streams; tray + hotkey overlay; lite-model titles.
- **M2** Attachments (images/files/PDF), paste + drag-drop, background picker
  (image/video), light/dark polish, toasts, error/retry, update-check UI.
- **M3** Tool loop + permission modes, `read_file`/`datetime`, workspace +
  branch chips, tool-call cards.
- **M4–M8** MCP, web search, image generation, skills/commands, multi-agent.
- **M9** Coding agent.
- **Release** Loom Setup app, ground-up updater, CI + release workflow.

## Conventions

- Rust: `loom-core` never depends on Tauri; the shell is a thin command layer.
  Config writes are atomic. Tests for every pure function that can have one.
- Frontend: small components, zustand stores, `call()` wrapper for IPC (fails
  soft to `null` outside Tauri so `vite dev` works in a browser).
- No dead controls: UI that has no behaviour yet says so, or is absent.
