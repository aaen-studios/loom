"use client";

import { useEffect, useRef, useState } from "react";
import { figure, type Figure, type FigureKind, type Pointer } from "@/lib/weave";
import { FigureSvg } from "./figure";

/**
 * The hero's weave: the one figure on the page that moves.
 *
 * ---------------------------------------------------------------------------
 * What moves, and what that is for
 * ---------------------------------------------------------------------------
 *
 * Three things, and each of them is a *force* rather than an effect.
 *
 *   - The threads drift. A thread field with a slow sine phase running through it is under tension
 *     and relaxing by turns, which is what a warp actually does.
 *   - The reader pushes it. The cursor's position is fed into the geometry as a repulsion, so moving
 *     the mouse bends the threads nearest to it. Not a hover state and not a texture that follows a
 *     cursor — the *figure* is different because the cursor is there.
 *   - It settles when the pointer stops and when the tab is hidden.
 *
 * ---------------------------------------------------------------------------
 * Why there is no React state on the animation path
 * ---------------------------------------------------------------------------
 *
 * The geometry is drawn into the DOM imperatively, by setting `d` attributes on the paths that were
 * rendered on the first frame. That is deliberate and it is the whole performance story: thirty
 * threads × forty samples is twelve hundred points recomputed per frame, and putting that through
 * `setState` would reconcile a thousand-element SVG sixty times a second.
 *
 * So React renders the *first* frame — the same frame the server rendered, from the same seed and
 * the same arguments — and then this effect takes over and mutates the paths it finds. There is no
 * hydration mismatch, there is no flash, and the interpolated strings never leave the browser.
 *
 * ---------------------------------------------------------------------------
 * Why it is polite about not running
 * ---------------------------------------------------------------------------
 *
 * Five conditions stop it, and they are all things that would make it a worse page rather than a
 * prettier one: `prefers-reduced-motion` (the reader asked), `document.hidden` (nobody is looking),
 * an off-screen hero (nothing to see), a small viewport (the threads are dense and the cost is
 * proportionally higher), and `IntersectionObserver` reporting a stale box.
 *
 * The pointer is tracked but *not* on the animation path — it writes to a ref, and the next frame
 * reads it. A cursor that moves forty times between two frames costs one write.
 */
/**
 * A figure that drifts, at a fixed size — the animated counterpart of `Still`.
 *
 * ---------------------------------------------------------------------------
 * Why this exists, and what was wrong with only animating the hero
 * ---------------------------------------------------------------------------
 *
 * The page had one moving figure and six still ones. That was defended as restraint — "five of the six
 * figures here cost a visitor nothing at all" — and restraint is not what it looked like. It looked
 * *inert*. A field of threads is a picture of material under tension, and material under tension moves;
 * a frozen one reads as a diagram of itself, which is the failure the hero was rebuilt to escape and
 * which the other six figures were quietly still making. The hero apologised for the rest of the page.
 *
 * So the drift is shared rather than special. This reuses `AnimatedWeave`'s machinery exactly — the same
 * phase, the same clamping, the same politeness — and differs in three ways, all of them because these
 * figures are *beside* prose rather than behind a headline:
 *
 *   - **Slower.** A quarter the hero's rate. A figure the reader is scrolling past does not need to
 *     announce itself; twelve seconds per cycle is enough that a glance sees movement and a paragraph of
 *     reading does not see a loop.
 *   - **No pointer.** The pull is right for the hero because the reader's cursor is *in* that figure.
 *     Applying it to a decorative plate would make the page twitch as the pointer crossed it, which is a
 *     worse page. These drift; they do not react.
 *   - **Cheap when unseen.** These are below the fold, so the observer is not an optimisation here — it
 *     is the whole reason this is affordable. Six figures each computing their own geometry every frame
 *     would be six `requestAnimationFrame` loops; this runs one per visible figure and none otherwise.
 */
export function Drifting({
  kind,
  seed,
  width = 1200,
  height = 620,
  detail = 1,
  className,
  opacity,
  label,
  id,
}: {
  kind: FigureKind;
  seed: number;
  width?: number;
  height?: number;
  detail?: number;
  className?: string;
  opacity?: number;
  label?: string;
  id: string;
}) {
  const host = useRef<HTMLDivElement>(null);
  const frame = useRef(0);

  const [shape] = useState<Figure>(() => figure(kind, { width, height, seed, detail }));

  useEffect(() => {
    const element = host.current;
    if (!element) return;

    const motion = window.matchMedia("(prefers-reduced-motion: reduce)");
    if (motion.matches || window.innerWidth < 768) return;

    const paths = Array.from(element.querySelectorAll<SVGPathElement>("path"));
    if (paths.length === 0) return;

    let running = false;
    let start = 0;

    const draw = (elapsed: number) => {
      // Twelve seconds a cycle rather than the hero's twenty-four. See the note above: a decorative
      // figure should be *caught* moving rather than seen to move.
      const phase = (elapsed / 12000) % 1;
      const next = figure(kind, { width, height, seed, detail, phase });

      for (let i = 0; i < paths.length && i < next.paths.length; i += 1) {
        paths[i].setAttribute("d", next.paths[i]);
      }
    };

    const loop = (now: number) => {
      if (!running) return;
      if (!start) start = now;
      draw(now - start);
      frame.current = requestAnimationFrame(loop);
    };

    const startLoop = () => {
      if (running) return;
      running = true;
      frame.current = requestAnimationFrame(loop);
    };

    const stopLoop = () => {
      running = false;
      if (frame.current) cancelAnimationFrame(frame.current);
      frame.current = 0;
    };

    const visible = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting && !document.hidden) startLoop();
        else stopLoop();
      },
      // A margin, unlike the hero's bare threshold: these figures are scattered down a long page, and
      // starting each one exactly as it crests the fold means the first thing a reader sees at the bottom
      // of every scroll is a figure *beginning*. Starting a little early means it is already running.
      { rootMargin: "200px" },
    );
    visible.observe(element);

    const onVisibility = () => {
      if (document.hidden) stopLoop();
    };
    document.addEventListener("visibilitychange", onVisibility);

    return () => {
      stopLoop();
      visible.disconnect();
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [kind, seed, width, height, detail]);

  return (
    <div ref={host} className="h-full w-full">
      <FigureSvg
        kind={kind}
        shape={shape}
        className={className}
        opacity={opacity}
        label={label}
        id={id}
      />
    </div>
  );
}

export function AnimatedWeave({
  seed,
  className,
}: {
  seed: number;
  className?: string;
}) {
  const host = useRef<HTMLDivElement>(null);
  const pointer = useRef<Pointer | null>(null);
  const frame = useRef(0);

  /*
   * The first frame, as React state — rendered once and never updated.
   *
   * `useState` with a lazy initialiser rather than `useMemo`, because the *server* has to produce
   * the same geometry: the initial value is computed during the server render, serialised into the
   * HTML, and this component's first client render recomputes it identically. A `useMemo` would do
   * the same thing here, and the state reads more honestly as "this is the figure, and it does not
   * change".
   */
  const [shape] = useState<Figure>(() =>
    figure("field", { width: 1600, height: 900, seed, detail: 1.15 }),
  );

  useEffect(() => {
    const element = host.current;
    if (!element) return;

    const motion = window.matchMedia("(prefers-reduced-motion: reduce)");

    // A small viewport gets the still frame. The threads are dense at 1600 wide, and a phone
    // recomputing twelve hundred points per frame is a phone that gets warm in the hand.
    if (motion.matches || window.innerWidth < 768) return;

    const paths = Array.from(element.querySelectorAll<SVGPathElement>("path"));
    if (paths.length === 0) return;

    let running = false;
    let start = 0;
    const WIDTH = 1600;
    const HEIGHT = 900;

    const draw = (elapsed: number) => {
      // A full cycle every 24 seconds. Slow enough that the drift reads as tension rather than as
      // motion — anything faster and the eye starts tracking individual threads, which is the
      // difference between a field and a screensaver.
      const phase = (elapsed / 24000) % 1;

      const next = figure("field", {
        width: WIDTH,
        height: HEIGHT,
        seed,
        detail: 1.15,
        phase,
        pointer: pointer.current,
      });

      // Written straight onto the elements React rendered. `paths` and `next.paths` are the same
      // length because both come from the same seed and detail — this is the contract between the
      // first frame and the animation, and it is why `detail` is not a prop here.
      for (let i = 0; i < paths.length && i < next.paths.length; i += 1) {
        paths[i].setAttribute("d", next.paths[i]);
      }
    };

    const loop = (now: number) => {
      if (!running) return;
      if (!start) start = now;
      draw(now - start);
      frame.current = requestAnimationFrame(loop);
    };

    const startLoop = () => {
      if (running) return;
      running = true;
      frame.current = requestAnimationFrame(loop);
    };

    const stopLoop = () => {
      running = false;
      if (frame.current) cancelAnimationFrame(frame.current);
      frame.current = 0;
    };

    /*
     * Only while the hero is on screen.
     *
     * The observer also *reports* visibility, so a reader who scrolls past the hero and back gets a
     * figure that resumes rather than one that has been animating, unseen, the whole way down the
     * page — which on a long page is most of a laptop's battery.
     */
    const visible = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting && !document.hidden) startLoop();
        else stopLoop();
      },
      { threshold: 0 },
    );
    visible.observe(element);

    // And not while the tab is in the background. Browsers throttle `requestAnimationFrame` in a
    // hidden tab rather than stopping it, so without this the figure keeps computing frames it will
    // never be asked to paint.
    const onVisibility = () => {
      if (document.hidden) stopLoop();
      else if (element.getBoundingClientRect().top < window.innerHeight) startLoop();
    };
    document.addEventListener("visibilitychange", onVisibility);

    /*
     * The cursor, written to a ref and never to state.
     *
     * Coordinates are normalised to the host's box rather than taken as page coordinates, so the
     * geometry can be computed in its own 1600×900 space and does not care where the hero happens to
     * sit on the page.
     */
    const onPointerMove = (event: PointerEvent) => {
      const box = element.getBoundingClientRect();
      pointer.current = {
        x: (event.clientX - box.left) / box.width,
        y: (event.clientY - box.top) / box.height,
        // A plain 0–1 intensity. The figure decides what that means in units, because the displacement
        // is bounded by the figure's own pitch — see the note on `pull` in `lib/weave.ts`.
        strength: 1,
      };
    };

    // Relaxed while dragging, so a selection over the hero does not fling the threads around.
    const onPointerDown = () => {
      if (pointer.current) pointer.current = { ...pointer.current, strength: 0.5 };
    };

    // And a slow release when the cursor leaves, rather than the figure snapping back to rest the
    // instant it crosses the edge of the box — which reads as a bug even though it is correct.
    const onPointerLeave = () => {
      const from = pointer.current;
      if (!from) return;
      const began = performance.now();

      const relax = () => {
        const t = Math.min(1, (performance.now() - began) / 420);
        if (t >= 1 || !pointer.current) {
          pointer.current = null;
          return;
        }
        pointer.current = { ...from, strength: from.strength * (1 - t) };
        requestAnimationFrame(relax);
      };

      requestAnimationFrame(relax);
    };

    element.addEventListener("pointermove", onPointerMove);
    element.addEventListener("pointerdown", onPointerDown);
    element.addEventListener("pointerleave", onPointerLeave);

    // Reacting to a change in the preference rather than only reading it at mount: someone who turns
    // reduced motion on while the page is open expects the movement to stop, not to keep going until
    // the next navigation.
    const onMotionChange = () => {
      if (motion.matches) stopLoop();
    };
    motion.addEventListener("change", onMotionChange);

    return () => {
      stopLoop();
      visible.disconnect();
      document.removeEventListener("visibilitychange", onVisibility);
      element.removeEventListener("pointermove", onPointerMove);
      element.removeEventListener("pointerdown", onPointerDown);
      element.removeEventListener("pointerleave", onPointerLeave);
      motion.removeEventListener("change", onMotionChange);
    };
  }, [seed]);

  return (
    <div ref={host} className={className}>
      <FigureSvg kind="field" shape={shape} className="h-full w-full" id={`hero-${seed}`} />
    </div>
  );
}
