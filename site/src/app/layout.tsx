import type { Metadata, Viewport } from "next";
import { inter } from "@/lib/fonts";
import { DARK, LIGHT } from "@/lib/background";
import { SITE } from "@/lib/site";
import { SiteHeader } from "@/components/chrome/site-header";
import { SiteFooter } from "@/components/chrome/site-footer";
import { LayoutProbe } from "@/components/dev/probe";
import "./globals.css";

export const metadata: Metadata = {
  // Without this, every relative URL in metadata — canonicals, the OG image — would have to be made
  // absolute by hand, and one of them eventually would not be.
  metadataBase: new URL(SITE.url),
  title: {
    default: "Loom — a window that holds everything",
    // So a page only has to name itself: `Download · Loom`, not `Download · Loom — a window…`.
    template: "%s · Loom",
  },
  description: SITE.description,
  applicationName: "Loom",
  keywords: [
    "AI agent",
    "desktop AI",
    "AI workspace",
    "AI terminal",
    "AI code editor",
    "docking workspace",
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
    title: "Loom — a window that holds everything",
    description: SITE.description,
  },
  twitter: {
    card: "summary_large_image",
    title: "Loom — a window that holds everything",
    description: SITE.description,
  },
  robots: { index: true, follow: true },
};

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  // The two colours the app paints itself with before React mounts, so a mobile browser's own chrome
  // does not clash with the page it is framing.
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: LIGHT },
    { media: "(prefers-color-scheme: dark)", color: DARK },
  ],
};

/**
 * Theme boot.
 *
 * Runs synchronously in `<head>`, before anything is painted, so the first frame is already the right
 * palette. A React effect instead would show a white flash to every dark visitor on every navigation
 * — and on a page whose ground is a near-black field with artwork moving over it, that flash is the
 * first thing anyone would see.
 *
 * The contract is the app's exactly: a `.dark` class on `<html>`, and `<html>` painted one of the two
 * colours the app uses for its own pre-mount backdrop. Dark is the default in both, which is why the
 * test is `stored !== "light"` rather than `stored === "dark"`.
 *
 * `root.style.background` is set here *as well as* in the stylesheet, and the duplication is
 * deliberate: the stylesheet may still be loading when this runs, and this is the one moment where
 * "already the right colour" matters.
 *
 * Wrapped in `try`/`catch` because a browser with storage disabled throws on `localStorage` access,
 * and failing to read a theme preference must not take the page down with it.
 */
const THEME_BOOT = `
(function () {
  try {
    var stored = localStorage.getItem("loom-theme");
    var dark = stored === "dark";
    var root = document.documentElement;
    root.classList.toggle("dark", dark);
    root.style.background = dark ? "${DARK}" : "${LIGHT}";
  } catch (error) {
    /* No storage available: the stylesheet's default ground stands. */
  }
})();
`;

/**
 * The frame.
 *
 * Three elements, and the first one is not an element at all: the ground is a `background-color` on
 * `<body>`, painted in the app's own pre-mount colour. There is no backdrop layer, no texture and no
 * scroll-following line, because the artwork on this page is drawn *inside* each movement rather than
 * behind the whole document — which is what lets a figure be clipped to a movement's own frame instead
 * of bleeding under everything.
 *
 * The one fixed layer is a light source: a single low-opacity radial in the accent's own family, pinned
 * to the top of the viewport. It does almost nothing in light mode, where the ground is already the
 * brightest thing on the page — but it costs one element, and in dark mode it is the difference between
 * a surface and a rectangle.
 *
 * `suppressHydrationWarning` on `<html>` is required rather than a shrug: the boot script adds a class
 * to that element before React hydrates, so the server markup and the client DOM legitimately differ
 * there and nowhere else.
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
        {/* The light source. `aria-hidden` because it is scenery, and fixed because it is the light
            the page is lit by rather than an object on it. */}
        <div className="glow" aria-hidden="true" />

        <SiteHeader />
        <main id="top">{children}</main>
        <SiteFooter />

        {/* Development only, and tree-shaken out of the production build entirely rather than merely
            inert inside it. Measures the rendered page when the URL carries `?probe`, so
            `probe-layout.mjs` can read geometry as text. */}
        {process.env.NODE_ENV === "development" && <LayoutProbe />}
      </body>
    </html>
  );
}
