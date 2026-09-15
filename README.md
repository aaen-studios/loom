# Loom

Desktop app for AI chat and agents. Tauri v2, React 19, Tailwind v4, and a
Rust engine (`crates/loom-core`).

**Status: feature-complete for v0.1.** Providers (including OpenCode Go/Zen),
streaming chat with reasoning, attachments, tools with permission modes, MCP
servers, skills, image generation, subagents, a coding workspace with semantic
search, tray + hotkey overlay, signed updater, and a bespoke glass installer.
See [docs/spec.md](docs/spec.md).

## Development

```bash
bun install
bun run tauri dev
```

Frontend-only (browser, IPC falls back to defaults):

```bash
bun run dev
```

## Checks

```bash
bun run check:encoding     # no UTF-8 BOMs (breaks Vite on Windows)
bun run typecheck
bun run test               # vitest: stores + pure helpers
cargo test -p loom-core    # engine
cargo check -p loom -p loom-setup
```

## Diagnosing the running app

Start the app with the DevTools protocol enabled, then use the helper scripts:

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9333"
.\target\debug\loom.exe
```

```bash
node scripts/inspect-webview.mjs 9333 target/shot.png --reload  # console errors + screenshot
node scripts/inspect-dom.mjs 9333                               # what is actually on screen
node scripts/drive-ui.mjs 9333 "hello"                          # sends a message through the real UI
```

`inspect-webview` prints uncaught exceptions and console output (this is how the
React render loop and the Tokio runtime panic were found), and the screenshot is
written to disk so it can be inspected directly.

## Layout

```
src/                 React UI
src-tauri/           Tauri shell (window, tray, hotkey, overlay, commands)
crates/loom-core/    engine: providers, tools, mcp, index, storage, updater
setup/               Loom Setup (bespoke installer)
docs/spec.md         product + architecture spec
```

## Data

Everything lives in `~/.loom`:

| Path | Contents |
| --- | --- |
| `config.json` | theme, background, providers, personas, MCP servers, chat defaults |
| `loom.db` | sessions, messages, and the workspace index (SQLite) |
| `attachments/` | files sent in chats, per session |
| `backgrounds/` | your own background images/videos |
| `generated/` | images produced by the `generate_image` tool |
| `skills/` | markdown skills, surfaced in the composer's `/` menu |
| `cache/` | downloaded update payloads |
| `keys/` | **release signing keys (maintainers only)** |

API keys are stored in Windows Credential Manager, never in these files.
Override the location with `LOOM_HOME`.

## Workspace indexing

Pick a folder from the workspace chip, then **Index workspace**. Files are
chunked, embedded with your chat provider (default `text-embedding-3-small`,
configurable in Settings → Chat), and stored per chat. The model can then call
`search_workspace` for semantic lookups; `grep` still handles exact matches.

## Releasing

```bash
# bump the version in package.json, src-tauri/tauri.conf.json, crates/loom-core/Cargo.toml
git tag v0.1.1
git push --tags
```

The release workflow builds the app, signs the binary if a certificate is
configured, packages the payload, embeds it into **Loom Setup** (one portable
exe), and publishes `update.json`. Users get the update in Settings → Updates
(or a toast ~10s after launch when one exists).

Installing locally, without the workflow:

```bash
bun run tauri build --no-bundle              # target/release/loom.exe
node scripts/make-payload.mjs                # setup/src-tauri/payload.zip
cd setup && bun run tauri build --no-bundle  # target/release/loom-setup.exe
./target/release/loom-setup.exe              # or --silent --dir <path>
```

Uninstalling: add/remove programs, or run
`%LOCALAPPDATA%\Loom\uninstall.cmd`.

### Signing keys

- Private key: `~/.loom/keys/loom.key` (generated with `minisign -G -W`).
  Add its contents as the `LOOM_MINISIGN_KEY` repository secret. Never commit
  it.
- Public key: compiled into `crates/loom-core/src/updater.rs`
  (`UPDATE_PUBLIC_KEY`). Rotating the keypair means updating that constant.
- Test keypair: a throwaway pair (`~/.loom/keys/loom-test.*`) whose public key
  and a fixture signature are embedded in the updater tests, so CI proves the
  verification path on every run.
- Code signing (optional): set `WINDOWS_CERT_BASE64` and
  `WINDOWS_CERT_PASSWORD` to sign the app and installer.

## Icons

Replace `src-tauri/icons/icon.svg` and run:

```bash
bun run icons
```
