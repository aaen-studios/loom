"use client";

import { useEffect, useRef } from "react";
import type { ReactNode } from "react";

/**
 * A fade-and-rise on first sight.
 *
 * ---------------------------------------------------------------------------
 * Why the server renders nothing and the client arms it
 * ---------------------------------------------------------------------------
 *
 * The obvious version renders `opacity: 0` into the server HTML and reveals on intersection. That is a
 * visible bug in three situations which all look the same from the outside: with JavaScript off, the
 * content is invisible forever; on a slow connection, it is invisible until the bundle lands; and on a
 * fast one, anything already in view on mount *flashes* because the observer's first callback arrives a
 * frame after paint.
 *
 * So the server renders an ordinary element and this effect arms the animation only if the element is
 * genuinely below the fold at mount. Anything already on screen is left alone and never hides. The
 * consequence is that this component's worst case is "no animation" rather than "no content", which is
 * the only trade worth making for a decoration — and it is why this is safe to put around body copy.
 *
 * `prefers-reduced-motion` short-circuits before anything is armed, so for those readers the whole thing
 * is a plain `div`. There is a matching rule in `globals.css` that un-arms it from the stylesheet side
 * as well, because a reader who turns the preference on while the page is open should not have to wait
 * for a re-render.
 */
export function Reveal({
  children,
  delay = 0,
  className,
}: {
  children: ReactNode;
  /** Milliseconds of stagger, for a row of things arriving together. */
  delay?: number;
  className?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const element = ref.current;
    if (!element) return;

    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;

    // Already on screen: leave it. This is the line that makes the component's worst case a missing
    // animation rather than missing text.
    if (element.getBoundingClientRect().top <= window.innerHeight * 0.92) return;

    element.dataset.reveal = "armed";

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          element.dataset.reveal = "in";
          observer.disconnect();
        }
      },
      // Triggering above the fold, so the transition is mostly finished by the time the element is
      // properly in view rather than still arriving while somebody reads it.
      { rootMargin: "0px 0px -12% 0px" },
    );

    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  return (
    <div
      ref={ref}
      className={className}
      style={delay ? { transitionDelay: `${delay}ms` } : undefined}
    >
      {children}
    </div>
  );
}
