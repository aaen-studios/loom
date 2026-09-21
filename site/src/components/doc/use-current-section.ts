"use client";

import { useEffect, useState } from "react";

export interface CurrentSection {
  number: string;
  title: string;
}

/**
 * Which section the reader is in.
 *
 * Written to state and read by two components — the running head, which names the
 * section on a narrow viewport where the margin index is not on screen, and the
 * margin index, which marks your position in the contents. They exist so the
 * answer to "where am I" is always in exactly one place: never zero, never two.
 *
 * The measurement is coalesced through `requestAnimationFrame`, so a scroll that
 * fires forty times between two frames is measured once. That matters more than it
 * looks: this effect is mounted twice (once per consumer) and each run walks the
 * sections and calls `getBoundingClientRect`, which forces layout.
 *
 * `setCurrent` bails out when the section has not changed, so the two components
 * re-render roughly ten times in a long read rather than on every frame.
 *
 * It reads the DOM rather than a scroll library because the document *is* the DOM:
 * the sections carry `data-section`, the browser knows where they are, and
 * subscribing to that fact is a few lines rather than a dependency.
 */
export function useCurrentSection(): CurrentSection | null {
  const [current, setCurrent] = useState<CurrentSection | null>(null);

  useEffect(() => {
    let queued = 0;

    const measure = () => {
      queued = 0;

      const sections = Array.from(
        document.querySelectorAll<HTMLElement>("[data-section]"),
      );
      // A page with no sections — the 404 — measures nothing and reports nothing,
      // rather than reporting the first of an empty list.
      if (sections.length === 0) return;

      // The line is 30% down the viewport: a little above centre, because that is
      // the part of a screen a reader is actually in, and because a section whose
      // heading has just crossed it is the section you are reading rather than the
      // one you have finished.
      const line = window.innerHeight * 0.3;
      let found = sections[0];
      for (const section of sections) {
        if (section.getBoundingClientRect().top <= line) found = section;
      }

      const next: CurrentSection = {
        number: found.dataset.section ?? "",
        title: found.dataset.sectionTitle ?? "",
      };

      setCurrent((previous) =>
        previous &&
        previous.number === next.number &&
        previous.title === next.title
          ? previous
          : next,
      );
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
    };
  }, []);

  return current;
}
