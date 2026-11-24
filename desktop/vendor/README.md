# Vendored crates

**gpui-pre-windows** is GPUI's Windows platform at 0.3.12, from crates.io, under its Apache-2.0 licence. Two changes are made, marked `loom:`.

In `src/platform.rs`, the vsync thread only tells windows that are on screen to paint. Before the change it told every window to paint at every vblank, so the app's main thread woke 240 times a second while minimised, which cost 68–90 ms of CPU a second. Once every window has been off screen for a second, as when loom is in the tray, the thread looks five times a second rather than at every vblank. It waits that second because a window that's opening isn't on screen until its first frame, and the frames after that wait on this thread: without the wait, the chat showed up to 200 ms later at start.

A window on screen is told to paint only when it has something new to draw (`FrameDemand` in `src/window.rs`): GPUI says so through the window's `frame_waker` and `schedule_frame`, as it does for macOS's display link. A window shown and idle used to be woken at every vblank to find it had nothing to draw, 28–42 ms of CPU a second. While a touchpad gesture runs it's still told at every vblank, since Direct Manipulation is pumped from each paint (`src/direct_manipulation.rs`), and every window is told four times a second regardless, in case a frame GPUI put off isn't asked for again.

To move to a newer GPUI, copy that version's crate here and make the same changes again. If upstream has made them already, drop the patch from `../Cargo.toml` instead.

**gpui-base** is GPUI Base at 0.10.0, from crates.io, under its Apache-2.0 licence. One change is made, marked `loom:`: an inline `<span style="color: #rrggbb; font-weight: 500">` in Markdown gives its text that color and weight (`TextMark`'s `color` and `weight` in `src/text/node.rs`, read in `src/text/format/markdown.rs`). The chat draws an agent's name that way where a message mentions it, in the agent's colour. To move to a newer GPUI Base, copy that version's crate here and make the same change again, or drop the patch if upstream has a way to colour a span.
