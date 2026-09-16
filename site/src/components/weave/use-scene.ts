"use client";

import { useEffect, useRef, useState } from "react";
import {
  BEATS,
  frameSignature,
  greetingFor,
  sceneAt,
  settledScene,
  type Frame,
} from "@/lib/scene";

/**
 * The clock behind the cloth.
 *
 * *What* happens is entirely in `@/lib/scene`, which is pure data and pure
 * functions — so the ordering the animation depends on is covered by tests that
 * need no DOM, no fake timer and no renderer. This hook owns only the three
 * concerns that genuinely need a browser:
 *
 *  1. **Whether it runs.** An `IntersectionObserver` starts the clock when the
 *     cloth is actually on screen and pauses it on the way out, so a page nobody
 *     is looking at is not burning animation frames.
 *  2. **How often it renders.** It ticks on every frame but commits state only
 *     when the *rendered output* changes — see below.
 *  3. **Whether it should run at all.** `prefers-reduced-motion` short-circuits to
 *     the settled frame: all of the content, none of the theatre.
 *
 * The local hour is read here too, because the server cannot know it and a
 * confidently wrong greeting is more noticeable than one that arrives late.
 *
 * ---------------------------------------------------------------------------
 * Why the commit is gated on a signature
 * ---------------------------------------------------------------------------
 *
 * The obvious version calls `setElapsed` from every animation frame, re-rendering
 * this whole subtree sixty times a second. Almost all of those renders produce
 * byte-identical output: the reply gains a character twenty-odd times a second,
 * and once the turn settles nothing changes at all while the clock runs on to the
 * end of the loop.
 *
 * So the clock lives in a ref and state is committed only when `frameSignature`
 * differs from the last committed frame. The DOM is identical, React does roughly
 * a third of the work, and the settled seconds cost nothing at all.
 */
export interface Scene extends Frame {
  /** True when the clock is running — i.e. the cloth has been seen. */
  playing: boolean;
  /** The visitor's local greeting, or null before mount. */
  greeting: string | null;
  ref: React.RefObject<HTMLDivElement | null>;
}

export function useScene(): Scene {
  const ref = useRef<HTMLDivElement | null>(null);

  // The clock is a ref, not state: it changes sixty times a second and nothing
  // renders from its value directly.
  const elapsed = useRef(0);
  const signature = useRef(frameSignature(sceneAt(0)));

  const [frame, setFrame] = useState<Frame>(() => sceneAt(0));
  const [playing, setPlaying] = useState(false);
  const [reduced, setReduced] = useState(false);
  const [greeting, setGreeting] = useState<string | null>(null);
  const [mounted, setMounted] = useState(false);

  // Both of these are only knowable in the browser, and doing them in one effect
  // keeps the first client render identical to the server's, so hydration has
  // nothing to disagree about.
  useEffect(() => {
    setMounted(true);
    setGreeting(greetingFor(new Date().getHours()));
    setReduced(window.matchMedia("(prefers-reduced-motion: reduce)").matches);
  }, []);

  // The visibility gate. The observer stays connected after the first
  // intersection: the scene loops, so it has to keep answering "is it on screen?"
  // for the whole visit.
  useEffect(() => {
    const element = ref.current;
    if (!element || reduced) return;

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) setPlaying(entry.isIntersecting);
      },
      { threshold: 0.15 },
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

      // Compute every frame — `sceneAt` is pure and cheap — but only re-render
      // when something a visitor could see has changed.
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
  // preference, so reporting the settled frame on the very first client render
  // would be a hydration mismatch. One frame of the empty state is invisible.
  const committed = reduced && mounted ? settledScene() : frame;

  return { ...committed, playing, greeting, ref };
}
