"use client";

import { cn } from "@/lib/cn";
import { GOAL, TASKS } from "@/lib/scene";
import type { Scene } from "./use-scene";

/**
 * The draft: the plan for the cloth, standing beside the cloth itself.
 *
 * On a loom the draft is drawn *first* — a squared notation of which threads lift
 * on which pick — and the weaving is then checked against it. This is the same
 * object for a turn: the task list the model maintains, checking itself off while
 * the reply streams in beside it.
 *
 * Nothing here is invented for a landing page. The app really does keep a live task
 * panel and a `/goal` objective, and `todo_write` really does replace the whole list
 * at once. Rendering the plan and the result side by side is what the product
 * already does; the draft is a name for it rather than a metaphor laid over it.
 *
 * ---------------------------------------------------------------------------
 * The empty state is not an early return
 * ---------------------------------------------------------------------------
 *
 * This used to render nothing until the turn was sent, which meant the card appeared
 * three seconds in and pushed the cloth down the page while a visitor was reading
 * it. Instead it always draws, with the same heading and the same height, and only
 * its body swaps once there is a plan to show. The card is furniture; the plan is
 * what arrives.
 */
export function Draft({ scene, className }: { scene: Scene; className?: string }) {
  return (
    <div className={cn("panel rounded-sheet p-4", scene.sent && "animate-fade-up", className)}>
      <div className="flex items-center gap-2.5">
        <span className="text-faint flex items-center gap-2 text-[10.5px] font-medium tracking-[0.16em] uppercase">
          <span className={cn("knot", scene.sent && "knot-lit")} />
          Draft
        </span>
        <span className="flex-1" />
        {scene.sent && (
          <span className="text-faint rounded-capsule border border-[var(--glass-border)] px-1.5 py-0.5 text-[10.5px] tabular-nums">
            {scene.tasksDone}/{TASKS.length}
          </span>
        )}
      </div>

      {scene.sent ? (
        <>
          <p className="text-soft mt-3 text-[13px] leading-5">
            <span className="text-faint">Objective · </span>
            {GOAL}
          </p>

          <ul className="mt-3 space-y-2.5">
            {TASKS.map((task, index) => {
              const status = scene.taskStatus[index];
              return (
                <li key={task.id} className="draft-row">
                  {/* The lift indicator: filled means the thread is up on this pick,
                      which is exactly what a real draft records. */}
                  <span
                    aria-hidden="true"
                    className={cn(
                      "mt-[1px] grid h-3.5 w-3.5 place-items-center",
                      status === "pending" && "text-faint",
                      status === "in_progress" && "text-[var(--thread-bright)]",
                      status === "completed" && "text-[var(--accent)]",
                    )}
                  >
                    {status === "completed" ? (
                      <CheckIcon />
                    ) : status === "in_progress" ? (
                      <HalfCircleIcon />
                    ) : (
                      <CircleIcon />
                    )}
                  </span>
                  <span
                    className={cn(
                      "text-[12.5px] leading-[1.45]",
                      status === "completed" && "text-faint line-through",
                      status === "in_progress" && "lifted",
                      status === "pending" && "text-soft",
                    )}
                  >
                    {task.label}
                  </span>
                </li>
              );
            })}
          </ul>
        </>
      ) : (
        // The height is reserved rather than measured, so the cloth below it does
        // not move when the plan arrives. `6.75rem` is the objective plus three rows
        // at this type size.
        <p className="text-soft mt-3 min-h-[6.75rem] text-[13px] leading-[1.55]">
          The plan appears here the moment a turn is sent, and checks itself off
          beside the reply.
        </p>
      )}
    </div>
  );
}

/* Glyphs on a 24-unit grid at the app's 1.7 stroke. */

function glyph(size: number) {
  return {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.7,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };
}

function CheckIcon({ size = 12 }: { size?: number }) {
  return (
    <svg {...glyph(size)} strokeWidth={2.2}>
      <path d="M5 12.5l4.5 4.5L19 7" />
    </svg>
  );
}

/** The app's in-progress mark: a circle filled on one side only. */
function HalfCircleIcon({ size = 12 }: { size?: number }) {
  return (
    <svg {...glyph(size)}>
      <circle cx="12" cy="12" r="7.2" />
      <path d="M12 4.8a7.2 7.2 0 0 1 0 14.4z" fill="currentColor" stroke="none" />
    </svg>
  );
}

function CircleIcon({ size = 12 }: { size?: number }) {
  return (
    <svg {...glyph(size)}>
      <circle cx="12" cy="12" r="7.2" />
    </svg>
  );
}
