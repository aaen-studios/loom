"use client";

import { useEffect, useState } from "react";

const STORAGE_KEY = "loom-theme";

/**
 * Light/dark switch, using the same contract as the app: a `.dark` class on
 * `<html>`, persisted under `loom-theme`.
 *
 * The initial state is read from the DOM rather than from `localStorage`,
 * because `THEME_BOOT` has already applied the stored preference by the time
 * this mounts. Reading storage again here would be a second source of truth,
 * and the two can disagree if the storage write failed.
 *
 * `useState(false)` then correcting in an effect is deliberate: the server has
 * no idea what the visitor prefers, so rendering the *dark* icon during
 * hydration would be a mismatch. The icon swaps immediately after mount, which
 * is invisible because the boot script has already painted the right palette.
 */
export function ThemeToggle() {
  const [dark, setDark] = useState(false);
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
    // Matches THEME_BOOT, so a reload and a toggle cannot leave the page on a
    // different backdrop than the stored preference implies.
    root.style.background = next ? "#070a12" : "#eef1f7";

    try {
      localStorage.setItem(STORAGE_KEY, next ? "dark" : "light");
    } catch {
      /* Storage unavailable: the choice simply does not persist. */
    }
  };

  return (
    <button
      type="button"
      onClick={toggle}
      className="btn-ghost hover-surface h-9 w-9 shrink-0 p-0"
      // The label is stable across both states: it names the control, not the
      // action, so a screen reader never announces a control that changes name
      // under the user.
      aria-label="Colour theme"
      aria-pressed={dark}
      title={mounted ? (dark ? "Switch to light" : "Switch to dark") : "Colour theme"}
    >
      {/* An invisible placeholder before mount keeps the header from shifting. */}
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

/** Drawn at 16px on a 24-unit grid, matching the app's icon set. */
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
