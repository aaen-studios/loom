/* ---------------------------------------------------------------------------
   Inter, self-hosted.

   The same font binary the desktop app ships (`@fontsource-variable/inter`),
   copied here rather than fetched from Google at build time. Two reasons:

   1. It is byte-identical to the app's type, so the site cannot render a
      slightly different Inter than the product it is advertising.
   2. The build does not depend on a third-party CDN being reachable, which
      matters because CI builds this on every push.

   Inter is licensed under the SIL Open Font License 1.1; the license text
   travels with it in `INTER-LICENSE.txt`.

   `wght` is the weight axis only. The app uses the same. Adding `opsz` would
   change the metrics slightly at large sizes, so the two surfaces would stop
   matching.
--------------------------------------------------------------------------- */
import localFont from "next/font/local";

export const inter = localFont({
  src: [{ path: "../fonts/inter-latin-wght-normal.woff2", weight: "100 900" }],
  variable: "--font-inter",
  display: "swap",
  // The app's own stack, so a fallback render is close to the real thing.
  fallback: [
    "ui-sans-serif",
    "system-ui",
    "-apple-system",
    "Segoe UI",
    "sans-serif",
  ],
});
