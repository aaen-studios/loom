# Loom

Desktop app for AI chat and agents. Tauri v2, React 19, Tailwind v4, and a
Rust engine (`crates/loom-core`).

**Status: feature-complete for v0.1.** Providers (including OpenCode Go/Zen),
streaming chat with reasoning, attachments, tools with permission modes
(including **Atelier**, which additionally lets the model edit its own harness:
personas, MCP servers, skills, prompts, providers and settings), MCP servers,
skills, image generation, subagents, a coding workspace with semantic search,
`/` commands and a live goal/task panel the model keeps checked off, shell
commands run windowless (and long ones run in the background, listed and
stoppable from the Runs panel), provider
usage and subscription limits (Settings →
Usage, plus a composer badge for the active model), **computer use** (Windows:
`Ctrl+Alt+Esc`
panic stop and screenshots in the transcript), tray + hotkey overlay,
launch at login (Settings → General), a fresh chat on every launch,
signed updater, and a bespoke glass installer. See [docs/spec.md](docs/spec.md).

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
node scripts/probe-streaming.mjs 9333 "write a paragraph"       # is text rendering progressively?
node scripts/probe-newchat.mjs 9333                             # new chat: streaming + auto title
node scripts/probe-state.mjs 9333                               # real store state during a turn
node scripts/probe-computer.mjs 9333                           # armed chip: screenshot + cursor, timed
node scripts/probe-appearance.mjs 9333                          # screenshots of both themes
```

`inspect-webview` prints uncaught exceptions and console output (this is how the
React render loop, the Tokio runtime panic, and the event-format mismatch were
found), and the screenshot is written to disk so it can be inspected directly.

Frontend diagnostics (`[loom] <- delta`, `[loom] apply delta`) turn on with
`localStorage.setItem("loomDebug","1")` — that also exposes the stores as
`window.__loom` for inspection.

## Layout

```
src/                 React UI
src-tauri/           Tauri shell (window, tray, hotkey, overlay, commands)
crates/loom-core/    engine: providers, tools, mcp, index, storage, updater
setup/               Loom Setup (bespoke installer)
site/                marketing site (Next.js) — see below
docs/spec.md         product + architecture spec
```

## Website

`site/` is the public landing page at **[loom.rip](https://loom.rip)**: Next.js
and Tailwind v4, deployed from this repository. It is not a separate project
with its own copy of the design — it is styled by the **same declarations** the
app uses.

```bash
cd site
bun install
bun run dev            # http://localhost:3000
bun run verify         # everything CI runs: tokens, typecheck, build, checks
```

### Shared tokens

`src/styles.css` marks the regions the website needs with
`loom-site:<name>:start` / `:end` comments. `scripts/sync-site-tokens.mjs`
extracts them verbatim into `site/src/app/loom-tokens.css`, which the site
imports. So the app's own `@utility` surfaces — `panel`, `pill`, `btn-primary`,
`chip`, `rounded-window`, the `loom-*` keyframes — exist in the site with no
reimplementation, and the two cannot drift in behaviour because there is one
copy of the declarations.

After changing a marked region, regenerate:

```bash
cd site && bun run tokens
```

CI runs `tokens:check` and fails when the generated file is stale, so a change
to `src/styles.css` cannot reach `main` without the site following it. Four
rules keep the regions extractable, and the script enforces all of them: each
marker appears exactly once, regions never nest, a region is never empty, and
they stay in source order.

`site/GATE-TEST.md` records the experiment that made this approach valid —
proving Tailwind v4 resolves `@utility`, `@theme` and `@custom-variant` when
they arrive through an `@import` rather than in the entry stylesheet.

### Site checks

Six gates, all runnable together with `bun run verify` from `site/`:

- **`tokens:check`** — the generated token sheet matches `src/styles.css`. This
  is the one that makes the copying safe: a change to a marked region cannot
  reach `main` without the regenerated sheet beside it.
- **`bun test`** — the hero's scripted turn. The animation is invisible to every
  other check the project has: `next build` cannot tell that a beat was edited
  into the wrong order, and the page verifier only ever sees the first frame,
  which is the empty state. So the timeline is a pure module (`src/lib/timeline.ts`)
  and is tested for the invariants the animation depends on.
- **`typecheck`** — `tsc --noEmit`.
- **`verify:tokens`** — greps the **built** CSS for the app's real tokens, and
  for the app-only rules that must *not* be there (the shell's
  `overflow: hidden`, the quick-ask overlay, the markdown pipeline). Catches a
  build that succeeds while the shared styles silently went missing.
- **`verify:pages`** — asserts the prerendered HTML contains what the pages
  claim, including that the download page reports its release state truthfully
  rather than falling back to a hardcoded version unnoticed.
- **`verify:a11y`** — heading structure, accessible names, alt text and
  in-page link targets, read from the built HTML. This is what caught the
  footer's column headings skipping a level on the 404.

`scripts/test-site-token-sync.mjs` is a seventh, run locally rather than in CI
because it temporarily edits `src/styles.css`: it breaks a marker in seven
different ways and checks the sync fails loudly for each, then confirms the file
is restored byte-for-byte.

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
| `backups/` | config snapshots taken before harness edits (newest ten) and overwritten skill files |
| `logs/` | `cmd-<id>.log` per tracked shell command (capped at 5 MB each) |
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

```powershell
# 1. Build the app (this also builds the frontend), then zip it into a payload.
bun run tauri build --no-bundle                # target\release\loom.exe
node scripts/make-payload.mjs                  # setup\src-tauri\payload.zip

# 2. Setup embeds the payload at compile time, so it must come last.
Push-Location setup
bun run tauri build --no-bundle                # target\release\loom-setup.exe
Pop-Location

.\target\release\loom-setup.exe                # or --silent --dir <path>
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

`src-tauri/icons/icon.svg` is the single source of truth for the app mark — the
three thread paths, the 1.9 stroke width, the 0.55 weft opacity. Redraw that file
and run:

```bash
bun run icons
```

That one command:

1. rasterises the source to `icon-1024.png`, and bails if the source has drifted
   from the shape the rest of the project assumes (a missing thread, a changed
   stroke, or an `rx` on the OS icon);
2. derives `icon-round.svg`, `index.html`'s favicon data URI, and
   `site/src/app/icon.svg` from it, so no browser icon is hand-maintained;
3. runs `tauri icon` to regenerate every raster, the `.ico`, and the `.icns`;
4. copies the `.ico` into `setup/`, verifies the copy byte-for-byte, and touches
   both `tauri.conf.json` files so the build re-embeds the Windows resources.

Two shapes are deliberate. The **OS** icons — desktop shortcut, taskbar, tray,
Alt-Tab, the installer — are full-bleed squares, because Windows and macOS supply
their own corner radius. The **browser** icons (the dev favicon, the site icon)
carry the window radius themselves. `src/lib/iconConsistency.test.ts` asserts
both, and that every shipped raster is greyscale.

Three React copies of the mark remain by hand — `src/components/icons.tsx`,
`setup/src/App.tsx`, and `site/src/components/loom-mark.tsx`. A regeneration
cannot reach them, so the same test checks their geometry too.

### A stale logo on a shortcut

The icon is embedded into `loom.exe` at link time, so rebuilding the artwork
alone does not change a binary that was already linked. Two guards exist:

- `scripts/make-payload.mjs` refuses to pack a payload whose `loom.exe` is older
  than `icon.svg`, `icon.ico`, or the app's `tauri.conf.json`, and refuses if the
  app's `.ico` and Setup's differ.
- Setup writes a dedicated `loom.ico` into the install folder and points each
  shortcut's `IconLocation` and the uninstall entry's `DisplayIcon` at it, rather
  than at `loom.exe`. Explorer caches an icon per file path, and a path an update
  rewrites is exactly the one it is free to keep serving from cache; a fresh path
  has no entry to inherit. Setup then nudges the shell cache, best-effort.

An already-pinned shortcut may still need one unpin/repin. The icon of the
installer exe sitting in your downloads folder is Explorer's cache keyed to that
path, which no installer can reach backwards into.
