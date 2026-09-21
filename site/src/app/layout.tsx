import type { Metadata, Viewport } from "next";
import { inter } from "@/lib/fonts";
import { DARK, LIGHT } from "@/lib/background";
import { SITE } from "@/lib/site";
import { RunningHead } from "@/components/doc/running-head";
import { LayoutProbe } from "@/components/dev/probe";
import "./globals.css";

export const metadata: Metadata = {
  // Without this, every relative URL in metadata — canonicals, the OG image —
  // would have to be made absolute by hand, and one of them eventually would not
  // be.
  metadataBase: new URL(SITE.url),
  title: {
    // The document's title, as a manual's is: the instrument, then what it is.
    default: "Loom — a desktop workspace for AI chat and agents",
    // So a page only has to name itself.
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
    "AI terminal",
    "AI code editor",
    "AI workspace",
  ],
  authors: [{ name: SITE.publisher, url: SITE.publisherUrl }],
  openGraph: {
    type: "website",
    url: SITE.url,
    siteName: "Loom",
    title: "Loom — a desktop workspace for AI chat and agents",
    description: SITE.description,
  },
  twitter: {
    card: "summary_large_image",
    title: "Loom — a desktop workspace for AI chat and agents",
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
    { media: "(prefers-color-scheme: light)", color: LIGHT },
    { media: "(prefers-color-scheme: dark)", color: DARK },
  ],
};

/**
 * Theme boot.
 *
 * Runs synchronously in `<head>`, before anything is painted, so the first frame
 * is already the right palette. A React effect instead would show a white flash to
 * every dark visitor on every navigation — and on a document whose ground is one
 * flat colour, that flash is the first thing anyone would see.
 *
 * The contract is the app's exactly: a `.dark` class on `<html>`, and `<html>`
 * painted one of the two colours the app uses for its own pre-mount backdrop.
 *
 * `root.style.background` is set here *as well as* in the stylesheet, and that
 * duplication is deliberate: the stylesheet may still be loading when this runs,
 * and this is the one moment where "already the right colour" matters. It is also
 * why `background.test.ts` names it as one of the three places the value lives.
 *
 * Wrapped in `try`/`catch` because a browser with storage disabled throws on
 * `localStorage` access, and failing to read a theme preference must not take the
 * page down with it.
 */
const THEME_BOOT = `
(function () {
  try {
    var stored = localStorage.getItem("loom-theme");
    var dark = stored !== "light";
    var root = document.documentElement;
    root.classList.toggle("dark", dark);
    root.style.background = dark ? "${DARK}" : "${LIGHT}";
  } catch (error) {
    /* No storage available: the stylesheet's default ground stands. */
  }
})();
`;

/**
 * The frame every page shares.
 *
 * Almost nothing is here, and that is the change. The previous layout stacked five
 * fixed layers: a drifting backdrop, twelve full-height warp hairlines, a weft
 * line that followed the scroll, then the content, then the footer. Three of those
 * were the metaphor applied as texture rather than as information.
 *
 * What is left:
 *
 *   - the ground, which is now a `background-color` on `<body>` and needs no
 *     element at all;
 *   - the running head, which is a real printed device — it names the document
 *     while you are inside it, and on a narrow viewport it names the section,
 *     because that is where the margin index is not;
 *   - the content, with nothing fixed over it and nothing behind it.
 *
 * `suppressHydrationWarning` on `<html>` is required rather than a shrug: the boot
 * script adds a class to that element before React hydrates, so the server markup
 * and the client DOM legitimately differ there and nowhere else.
 */
export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" className={`${inter.variable} dark`} suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: THEME_BOOT }} />
      </head>
      <body>
        <RunningHead />
        <main id="top">{children}</main>
        {/* Development only, and tree-shaken out of the production build entirely
            rather than merely inert inside it. Measures the rendered document when
            the URL carries `?probe`, so `probe-layout.mjs` can read geometry as
            text. */}
        {process.env.NODE_ENV === "development" && <LayoutProbe />}
      </body>
    </html>
  );
}
