"use client";

import { useEffect } from "react";

/**
 * The shuttle, and the weft it carries.
 *
 * The line crossing the page is the reading position: the weft is where the weaving
 * has got to. The active pass is the last one whose top has passed the reading line —
 * a point 42% down the viewport — and the line parks near that pass's top edge.
 *
 * ---------------------------------------------------------------------------
 * Why this writes a CSS variable instead of setting state
 * ---------------------------------------------------------------------------
 *
 * The obvious version keeps the position in `useState`, re-rendering a client
 * component on every scroll frame. A one-pixel line moving needs none of that: the
 * position is written straight onto `<html>` as `--weft-y` and the `.weft` rule reads
 * it. Tracking the scroll therefore costs zero renders, and this component never
 * re-renders after mount.
 *
 * Measurement is coalesced through `requestAnimationFrame`, so a scroll that fires
 * forty times between two frames is measured once.
 *
 * ---------------------------------------------------------------------------
 * Why this is NOT skipped under reduced motion
 * ---------------------------------------------------------------------------
 *
 * It used to bail out entirely when `prefers-reduced-motion: reduce` was set, on the
 * reasoning that a moving line is motion. That reasoning was wrong, and a probe
 * caught it: headless Chrome reports `reduce`, and the reading came back with the
 * weft sitting at its initial value — which is what any reduced-motion visitor would
 * have seen. An invisible line.
 *
 * The distinction that matters is between the line's *position* and its *movement*.
 * The position is information — this is the pass you are in — and withholding
 * information because someone asked for less animation is not respecting the
 * preference, it is removing a feature. The movement is the `transition` on `top`,
 * and that is already collapsed to nothing by the shared reduced-motion override in
 * the token sheet, which every animated property in this project is subject to.
 *
 * So the line is always positioned, and the preference only removes the glide.
 */
export function Shuttle() {
  useEffect(() => {
    const root = document.documentElement;

    let queued = 0;

    const measure = () => {
      queued = 0;

      const passes = Array.from(
        document.querySelectorAll<HTMLElement>("[data-pass]"),
      );
      if (passes.length === 0) return;

      const readingLine = window.innerHeight * 0.42;

      // The active pass is the last one whose top has crossed the reading line.
      // Before any pass has, the weft sits at the first — so the line is already
      // there when the cloth starts, rather than floating off-screen.
      let active = passes[0];
      for (const pass of passes) {
        if (pass.getBoundingClientRect().top <= readingLine) active = pass;
      }

      const top = active.getBoundingClientRect().top;
      root.style.setProperty("--weft-y", `${Math.round(top)}px`);
      root.style.setProperty("--weft-on", "1");
    };

    const schedule = () => {
      if (queued) return;
      queued = requestAnimationFrame(measure);
    };

    measure();
    window.addEventListener("scroll", schedule, { passive: true });
    window.addEventListener("resize", schedule);

    return () => {
      if (queued) cancelAnimationFrame(queued);
      window.removeEventListener("scroll", schedule);
      window.removeEventListener("resize", schedule);
      root.style.removeProperty("--weft-y");
      root.style.removeProperty("--weft-on");
    };
  }, []);

  // Present in the server render so the element exists before measurement, and
  // hidden by `--weft-on: 0` until it has a position worth showing.
  return <div aria-hidden="true" className="weft" />;
}
