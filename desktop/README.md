# desktop

Rust: the GPUI app for Windows and macOS. It draws screens and reaches the system (tray, notifications, starting at login, screenshots, the clipboard, the updater), and leaves everything else to the [core](../core/), which it links.

Phase 2 opens with a GPUI spike on Windows (LOO-4), then the throwaway test of the Calx library inside a GPUI window, then the app. See [PLAN.md: Desktop on GPUI](../PLAN.md#desktop-on-gpui-what-to-know).
