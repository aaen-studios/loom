"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { DESTINATIONS } from "@/lib/weave/passes";
import { DOWNLOAD, REPO } from "@/lib/site";
import { LoomMark } from "@/components/loom-mark";
import { ThemeToggle } from "./theme-toggle";
import { cn } from "@/lib/cn";

/**
 * The header, as a weaving draft.
 *
 * A draft is read at a glance: a row of cells, some lifted and some not, showing
 * the whole pattern. A navigation wants the same thing, so the two are the same
 * object — each destination is a cell, and the one you are standing on is the
 * lifted thread, marked with the filled square a real draft would use.
 *
 * The `data-current` attribute is the entire mechanism. Its marker is drawn in
 * CSS, so there is no second copy of "which page am I on" in the markup.
 *
 * ---------------------------------------------------------------------------
 * Why the small-screen menu is a `<details>` and not a button
 * ---------------------------------------------------------------------------
 *
 * Below `md` four destinations plus the mark, the theme toggle and a download
 * button do not fit. The reflex is a button that toggles a panel, which needs
 * state, which means a client component rendering nothing usable into the
 * prerender — so navigation would be the one part of this site that cannot be
 * read without JavaScript.
 *
 * `<details>` does the same job with no script, is keyboard-operable for free, and
 * keeps its links in the DOM whether open or closed, so they are in the prerender,
 * in a page search, and in a crawler's view. The inline cells and the panel are
 * `md`-gated in opposite directions, so exactly one set is ever reachable.
 */
export function DraftStrip() {
  const pathname = usePathname();

  return (
    <header className="sticky top-0 z-50 px-4 pt-3 sm:px-6">
      {/* A scrim. The bar floats with a gutter above it, and content scrolling
          through that gutter reads as text bleeding out from behind the header.
          Painted with the background's flat base colours rather than the panel
          tokens — those are translucent, and a translucent scrim lets the text
          straight back through. */}
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

        <div className="draft-strip ml-2 hidden md:flex">
          {DESTINATIONS.map((destination) => (
            <Link
              key={destination.href}
              href={destination.href}
              className="draft-cell"
              data-current={destination.match.test(pathname)}
            >
              {destination.label}
            </Link>
          ))}
          <a
            href={REPO.url}
            target="_blank"
            rel="noreferrer"
            className="draft-cell"
          >
            Source
          </a>
        </div>

        <details className="group ml-1 md:hidden">
          <summary
            className="hover-surface text-soft grid h-9 w-9 cursor-pointer place-items-center rounded-full [&::-webkit-details-marker]:hidden"
            aria-label="Destinations"
          >
            {/* Swaps to a close glyph while open, so the control always says what
                pressing it will do. */}
            <span className="group-open:hidden">
              <MenuIcon />
            </span>
            <span className="hidden group-open:block">
              <CloseIcon />
            </span>
          </summary>

          <div className="panel-strong rounded-sheet absolute inset-x-0 top-[calc(100%+6px)] z-50 overflow-hidden p-1.5">
            {DESTINATIONS.map((destination) => (
              <Link
                key={destination.href}
                href={destination.href}
                className="hover-surface text-soft rounded-row flex items-center gap-2.5 px-3 py-2.5 text-[14px]"
                data-current={destination.match.test(pathname)}
              >
                <span
                  className={cn(
                    "knot",
                    destination.match.test(pathname) && "knot-lit",
                  )}
                />
                {destination.label}
              </Link>
            ))}
            <a
              href={REPO.url}
              target="_blank"
              rel="noreferrer"
              className="hover-surface text-soft rounded-row flex items-center gap-2.5 px-3 py-2.5 text-[14px]"
            >
              <span className="knot" />
              Source
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
