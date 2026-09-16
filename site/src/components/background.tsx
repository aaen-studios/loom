"use client";

import { useEffect, useState } from "react";
import { DARK_DIM_FLOOR, PORCELAIN } from "@/lib/background";

/**
 * A whisper of film grain, as an inline SVG turbulence filter.
 *
 * Large fields of a very subtle gradient band visibly on some panels; breaking
 * the surface with noise hides the steps. The app uses the same technique at
 * the same 4% opacity, so the two surfaces have the same tooth.
 */
const GRAIN =
  "url(\"data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='180' height='180'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='2' stitchTiles='stitch'/></filter><rect width='100%25' height='100%25' filter='url(%23n)' opacity='0.55'/></svg>\")";

/**
 * Tracks the `.dark` class on `<html>`, which `THEME_BOOT` and the toggle keep
 * in sync. An observer rather than context on purpose: the class is set by a
 * blocking script before React exists, so the DOM is the source of truth and
 * reading it here cannot disagree with what is painted.
 */
function useThemeIsDark(): boolean {
  const [dark, setDark] = useState(false);

  useEffect(() => {
    const root = document.documentElement;
    const update = () => setDark(root.classList.contains("dark"));
    update();

    const observer = new MutationObserver(update);
    observer.observe(root, { attributes: true, attributeFilter: ["class"] });
    return () => observer.disconnect();
  }, []);

  return dark;
}

/**
 * The full-bleed background layer.
 *
 * `fixed` rather than `absolute`: the landing page scrolls, and the app's
 * background does not move with its content. Pinning it also means the glass
 * surfaces above it always have the same thing to frost as you scroll, which is
 * what makes them read as panes over a desktop rather than as tinted cards.
 *
 * Server-rendered with the light values (dark is the exception, not the
 * default), then corrected in an effect if the visitor prefers dark.
 */
export function Background() {
  const dark = useThemeIsDark();
  const dim = dark ? DARK_DIM_FLOOR : 0;

  return (
    <div
      aria-hidden="true"
      className="pointer-events-none fixed inset-0 -z-10 overflow-hidden"
      style={{ backgroundColor: PORCELAIN.base }}
    >
      {/* The layers are painted larger than the viewport and drift slowly, so
          the washes never read as a static image behind the glass. The overscan
          keeps the edges covered while the scale and translate move. */}
      <div
        className="animate-drift absolute inset-[-6%]"
        style={{ background: PORCELAIN.layers }}
      />

      {/* Dim veil: dark mode only, so bright art cannot wash out near-white ink.
          Skipped entirely at zero rather than layered as a no-op. */}
      {dim > 0 && (
        <div
          className="absolute inset-0"
          style={{ backgroundColor: `rgb(3 6 14 / ${dim / 100})` }}
        />
      )}

      <div
        className="absolute inset-0 opacity-[0.04] mix-blend-overlay"
        style={{ backgroundImage: GRAIN, backgroundSize: "180px 180px" }}
      />
    </div>
  );
}
