# desktop

Rust: the GPUI app for Windows and macOS. It draws screens and reaches the system (tray, notifications, starting at login, screenshots, the clipboard, the updater), and leaves everything else to the [core](../core/), which it links.

GPUI comes from GPUI Kit (`gpui-kit`), pinned exactly in `Cargo.toml`. See [PLAN.md: Desktop on GPUI](../PLAN.md#desktop-on-gpui-what-to-know).

## Building

You need a recent stable Rust (`rustup update stable`) and `calx` on PATH. `build.rs` builds the core (`core/lib.calx`) into a static library with `calx build --staticlib` and links it, so `cargo` is the only command:

```sh
cargo run --release
```

Set `CALX` to use a different `calx` binary.

## What's here

- `src/main.rs`: starts GPUI, loads Geist (in `assets/fonts`, under the OFL), puts GPUI Kit's widgets in loom's colours, and opens the window with loom's own title bar.
- `src/core.rs`: the core's C boundary. Its events come on the core's thread and go to the window over a channel.
- `src/model.rs`: what the screens show, as the core tells it. Lists come from the core's cache with `core.get` and are fetched again when an event says they changed. Only a reply still being written lives here alone, from its deltas.
- `src/app.rs`: the window. It has the sidebar of agents, the header with the agent's menu and its permission mode, and the chat, and an agent's edit page, with its name, its instructions and every revision to go back to, and its activity page: what it ran, what it was allowed, and what changed, and its memory page: its notes and the shared ones, to add to or delete. The sidebar's **About you** is the person's profile, which every agent starts with, to edit. **Routines** lists the routines, with when each runs next and how its last run went, to make, change, pause, run now or delete, and with a webhook, its address and secret to copy. A message a routine sent shows "From the routine …" in the chat. **Skills** lists the person's skills, to add, change or delete, and **Agent programs** lists the instance's ACP agents, with their settings, and the ACP Registry's, to install, update or remove. **MCP servers** sets up the person's MCP servers and the keys they use, which each agent picks on its edit page. **This PC** (when the instance is somewhere else) sets what agents may do on this computer: run commands, read files, list folders, open things, take screenshots and use the clipboard, each off, ask or allow. It does the screenshots (with `xcap`) and the clipboard when the core asks (`core.device_ask`). **Add a phone** (also only for an instance somewhere else) draws the QR code a phone scans to join the account (with `qrcode`), says when it has, and lists the devices signed in, to sign one out. The chat shows replies, thoughts, tool calls, questions, files and pictures, and the composer, which takes files from the paperclip, dropped on the chat, or pasted (an image, or files copied in Explorer). Search in the sidebar finds words in every chat, and in file names. There are also the sign-in and new-agent pages. The look follows `design/explorations/round-4.html`, with every value from `design/tokens.toml` (`design/generated/tokens.rs`).
- `src/thread.rs`: an agent's thread, drawn as one path that moves with what the agent is doing.
- `src/shell.rs`: the tray (Open, Start at login, Quit), Windows notifications for what the core says is worth one, starting at login with `--background`, and hiding the window to the tray when it's closed. A tick that comes long after the last means the computer slept, so the app tells the core (`app.wake`). It also turns a pasted bitmap into a PNG.

Not here yet: the updater (LOO-29), the instance in the app (LOO-27), and files, images and search (LOO-28).

## Trying it

Run an instance (`calx server/main.calx serve`), then start the app and sign in with its address, a username, a password and, the first time, the setup code it printed. Or choose **Run one on this computer** on the sign-in page: the app keeps that choice in `mode` in its data folder, and runs the instance itself from then on, with its data in `instance` there. **Sign out** goes back to the sign-in page and stops it; its agents and chats stay for next time. While developing:

- `LOOM_SERVER`: an instance's address, filled in on the sign-in page.
- `LOOM_DATA`: the folder for the core's cache, instead of `%APPDATA%\Loom`.
- `LOOM_THEME`: `dark` or `light`, instead of following the system.
- `LOOM_UPDATE`: a release manifest's address, for trying updates in a build from source, which doesn't update itself otherwise.

## Releases

A tag like `v0.3.0` runs `.github/workflows/release.yml`. It builds `loom.exe` with the version in it (`LOOM_VERSION`), and `loom-setup.exe` from `installer.iss` with Inno Setup. It also builds the server for Linux and Windows, and publishes them all with a `manifest.json` that `scripts/release.calx` signs with the release key, from the CI secret `LOOM_RELEASE_KEY`. The tag's message is the release notes.

The installer puts loom in `%LOCALAPPDATA%\Programs\loom`, for the person alone, and gives its Start menu shortcut the AppUserModelID `AaenStudios.Loom`, so notifications show as loom. From then on loom updates itself. The core finds and checks a release, the sidebar offers "Restart to update", and `shell::apply_update` renames the running `loom.exe` to `loom.old.exe`, puts the new one in its place and starts it. The new one waits for the old one to end, then removes it. "Check for updates" in the tray looks at once.
