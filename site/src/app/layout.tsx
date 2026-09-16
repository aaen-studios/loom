import type { Metadata, Viewport } from "next";
import { inter } from "@/lib/fonts";
import { SITE } from "@/lib/site";
import "./globals.css";

export const metadata: Metadata = {
  metadataBase: new URL(SITE.url),
  title: {
    default: "Loom — a desktop app for AI chat and agents",
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
  // Two colours, matching the two `background` values the app paints before
  // React mounts, so a mobile browser's chrome does not clash with the page.
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#eef1f7" },
    { media: "(prefers-color-scheme: dark)", color: "#070a12" },
  ],
};

/**
 * Theme boot.
 *
 * Runs synchronously in <head>, before anything paints, so the first frame is
 * already the right palette. Doing this in a React effect instead would show a
 * light flash to every dark-mode visitor on every navigation.
 *
 * The contract is exactly the app's (see `App.tsx`): `.dark` on <html>, and
 * <html> painted with the same two colours the app uses for its pre-mount
 * backdrop. Light is the default, matching the app.
 *
 * Wrapped in try/catch because a browser with storage disabled throws on
 * `localStorage` access, and a failed theme must not take the page down.
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
    /* No storage: stay on the light default. */
  }
})();
`;

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" className={inter.variable} suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: THEME_BOOT }} />
      </head>
      <body>{children}</body>
    </html>
  );
}
