"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { LOOM_TERMS, SECTIONS } from "@/lib/document";
import { LoomMark } from "@/components/loom-mark";
import { useCurrentSection } from "./use-current-section";

/**
 * The running head.
 *
 * A manual has one: it names the document while you are inside it. This one also
 * carries the current section, which is the whole reason it is a client component
 * — at `lg` and up the margin index does that job, and below it the index is not
 * on screen, so the running head takes over. Exactly one of the two is ever
 * visible, which is what keeps "where am I" from being answered twice.
 *
 * Opaque, not frosted. A blurred bar over a page of text is a window treatment; a
 * printed running head is simply the top of the sheet, and content must not show
 * through it. That is also why there is no `backdrop-filter` anywhere in this
 * project — see the note at the top of `globals.css`.
 *
 * ---------------------------------------------------------------------------
 * Two links, and why not more
 * ---------------------------------------------------------------------------
 *
 * `Contents` and `Download`. Everything else the document contains is in the
 * contents list, which is three quarters of a screen away at the top of the page —
 * and a header that tries to be the contents is how a long document acquires
 * twenty navigation links and no navigation.
 *
 * The section number on a narrow viewport is `§3 One turn of the agent` rather than
 * `3 — One turn of the agent`, because the section mark is what a reader who has
 * seen the contents list will recognise, and it costs one character.
 */
export function RunningHead() {
  const current = useCurrentSection();

  return (
    <header className="bar">
      <nav className="bar-inner" aria-label="Document">
        <Link href="#top" className="bar-brand">
          {/* The shared mark, not a fourth hand-drawn copy of it. The path data is
              the application's, and `src/lib/iconConsistency.test.ts` in the app
              asserts this file's copy keeps all three of them — which is only worth
              anything if there is one copy to assert about. */}
          <LoomMark size={15} className="text-[var(--accent)]" />
          Loom
        </Link>

        <span className="bar-descriptor">
          {SECTIONS.length} sections, {LOOM_TERMS.length} terms, one application
        </span>

        {/* `aria-hidden` because it is a position indicator for the eye: the
            margin index and the headings themselves are what a screen reader
            navigates by, and a live region announcing every section change would
            interrupt the reading it is describing. */}
        <span className="bar-current" aria-hidden="true">
          {current ? `§${current.number} ${current.title}` : ""}
        </span>

        <div className="bar-links">
          <Link href="#contents" className="bar-link">
            Contents
          </Link>
          <Link href="#off" className="bar-link">
            Download
          </Link>
          <ThemeToggle />
        </div>
      </nav>
    </header>
  );
}

/**
 * The light/dark switch, using the app's contract exactly: a `.dark` class on
 * `<html>`, persisted under `loom-theme`.
 *
 * Three details are load-bearing.
 *
 * The state is read from the DOM rather than from `localStorage`, because the boot
 * script in `<head>` has already applied the stored preference by the time this
 * mounts. Reading storage a second time would create a second source of truth, and
 * the two can disagree — storage can throw, or be disabled, and the class is what
 * is actually painted.
 *
 * `root.style.background` is written on toggle even though the stylesheet already
 * derives the ground from the class. That is the same duplication the boot script
 * makes and for the same reason: it is the one property where being a frame late
 * is visible.
 *
 * The accessible name is fixed (`aria-label`, below) and does not change with
 * state, while the visible text does. A button whose *name* changes as you press
 * it is announced as two different controls, which is why `aria-pressed` carries
 * the state and the label carries the purpose.
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
      className="bar-link"
      aria-label="Colour theme"
      aria-pressed={dark}
      title={mounted ? (dark ? "Switch to light" : "Switch to dark") : "Colour theme"}
    >
      {/* Before mount the label is the neutral one, because the server cannot know
          what the visitor prefers and rendering a confident "Light" into the HTML
          would be a hydration mismatch. The swap happens one frame later and is
          invisible: the boot script has already painted the right ground. */}
      <span aria-hidden="true">{mounted ? (dark ? "Light" : "Dark") : "Theme"}</span>
    </button>
  );
}
