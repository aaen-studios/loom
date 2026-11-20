# loom's logo

The mark is **the knot**: one thread with a loop that crosses itself, the same knot an agent's thread ties when it's waiting for you. The loop also reads as a cursive l.

| File | What it is |
|---|---|
| [`knot.svg`](knot.svg) | The mark, in `currentColor`, on a 32 × 32 grid. Use this everywhere you can. |
| `png/light-<size>.png` | The mark in ink (`#111113`) on transparent, for light backgrounds. 16 to 1024 px. |
| `png/dark-<size>.png` | The mark in `#ededee` on transparent, for dark backgrounds. 16 to 1024 px. |
| `png/app-<size>.png` | The app icon: the mark in white on a dark rounded square. 16 to 1024 px. |
| [`app.ico`](app.ico) | The Windows icon: the app icon at 16, 24, 32, 48, 64, 128 and 256 px. |

## Drawing it

- **One stroke weight**, 2.6 on the 32 grid, with round caps and joins. Large uses (76 px and up) can go down to 2.2.
- **The crossing:** the falling stroke on the left passes over the rising one. `knot.svg` cuts the gap with a mask, so it works on any background. GPUI draws the rising stroke as two paths with the gap left out.
- **One colour.** The mark is never in an agent's colour in the app's chrome. Colour belongs to agents' threads.
- **The wordmark** is "loom" in Geist SemiBold, tracking −0.035 em, set beside the mark at about 1.5 times the mark's height.

## Still to make

- The macOS `.icns` (Phase 6) and the iOS icon set, with dark and tinted versions (Phase 5). Both come from `png/app-1024.png` and `png/dark-1024.png`.
- The wordmark as outlined paths, for places without Geist.
