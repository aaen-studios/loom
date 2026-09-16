import Link from "next/link";
import { DOWNLOAD, REPO } from "@/lib/site";
import { LoomMark } from "./loom-mark";
import { ThemeToggle } from "./theme-toggle";

const NAV = [
  { href: "/#features", label: "Features" },
  { href: "/#how", label: "How it works" },
  { href: "/#trust", label: "Privacy" },
  { href: "/#faq", label: "FAQ" },
] as const;

/**
 * The site header.
 *
 * Sticky, with the app's `pill` surface — one of the glass utilities the token
 * sync brings across — so the bar frosts the background as the page scrolls
 * under it rather than sitting on an opaque strip.
 *
 * ---------------------------------------------------------------------------
 * Why there is no hamburger button
 *
 * Below `md` the four section links do not fit beside the wordmark, the theme
 * toggle and the download button. The obvious fix is a button that toggles a
 * menu, but that needs state, which means a client component that renders
 * nothing useful in the prerendered HTML — so the site's own navigation would
 * be the one part of it that requires JavaScript.
 *
 * `<details>` does the same job natively: it opens and closes with no script, it
 * is keyboard accessible for free, and its contents are in the DOM (and so in
 * the prerender and in a page search) whether it is open or not. The `<summary>`
 * is styled as a button and the panel is absolutely positioned so opening it
 * does not shift the page.
 *
 * The panel is `md:hidden`, and the desktop links are `hidden md:flex`, so
 * exactly one of the two is ever reachable — never both, which would duplicate
 * every destination for a screen reader.
 * ---------------------------------------------------------------------------
 */
export function SiteHeader() {
  return (
    <header className="sticky top-0 z-50 px-4 pt-3 sm:px-6">
      {/* A scrim behind the bar.
          
          The nav is a floating pill with a 12px gutter above it, so page content
          scrolling past was visible *through* that gutter and read as text
          bleeding out from behind the header. The app has no equivalent problem
          because its chrome sits on a window that never scrolls.

          The gradient fades to transparent at the bottom so there is no visible
          seam, and it uses the two background `base` colours rather than the
          panel tokens — those are translucent, and a translucent scrim would
          let the text through again. */}
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-x-0 top-0 -z-10 h-[68px] bg-gradient-to-b from-[var(--site-light)] via-[color-mix(in_srgb,var(--site-light)_85%,transparent)] to-transparent dark:from-[var(--site-dark)] dark:via-[color-mix(in_srgb,var(--site-dark)_85%,transparent)]"
      />

      <nav
        aria-label="Main"
        className="pill rounded-capsule relative mx-auto flex h-14 max-w-5xl items-center gap-2 px-3 sm:px-4"
      >
        <Link
          href="/"
          className="hover-surface rounded-capsule flex shrink-0 items-center gap-2 px-2 py-1.5"
        >
          <LoomMark size={20} className="text-[var(--accent)]" />
          <span className="text-[15px] font-medium tracking-tight">Loom</span>
        </Link>

        <div className="ml-2 hidden items-center gap-0.5 md:flex">
          {NAV.map((item) => (
            <Link
              key={item.href}
              href={item.href}
              className="hover-surface text-soft rounded-capsule px-3 py-1.5 text-[13.5px]"
            >
              {item.label}
            </Link>
          ))}
          <a
            href={REPO.url}
            target="_blank"
            rel="noreferrer"
            className="hover-surface text-soft rounded-capsule px-3 py-1.5 text-[13.5px]"
          >
            GitHub
          </a>
        </div>

        {/* The small-screen menu. `details` rather than a button with state, so
            opening it needs no JavaScript and its links exist in the prerender. */}
        <details className="group ml-1 md:hidden">
          <summary
            className="hover-surface text-soft grid h-9 w-9 cursor-pointer place-items-center rounded-full [&::-webkit-details-marker]:hidden"
            aria-label="Sections"
          >
            {/* Turns into a close glyph while open, so the control always says
                what pressing it will do. */}
            <span className="group-open:hidden">
              <MenuIcon />
            </span>
            <span className="hidden group-open:block">
              <CloseIcon />
            </span>
          </summary>

          <div className="panel-strong rounded-sheet absolute inset-x-0 top-[calc(100%+6px)] z-50 overflow-hidden p-1.5">
            {NAV.map((item) => (
              <Link
                key={item.href}
                href={item.href}
                className="hover-surface text-soft rounded-row block px-3 py-2.5 text-[14px]"
              >
                {item.label}
              </Link>
            ))}
            <a
              href={REPO.url}
              target="_blank"
              rel="noreferrer"
              className="hover-surface text-soft rounded-row block px-3 py-2.5 text-[14px]"
            >
              GitHub
            </a>
          </div>
        </details>

        <div className="ml-auto flex items-center gap-2">
          <ThemeToggle />
          <Link
            href={DOWNLOAD.publicPath}
            className="btn-primary h-9 px-3.5 text-[13.5px] whitespace-nowrap"
          >
            Download
          </Link>
        </div>
      </nav>
    </header>
  );
}

function MenuIcon() {
  return (
    <svg
      width={17}
      height={17}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      aria-hidden="true"
    >
      <path d="M4 7h16M4 12h16M4 17h16" />
    </svg>
  );
}

function CloseIcon() {
  return (
    <svg
      width={17}
      height={17}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      aria-hidden="true"
    >
      <path d="M6 6l12 12M18 6L6 18" />
    </svg>
  );
}
