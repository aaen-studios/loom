# loom's logo

The mark is **the knot**: one thread with a loop that crosses itself, the same knot an agent's thread ties when it's waiting for you. The loop also reads as a cursive l.

| File | What it is |
|---|---|
| [`knot.svg`](knot.svg) | The mark, in `currentColor`, on a 32 × 32 grid. Use this everywhere you can. |
| `png/light-<size>.png` | The mark in ink (`#111113`) on transparent, for light backgrounds. 16 to 1024 px. |
| `png/dark-<size>.png` | The mark in `#ededee` on transparent, for dark backgrounds. 16 to 1024 px. |
| `png/app-<size>.png` | The app icon: the mark in white on a dark rounded square. 16 to 1024 px. |
| [`app.ico`](app.ico) | The Windows icon: the app icon at 16, 24, 32, 48, 64, 128 and 256 px. |
| [`wordmark.svg`](wordmark.svg) | The wordmark, "loom" in Geist SemiBold with its tracking, as outlined paths in `currentColor`, for places without Geist. `calx scripts/wordmark.calx` makes it. |
| [`app.icns`](app.icns) | The Mac icon: the app icon inset on Apple's grid (824 of 1024 px, with room for the shadow macOS adds), 16 to 1024 px. |
| [`ios/AppIcon.appiconset`](ios/AppIcon.appiconset) | The iPhone icon at 1024 px, for Xcode: the app icon as an opaque square (iOS rounds it), the mark alone in light for dark mode, and in gray for tinted. |

`calx scripts/icons.calx` makes the last two from `png/app-1024.png` and `png/dark-1024.png`.

## Drawing it

- **One stroke weight**, 2.6 on the 32 grid, with round caps and joins. Large uses (76 px and up) can go down to 2.2.
- **The crossing:** the falling stroke on the left passes over the rising one. `knot.svg` cuts the gap with a mask, so it works on any background. GPUI draws the rising stroke as two paths with the gap left out.
- **One colour.** The mark is never in an agent's colour in the app's chrome. Colour belongs to agents' threads.
- **The wordmark** is "loom" in Geist SemiBold, tracking −0.035 em, set beside the mark at about 1.5 times the mark's height.
