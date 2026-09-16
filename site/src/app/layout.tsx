import type { Metadata, Viewport } from "next";
import { inter } from "@/lib/fonts";
import { SITE } from "@/lib/site";
import { Background } from "@/components/chrome/background";
import { Warp } from "@/components/chrome/warp";
import { Shuttle } from "@/components/chrome/shuttle";
import { DraftStrip } from "@/components/chrome/draft-strip";
import { Selvedge } from "@/components/chrome/selvedge";
import { LayoutProbe } from "@/components/dev/probe";
import "./globals.css";

export const metadata: Metadata = {
  // Without this, every relative URL in metadata — canonicals, the OG image —
  // would have to be made absolute by hand, and one of them eventually would not
  // be.
  metadataBase: new URL(SITE.url),
  title: {
    default: "Loom — a desktop app for AI chat and agents",
    // So a page only has to name itself. `Download · Loom`, not
    // `Download · Loom — a desktop app…`.
    template: "%s · Loom",
  },
  description: SITE.description,
  applicationName: "Loom",
  keywords: [
    "AI agent",
    "desktop AI",
    "local AI client",
    "MCP",
    "agentic coding",
    "Windows AI app",
  ],
  authors: [{ name: SITE.publisher, url: SITE.publisherUrl }],
  openGraph: {
    type: "website",
    url: SITE.url,
    siteName: "Loom",
    title: "Loom — a desktop app for AI chat and agents",
    description: SITE.description,
  },
  twitter: {
    card: "summary_large_image",
    title: "Loom — a desktop app for AI chat and agents",
    description: SITE.description,
  },
  robots: { index: true, follow: true },
};

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  // The two colours the app paints itself with before React mounts, so a mobile
  // browser's own chrome does not clash with the page it is framing.
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#eef1f7" },
    { media: "(prefers-color-scheme: dark)", color: "#070a12" },
  ],
};

/**
 * Theme boot.
 *
 * Runs synchronously in `<head>`, before anything is painted, so the first frame is
 * already the right palette. A React effect instead would show a white flash to
 * every dark visitor on every navigation — and on a page strung with hairlines and
 * frosted panels, that flash is the first thing anyone would notice.
 *
 * The contract is the app's exactly: a `.dark` class on `<html>`, and `<html>`
 * painted one of the two colours the app uses for its own pre-mount backdrop. Light
 * is the default in both, so the shared `html:not(.dark)` token block applies to
 * this page unchanged and nothing has to override anything.
 *
 * Wrapped in `try`/`catch` because a browser with storage disabled throws on
 * `localStorage` access, and failing to read a theme preference must not take the
 * page down with it.
 */
const THEME_BOOT = `
(function () {
  try {
    var stored = localStorage.getItem("loom-theme");
    var dark = stored === "dark";
    var root = document.documentElement;
    root.classList.toggle("dark", dark);
    root.style.background = dark ? "#070a12" : "#eef1f7";
  } catch (error) {
    /* No storage available: stay on the light default. */
  }
})();
`;

/**
 * The frame every page is woven into.
 *
 * Five fixed layers, in stacking order, and the order is the entire architecture:
 *
 *   1. `Background` at `-z-10` — the app's Porcelain preset, painted in CSS.
 *   2. `Warp` at `z-0`         — twelve threads, strung full height.
 *   3. `Shuttle` at `z-5`      — the weft, at the reading position.
 *   4. the content at `z-10`   — every page, in a `.warp-grid`.
 *   5. `Selvedge`             — the footer, in flow.
 *
 * That the content sits *above* the weft is deliberate and is why the threads read
 * as being behind the page rather than laid over it: the weft crosses the whole
 * viewport, and text is painted on top of it, so the line appears in the whitespace
 * and the gutters and passes behind the words rather than through them.
 *
 * `suppressHydrationWarning` on `<html>` is required rather than a shrug: the boot
 * script adds a class to that element before React hydrates, so the server markup
 * and the client DOM legitimately differ there and nowhere else.
 */
export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" className={inter.variable} suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: THEME_BOOT }} />
      </head>
      <body>
        <Background />
        <Warp />
        <Shuttle />
        <div className="relative z-10">
          <DraftStrip />
          <main id="top">{children}</main>
          <Selvedge />
        </div>
        {/* Development only, and tree-shaken out of the production build entirely
            rather than merely inert inside it. Measures the rendered page when the
            URL carries `?probe`, so `probe-layout.mjs` can read geometry as text. */}
        {process.env.NODE_ENV === "development" && <LayoutProbe />}
      </body>
    </html>
  );
}
