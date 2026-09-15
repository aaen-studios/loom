# Loom

Desktop app for AI chat and agents. Built with Tauri v2, React 19, Tailwind
v4, and a Rust engine (`crates/loom-core`).

**Status: M0 — shell.** The window, glass surfaces, theming, and config
persistence are real. Providers, streaming, sessions, and tools arrive in
M1–M3. See [docs/spec.md](docs/spec.md) for the full plan.

## Development

```bash
bun install
bun run tauri dev
```

Frontend-only (browser, IPC mocked to defaults):

```bash
bun run dev
```

## Checks

```bash
bun run typecheck
cargo test -p loom-core
cargo check -p loom
```

## Layout

```
src/                 React UI
src-tauri/           Tauri shell (window, commands, capabilities)
crates/loom-core/    engine: config, paths, storage, providers, tools, updater
docs/spec.md         product + architecture spec
```

## Data

Everything lives in `~/.loom` (`config.json`, `loom.db`, `backgrounds/`,
`logs/`). Override with the `LOOM_HOME` environment variable.

## Icons

Replace `src-tauri/icons/icon.svg` and run:

```bash
bun run icons
```
