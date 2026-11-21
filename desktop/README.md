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
- `src/app.rs`: the window. It has the sidebar of agents, the header with the agent's menu and its permission mode, and the chat. The chat shows replies, thoughts, tool calls, questions and the composer. There are also the sign-in and new-agent pages. The look follows `design/explorations/round-4.html`, with every value from `design/tokens.toml` (`design/generated/tokens.rs`).
- `src/thread.rs`: an agent's thread, drawn as one path that moves with what the agent is doing.
- `src/shell.rs`: the tray (Open, Start at login, Quit), Windows notifications for what the core says is worth one, starting at login with `--background`, and hiding the window to the tray when it's closed. A tick that comes long after the last means the computer slept, so the app tells the core (`app.wake`).

Not here yet: the updater (LOO-29), the instance in the app (LOO-27), and files, images and search (LOO-28).

## Trying it

Run an instance (`calx server/main.calx serve`), then start the app and sign in with its address, a username, a password and, the first time, the setup code it printed. While developing:

- `LOOM_SERVER`: an instance's address, filled in on the sign-in page.
- `LOOM_DATA`: the folder for the core's cache, instead of `%APPDATA%\Loom`.
- `LOOM_THEME`: `dark` or `light`, instead of following the system.
