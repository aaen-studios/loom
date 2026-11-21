# desktop

Rust: the GPUI app for Windows and macOS. It draws screens and reaches the system (tray, notifications, starting at login, screenshots, the clipboard, the updater), and leaves everything else to the [core](../core/), which it links.

GPUI comes from GPUI Kit (`gpui-kit`), pinned exactly in `Cargo.toml`. See [PLAN.md: Desktop on GPUI](../PLAN.md#desktop-on-gpui-what-to-know).

## Building

You need a recent stable Rust (`rustup update stable`) and `calx` on PATH. `build.rs` builds the core into a static library with `calx build --staticlib` and links it, so `cargo` is the only command:

```sh
cargo run --release
```

Set `CALX` to use a different `calx` binary.

## What's here now

The Phase 2 spike (LOO-4): a window with 3000 Markdown messages in a virtual list, a reply streamed in by the spike core (`core/spike.calx`) through `on_event`, a composer that sends to the core, file drop, a tray icon and a notification. What it showed is in the plan. The app proper replaces it, starting with LOO-25.
