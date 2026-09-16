/*
 * Inter, self-hosted.
 *
 * The exact binary the desktop app ships, copied out of
 * `@fontsource-variable/inter` rather than fetched from a font CDN at build
 * time. Two things follow from that, and both matter:
 *
 *  1. The site cannot render a subtly different Inter than the product it is
 *     advertising. Same file, same weight axis, same metrics.
 *  2. A build does not depend on a third party being reachable, which matters
 *     because CI builds this on every push to `main`.
 *
 * `wght` only. The `opsz` axis exists in the same package and is deliberately
 * not used: it would change the metrics at display sizes, so the hero's type
 * would stop matching the app's.
 *
 * Loaded through `next/font/local` for the preload and the inlined metric
 * overrides; see the note in `globals.css` for why the family stack is
 * assembled there instead of here.
 *
 * Inter is under the SIL Open Font License 1.1, which travels with the binary
 * in `INTER-LICENSE.txt`.
 */
import localFont from "next/font/local";

export const inter = localFont({
  src: [{ path: "../fonts/inter-latin-wght-normal.woff2", weight: "100 900" }],
  variable: "--font-inter",
  display: "swap",
  // The app's own fallback stack, so a swap is as close to the real thing as a
  // system font can be.
  fallback: [
    "ui-sans-serif",
    "system-ui",
    "-apple-system",
    "Segoe UI",
    "sans-serif",
  ],
});
