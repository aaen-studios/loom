"use client";

import { useEffect, useState } from "react";
import { DARK_DIM_FLOOR, GRAIN, PORCELAIN } from "@/lib/background";

/**
 * Watches the `.dark` class on `<html>`.
 *
 * An observer rather than context, because the class is set by a blocking script
 * in `<head>` before React exists — so the DOM is the source of truth here, and
 * reading it cannot disagree with what is already painted.
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
 * The backdrop.
 *
 * `fixed`, not `absolute`: the page scrolls and the app's background does not
 * move with its content. Pinning it also means the glass surfaces above always
 * have the same thing to frost as you scroll, which is what makes them read as
 * panes over a desktop rather than as tinted cards drifting up the page.
 *
 * Server-rendered with the light values — light is the default, as it is in the
 * app — and corrected in an effect if the visitor's theme is dark. That ordering
 * is deliberate: the alternative is a hydration mismatch on every dark-mode
 * visit.
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
      {/* Painted larger than the viewport and drifting slowly, so the washes
          never settle into looking like a static image behind the glass. The
          overscan keeps the edges covered while the transform moves. */}
      <div
        className="animate-drift absolute inset-[-6%]"
        style={{ background: PORCELAIN.layers }}
      />

      {/* The dark-mode veil. Skipped entirely at zero rather than layered as a
          no-op, so the light path paints one element fewer. */}
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
