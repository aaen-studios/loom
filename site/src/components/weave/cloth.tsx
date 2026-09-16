"use client";

import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/cn";
import { reasoningPreview, thinkingLabel } from "@/lib/scene";
import { Prose } from "./prose";
import type { Scene } from "./use-scene";

/** The workspace this turn is working in — a real directory in this repository. */
const WORKSPACE = "loom";

/**
 * The cloth: the app's transcript, rendered as the thing a loom produces.
 *
 * ---------------------------------------------------------------------------
 * Why this is a cloth and not a window
 * ---------------------------------------------------------------------------
 *
 * An earlier draft of this page showed the app in a drawn window — a title bar, a
 * chat header, traffic-light buttons — and it was accurate and it was inert. A
 * picture of a window invites you to look *at* it. A cloth that grows while you
 * read invites you to watch *it happen*, which is the only thing about an agent
 * worth showing.
 *
 * So the frame is gone. What is left is the part that was always the interesting
 * part: a rail down the left with a knot for each thing that happened, the
 * reasoning spinning down one thread, the tool call tied off as a knot, and the
 * reply beaten in across it.
 *
 * ---------------------------------------------------------------------------
 * The reading follows the weft
 * ---------------------------------------------------------------------------
 *
 * The app tracks whether the transcript is *pinned* to the bottom rather than
 * jumping on every token: it measures the distance to the end, and anything under
 * 90px counts as pinned. Scrolling up unpins it, the follow stops, and a control
 * appears to come back. Without that, anyone scrolling back to re-read a message
 * is yanked forward again by the next character.
 *
 * One adaptation the app does not need: it scrolls with `scrollIntoView`, which
 * walks up and scrolls *every* scrollable ancestor. Inside a desktop window that
 * is only the transcript; on a page it would also scroll the document, so the
 * visitor's own scroll position would be hijacked by the animation. Assigning
 * `scrollTop` on the container does the same thing inside the frame and touches
 * nothing outside it.
 */
export function Cloth({ scene }: { scene: Scene }) {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const [pinned, setPinned] = useState(true);

  const onScroll = () => {
    const element = scrollRef.current;
    if (!element) return;
    const distance = element.scrollHeight - element.scrollTop - element.clientHeight;
    setPinned(distance < 90);
  };

  // `scene.elapsed` is the right dependency precisely because of how the clock
  // commits: a frame is published only when the rendered output changes, so this
  // advances exactly when the cloth grows rather than once per animation frame.
  useEffect(() => {
    if (!pinned) return;
    const element = scrollRef.current;
    if (!element) return;
    element.scrollTop = element.scrollHeight;
  }, [scene.elapsed, pinned]);

  const comeBack = () => {
    setPinned(true);
    const element = scrollRef.current;
    if (!element) return;
    element.scrollTo({ top: element.scrollHeight, behavior: "smooth" });
  };

  const arriving = scene.replyText !== "" && !scene.usageText;

  return (
    <div className="panel-strong rounded-sheet relative flex h-[400px] flex-col overflow-hidden sm:h-[460px]">
      <div className="border-b border-[var(--glass-border)] px-4 py-2.5">
        <p className="text-faint flex items-center gap-2.5 text-[10.5px] font-medium tracking-[0.16em] uppercase">
          The cloth
          <span className="text-ghost flex-1" />
          <span className="normal-case tracking-normal">
            working in {WORKSPACE}
          </span>
        </p>
      </div>

      <div
        ref={scrollRef}
        onScroll={onScroll}
        // Test hooks, and only that. `data-scene-phase` is the one thing a browser
        // probe cannot otherwise determine about this component: whether the clock
        // is running at all. It was added after a probe came back showing the settled
        // frame and there was no way to tell whether the animation had finished or
        // never started.
        data-scene-phase={scene.phase}
        data-scene-running={scene.playing ? "yes" : "no"}
        className="loom-scroll min-h-0 flex-1 overflow-y-auto px-4 py-4"
      >
        <div className="cloth flex flex-col gap-4">
          {/* The thread tying the passes together, running behind their knots. */}
          <span aria-hidden="true" className="cloth-rail" />

          {/* The opening state: the mark weaving itself in, then the greeting and
              the workspace line on the app's own stagger — the delays its chat
              canvas uses, so this opens the way the product opens. */}
          {!scene.sent && (
            <div className="flex flex-col items-start py-2">
              <div className="cloth-pass w-full">
                {/* A `<p>`, not a heading. This is scenery — a picture of the app's
                    opening screen — and making it a heading would put an `h2`
                    directly under the page's `h1` that names nothing a reader
                    would navigate by. The app's own greeting *is* its `h1`,
                    because there it is the only thing on screen. */}
                <p
                  className="intro-step text-[22px] font-medium tracking-tight"
                  style={{ animationDelay: "180ms" }}
                >
                  {/* The app greets by the hour. Rendered only after mount,
                      because the server cannot know what time it is where the
                      visitor is — and a confidently wrong greeting is worse than
                      one that arrives a moment late. */}
                  {scene.greeting ?? "\u00a0"}
                </p>
                <p
                  className="intro-step text-faint mt-1.5 text-[13px] leading-5"
                  style={{ animationDelay: "260ms" }}
                >
                  Working in{" "}
                  <span className="text-soft font-medium">{WORKSPACE}</span>
                </p>
              </div>
            </div>
          )}

          {/* Pick one: the prompt, laid across the warp. */}
          {scene.sent && (
            <div className="cloth-pass animate-fade-up">
              <p className="text-faint text-[10.5px] font-medium tracking-[0.16em] uppercase">
                You
              </p>
              <p className="text-soft mt-1.5 text-[13.5px] leading-[1.6]">
                {scene.prompt}
              </p>
            </div>
          )}

          {/* Pick two: the reasoning, spinning down its thread. The preview trails
              off under a mask rather than ending in an ellipsis, because it is cut
              by the edge of the panel rather than by the sentence. */}
          {scene.thinking && (
            <div className="cloth-pass animate-fade-up" data-live="true">
              <p className="text-faint mb-1 flex items-center gap-1.5 text-[10.5px] font-medium tracking-[0.16em] uppercase">
                <span
                  className={cn(scene.thinking && "thinking-shimmer")}
                >
                  {thinkingLabel(scene.thinking)}
                </span>
              </p>
              <p className="text-soft thinking-preview overflow-hidden text-[12.5px] leading-[1.6]">
                {reasoningPreview(scene.reasoningText, true)}
              </p>
            </div>
          )}

          {/* Pick three: a tool call, tied off as a knot. The elapsed time is the
              whole shape of a tool call as a reader meets it, and the file name is
              what makes it concrete. */}
          {scene.tool !== "idle" && (
            <div className="cloth-pass animate-fade-up" data-live={scene.tool === "running"}>
              <p className="text-faint text-[10.5px] font-medium tracking-[0.16em] uppercase">
                Read
              </p>
              <div className="mt-1.5 flex items-center gap-2.5">
                <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1.5 py-[0.15em] font-mono text-[12px]">
                  src/lib/background.ts
                </code>
                {scene.tool === "running" ? (
                  <Pulse />
                ) : (
                  <span className="text-faint text-[11.5px] tabular-nums">0.4s</span>
                )}
              </div>
            </div>
          )}

          {/* The weft: the reply, streaming in and growing a line at a time. */}
          {scene.replyText && (
            <div className="cloth-pass" data-live={arriving}>
              <p className="text-faint text-[10.5px] font-medium tracking-[0.16em] uppercase">
                Loom
              </p>
              <div className="mt-1.5">
                <Prose text={scene.replyText} streaming={arriving} />
              </div>
              {/* The app renders usage only once the turn has stopped streaming.
                  A running turn has no final count to report, so showing one here
                  would be inventing a number. */}
              {scene.usageText && (
                <p className="text-faint mt-2 text-[11.5px]">{scene.usageText}</p>
              )}
            </div>
          )}
        </div>
      </div>

      {/* Appears only when the follow has been interrupted, so the control is its
          own explanation of what happened. */}
      {!pinned && (
        <button
          type="button"
          onClick={comeBack}
          className="panel-strong rounded-capsule text-soft animate-fade-in absolute bottom-3 left-1/2 -translate-x-1/2 px-3 py-1.5 text-[12px]"
        >
          Come back to the weft
        </button>
      )}
    </div>
  );
}

/**
 * A three-dot activity pulse.
 *
 * Built from the shared `cursor-blink` animation on staggered delays rather than
 * from the app's own three-bar weft indicator: that indicator lives in the app's
 * *quick-ask overlay* section, which is deliberately outside the shared regions
 * and which `verify-tokens.mjs` fails the build over if it reaches this
 * stylesheet. So this is assembled from a shared motion token instead.
 */
function Pulse() {
  return (
    <span className="flex items-center gap-[3px]" aria-hidden="true">
      {[0, 160, 320].map((delay) => (
        <span
          key={delay}
          className="cursor-blink bg-[var(--accent)] block h-1 w-1 rounded-full"
          style={{ animationDelay: `${delay}ms` }}
        />
      ))}
    </span>
  );
}
