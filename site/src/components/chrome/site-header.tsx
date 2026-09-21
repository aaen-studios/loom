"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { NAV_MOVEMENTS } from "@/lib/movements";
import { DOWNLOAD, REPO } from "@/lib/site";
import { LoomMark } from "@/components/loom-mark";

/**
 * The header.
 *
 * The app's own title-bar idiom: a floating glass pill over the top of the window. It is the one
 * piece of chrome this page takes wholesale from the product, and it earns it — a sticky header
 * *is* chrome, and the pill is how the application draws its own.
 *
 * The bar behind it is frosted rather than opaque, which is the opposite of what an earlier version
 * of this site did and is right here for a concrete reason: the ground is a near-black field with
 * generative artwork moving behind the top of it, so a translucent bar lets the threads through and
 * keeps the header from reading as a lid. The pill inside is opaque enough that the navigation stays
 * legible over whatever happens to be behind it.
 *
 * Below `md` the three destinations plus the button do not fit, so the destinations move into a
 * `<details>` — a native disclosure rather than a scripted menu, because navigation is the one part
 * of a page that must never depend on JavaScript, and the links are in the prerendered HTML whether
 * the panel is open or shut.
 */
export function SiteHeader() {
  /*
   * The header's three, chosen from the page's own list of movements rather than written out here.
   *
   * No current-page highlight, deliberately. On a site with more than one page a header should say
   * where you are; on a single-page site the scroll position is already saying it, and these three
   * point at anchors on the same document — so a highlight would light all of them at once and tell
   * the reader nothing.
   */
  const destinations = NAV_MOVEMENTS.map((movement) => ({
    href: `/#${movement.id}`,
    label: movement.label,
  }));

  return (
    <header className="header">
      <div className="shell header-row">
        <Link
          href="/"
          className="hover-surface rounded-capsule flex shrink-0 items-center gap-2 px-2 py-1.5"
        >
          <LoomMark size={19} className="text-[var(--accent)]" />
          <span className="text-[15px] font-semibold tracking-[-0.02em]">Loom</span>
        </Link>

        <nav aria-label="Main" className="ml-2 hidden items-center gap-0.5 md:flex">
          {destinations.map((destination) => (
            <Link key={destination.label} href={destination.href} className="nav-link">
              {destination.label}
            </Link>
          ))}
          <a href={REPO.url} target="_blank" rel="noreferrer" className="nav-link">
            Source
          </a>
        </nav>

        <details className="group ml-1 md:hidden">
          <summary
            className="hover-surface text-soft grid h-9 w-9 cursor-pointer place-items-center rounded-full [&::-webkit-details-marker]:hidden"
            aria-label="Destinations"
          >
            <span className="group-open:hidden">
              <MenuIcon />
            </span>
            <span className="hidden group-open:block">
              <CloseIcon />
            </span>
          </summary>

          <div className="panel-strong rounded-sheet absolute inset-x-3 top-[calc(100%+8px)] z-50 overflow-hidden p-1.5">
            {destinations.map((destination) => (
              <Link
                key={destination.label}
                href={destination.href}
                className="hover-surface text-soft rounded-row block px-3 py-2.5 text-sm"
              >
                {destination.label}
              </Link>
            ))}
            <a
              href={REPO.url}
              target="_blank"
              rel="noreferrer"
              className="hover-surface text-soft rounded-row block px-3 py-2.5 text-sm"
            >
              Source
            </a>
          </div>
        </details>

        <div className="ml-auto flex items-center gap-2">
          <ThemeToggle />
          <Link
            href={DOWNLOAD.publicPath}
            className="btn-primary h-9 px-3.5 text-[13px] whitespace-nowrap"
          >
            Download
          </Link>
        </div>
      </div>
    </header>
  );
}

/**
 * The light/dark switch, using the app's contract exactly: a `.dark` class on `<html>`, persisted
 * under `loom-theme`.
 *
 * The state is read from the DOM rather than from `localStorage`, because the boot script in `<head>`
 * has already applied the stored preference by the time this mounts. Reading storage a second time
 * would create a second source of truth, and the two can disagree — storage can throw, or be
 * disabled, and the class is what is actually painted.
 *
 * `root.style.background` is written on toggle even though `globals.css` already derives the ground
 * from the class. That is the same duplication the boot script makes and for the same reason: this is
 * the one property where being a frame late is visible, because the compositor paints `<html>` before
 * the stylesheet has resolved a custom property.
 */
function ThemeToggle() {
  const [dark, setDark] = useState(true);
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    setDark(document.documentElement.classList.contains("dark"));
    setMounted(true);
  }, []);

  const toggle = () => {
    const next = !dark;
    setDark(next);

    const root = document.documentElement;
    root.classList.toggle("dark", next);
    root.style.background = next ? "#070a12" : "#eef1f7";

    try {
      localStorage.setItem("loom-theme", next ? "dark" : "light");
    } catch {
      /* Storage unavailable: the choice simply does not persist. */
    }
  };

  return (
    <button
      type="button"
      onClick={toggle}
      className="btn-ghost hover-surface h-9 w-9 shrink-0 p-0"
      // The label names the control, not the action. A button whose accessible name changes as you
      // press it is announced as two different controls; `aria-pressed` carries the state.
      aria-label="Colour theme"
      aria-pressed={dark}
      title={mounted ? (dark ? "Switch to light" : "Switch to dark") : "Colour theme"}
    >
      {/* An invisible placeholder before mount, so the header does not shift sideways when the real
          icon appears. The server cannot know the visitor's preference, so rendering a confident icon
          into the HTML would be a hydration mismatch. */}
      {!mounted ? (
        <span className="block h-4 w-4" />
      ) : dark ? (
        <SunIcon />
      ) : (
        <MoonIcon />
      )}
    </button>
  );
}

/* Both drawn at 16px on a 24-unit grid, matching the app's icon set. */

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

function SunIcon() {
  return (
    <svg
      width={16}
      height={16}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      aria-hidden="true"
    >
      <circle cx="12" cy="12" r="4.2" />
      <path d="M12 2.6v2.2M12 19.2v2.2M2.6 12h2.2M19.2 12h2.2M5.4 5.4l1.6 1.6M17 17l1.6 1.6M18.6 5.4L17 7M7 17l-1.6 1.6" />
    </svg>
  );
}

function MoonIcon() {
  return (
    <svg
      width={16}
      height={16}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M20.2 14.6A8.6 8.6 0 0 1 9.4 3.8a8.6 8.6 0 1 0 10.8 10.8z" />
    </svg>
  );
}
