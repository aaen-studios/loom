"use client";

import { cn } from "@/lib/cn";
import { GOAL, TASKS } from "@/lib/timeline";
import type { Scene } from "./use-scene";

/**
 * The goal and live task list that sits above the composer.
 *
 * This is the panel the model keeps checked off as a turn runs: `todo_write`
 * replaces the list, and the panel reads it back each turn alongside the
 * `/goal` objective. Watching a task go from pending to in-progress to done
 * while the reply is still streaming is the clearest single demonstration of
 * what an agent does that a chat interface does not — so it is rendered at the
 * app's real size with the app's real states, rather than summarised in a
 * screenshot.
 *
 * Three visual states, all from the app: a pending circle, a half-filled circle
 * with a "now" marker for the item in progress, and a struck-through check for
 * the finished ones.
 *
 * Returns nothing before the turn is sent, because that is what the app does:
 * an empty chat has no goal and no tasks, and `GoalPanel` there returns null
 * rather than rendering an empty box.
 */
export function GoalPanel({
  scene,
  className,
}: {
  scene: Scene;
  className?: string;
}) {
  if (!scene.sent) return null;

  return (
    <div
      className={cn(
        "panel-strong rounded-sheet animate-fade-up mb-2 w-full overflow-hidden",
        className,
      )}
    >
      <div className="flex w-full items-center gap-2 px-3 py-2">
        <span className="text-[var(--accent)] shrink-0">
          <TargetIcon />
        </span>
        <span className="text-soft min-w-0 flex-1 truncate text-[12.5px]">
          <span className="text-faint">Goal · </span>
          {GOAL}
        </span>
        <span className="text-faint rounded-capsule shrink-0 border border-[var(--glass-border)] px-1.5 py-0.5 text-[10.5px] tabular-nums">
          {scene.tasksDone}/{TASKS.length}
        </span>
        <ChevronIcon />
      </div>

      <ul className="px-1.5 pb-1.5">
        {TASKS.map((task, index) => {
          const status = scene.taskStatus[index];
          return (
            <li
              key={task.id}
              className="rounded-row flex items-start gap-2.5 px-2 py-1.5"
            >
              <span
                className={cn(
                  "mt-[2px] grid h-4 w-4 shrink-0 place-items-center",
                  status === "pending" ? "text-faint" : "text-[var(--accent)]",
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
                  "min-w-0 flex-1 text-[13px] leading-5",
                  status === "completed" && "text-faint line-through",
                  status === "in_progress" && "text-[var(--ink)]",
                  status === "pending" && "text-soft",
                )}
              >
                {task.label}
              </span>
              {status === "in_progress" && (
                <span className="text-[var(--accent)] mt-[3px] shrink-0 text-[10.5px] tracking-wide uppercase">
                  now
                </span>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

/* ---------------------------------------------------------------------------
   Glyphs, at the app's sizes on a 24-unit grid.
--------------------------------------------------------------------------- */

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

function TargetIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...glyph(size)}>
      <circle cx="12" cy="12" r="8.5" />
      <circle cx="12" cy="12" r="3.5" />
    </svg>
  );
}

function ChevronIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...glyph(size)} className="text-faint shrink-0">
      <path d="M6 9l6 6 6-6" />
    </svg>
  );
}

function CheckIcon({ size = 13 }: { size?: number }) {
  return (
    <svg {...glyph(size)} strokeWidth={2.2}>
      <path d="M5 12.5l4.5 4.5L19 7" />
    </svg>
  );
}

/** The app's in-progress mark: a circle filled on one side. */
function HalfCircleIcon({ size = 13 }: { size?: number }) {
  return (
    <svg {...glyph(size)}>
      <circle cx="12" cy="12" r="7.2" />
      <path
        d="M12 4.8a7.2 7.2 0 0 1 0 14.4z"
        fill="currentColor"
        stroke="none"
      />
    </svg>
  );
}

function CircleIcon({ size = 13 }: { size?: number }) {
  return (
    <svg {...glyph(size)}>
      <circle cx="12" cy="12" r="7.2" />
    </svg>
  );
}
