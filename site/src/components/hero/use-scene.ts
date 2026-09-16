"use client";

import { useEffect, useRef, useState } from "react";
import {
  BEATS,
  frameSignature,
  greetingFor,
  sceneAt,
  settledScene,
  type SceneFrame,
} from "@/lib/timeline";

/**
 * The clock behind the hero.
 *
 * Everything about *what* happens is in `@/lib/timeline`, which is pure data and
 * pure functions — so the ordering the animation depends on is covered by
 * `timeline.test.ts` without a DOM, a timer or a renderer. This hook owns only
 * the three browser-only concerns:
 *
 * 1. **When it runs.** An `IntersectionObserver` gates the clock on the window
 *    being on screen, so the animation is not burning frames while a visitor
 *    reads the sections below, and it resumes exactly where it stopped.
 * 2. **How often it renders.** It ticks every frame but commits state only when
 *    the rendered output actually changes, which is what keeps a 60fps clock
 *    from causing 60 React renders a second. See below.
 * 3. **Whether it runs at all.** `prefers-reduced-motion` short-circuits to the
 *    settled frame: all of the content, none of the theatre.
 *
 * The local hour is read here too, because the server has no idea what time it
 * is where the visitor is — a wrong greeting would be a hydration mismatch.
 *
 * ---------------------------------------------------------------------------
 * Why the render is gated on a signature
 *
 * The obvious implementation calls `setElapsed` from every animation frame,
 * which re-renders this entire subtree 60 times a second. Almost all of those
 * renders produce byte-identical output: the reply only gains a character
 * twenty-odd times a second, and once the turn settles it stops changing
 * altogether while the clock keeps running until the loop restarts.
 *
 * So the clock is kept in a ref, and state is committed only when
 * `frameSignature` differs from the last committed frame. The DOM ends up
 * identical, and React does roughly a third of the work — with the settled
 * seconds costing nothing at all.
 * ---------------------------------------------------------------------------
 */
export interface Scene extends SceneFrame {
  /** True once the clock is running (the window has been seen). */
  playing: boolean;
  /** Locally-correct greeting, or null before mount. */
  greeting: string | null;
  ref: React.RefObject<HTMLDivElement | null>;
}

export function useScene(): Scene {
  const ref = useRef<HTMLDivElement | null>(null);

  // The clock lives in a ref rather than in state: it changes 60 times a second
  // and nothing renders from its value directly.
  const elapsed = useRef(0);
  const signature = useRef(frameSignature(sceneAt(0)));

  const [frame, setFrame] = useState<SceneFrame>(() => sceneAt(0));
  const [playing, setPlaying] = useState(false);
  const [reduced, setReduced] = useState(false);
  const [greeting, setGreeting] = useState<string | null>(null);
  const [mounted, setMounted] = useState(false);

  // Both of these are only knowable in the browser, and doing them in one effect
  // keeps the first client render identical to the server's so hydration has
  // nothing to disagree about.
  useEffect(() => {
    setMounted(true);
    setGreeting(greetingFor(new Date().getHours()));
    setReduced(window.matchMedia("(prefers-reduced-motion: reduce)").matches);
  }, []);

  // The visibility gate. The observer is *not* disconnected on first entry: the
  // scene loops, so it has to keep answering "is it on screen?" all visit.
  useEffect(() => {
    const element = ref.current;
    if (!element || reduced) return;

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) setPlaying(entry.isIntersecting);
      },
      { threshold: 0.2 },
    );

    observer.observe(element);
    return () => observer.disconnect();
  }, [reduced]);

  useEffect(() => {
    if (reduced || !playing) return;

    let handle = 0;
    let previous = performance.now();

    const tick = (now: number) => {
      const delta = now - previous;
      previous = now;

      let next = elapsed.current + delta;
      if (next >= BEATS.loopAt) next -= BEATS.loopAt;
      elapsed.current = next;

      // Only commit when the rendered output changes. `sceneAt` is pure and
      // cheap, so computing it every frame is fine; re-rendering is what is not.
      const candidate = sceneAt(next);
      const nextSignature = frameSignature(candidate);
      if (nextSignature !== signature.current) {
        signature.current = nextSignature;
        setFrame(candidate);
      }

      handle = requestAnimationFrame(tick);
    };

    handle = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(handle);
  }, [playing, reduced]);

  // `mounted` is required as well as `reduced`: the server cannot know the
  // preference, so reporting the settled frame on the first client render would
  // be a hydration mismatch. One frame of the empty state is invisible.
  const committed = reduced && mounted ? settledScene() : frame;

  return {
    ...committed,
    playing,
    greeting,
    ref,
  };
}


