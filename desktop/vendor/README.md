# Vendored crates

**gpui-pre-windows** is GPUI's Windows platform at 0.3.12, from crates.io, under its Apache-2.0 licence. One change is made, in `src/platform.rs` and marked `loom:`: the vsync thread only tells windows that are on screen to paint. Before the change it told every window to paint at every vblank, so the app's main thread woke 240 times a second while minimised, which cost 68–90 ms of CPU a second. With every window hidden, as when loom is in the tray, the thread now looks five times a second rather than at every vblank.

To move to a newer GPUI, copy that version's crate here and make the same change again. If upstream has made it already, drop the patch from `../Cargo.toml` instead.
