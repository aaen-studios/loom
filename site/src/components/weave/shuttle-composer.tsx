"use client";

import { cn } from "@/lib/cn";

/**
 * The composer, as the shuttle.
 *
 * In weaving the shuttle is what carries the weft across the warp — it is the
 * thing that makes each pass happen. In the app it is the composer, and that is
 * not a metaphor stretched to fit: it is the object you use to send the next pick,
 * and while a turn runs it changes shape from Send to Stop the way a shuttle
 * changes direction at the end of a pass.
 *
 * The iridescent thread along the edge belongs to the app's quick-ask overlay
 * (`Ctrl+Shift+Space`), not the main composer, so it is not reproduced here —
 * advertising a detail in a place a visitor will never meet it is worse than
 * leaving it out. What is carried over is the shape: a capsule, a 15px line, a
 * footer row of chips, and a 36px circular control filled with the control ink.
 *
 * ---------------------------------------------------------------------------
 * Why none of these are real controls
 * ---------------------------------------------------------------------------
 *
 * This is scenery: there is no application on this page, so a `textarea` here
 * would take focus and then discard every keystroke, and a `button` would invite a
 * press and do nothing. Both are worse than a picture of a window, because they
 * lie about being interactive. The chips and the send control are therefore plain
 * elements — no `tabindex`, no `role="button"` — and only the placeholder text
 * carries over, which is what the page's verifier asserts.
 */
export function ShuttleComposer({
  value,
  typing,
  busy,
  hero = false,
}: {
  value: string;
  typing: boolean;
  busy: boolean;
  /** The empty-chat variant carries `animate-fade-up`, as the app's does. */
  hero?: boolean;
}) {
  return (
    <div
      className={cn(
        "panel-strong rounded-sheet relative w-full p-2.5",
        hero && "animate-fade-up",
      )}
    >
      <div className="block min-h-[44px] w-full px-3 pt-2 text-[15px] leading-6">
        {value ? (
          <span className="text-[var(--ink)] whitespace-pre-wrap">
            {value}
            {typing && (
              <span className="cursor-blink bg-[var(--ink-soft)] ml-0.5 inline-block h-[15px] w-[7px] translate-y-[3px] rounded-[2px]" />
            )}
          </span>
        ) : (
          <span className="text-[var(--ink-faint)]">
            {busy ? "Queue a message…" : "Do anything…"}
          </span>
        )}
      </div>

      <div className="mt-1.5 flex items-center gap-1.5 pl-0.5">
        <Capsule icon={<ModelIcon />} label="gpt-5.2" />
        <Capsule icon={<ModeIcon />} label="Build · Auto all" />

        <div className="flex-1" />

        <span
          className="text-faint grid h-8 w-8 place-items-center rounded-full"
          aria-hidden="true"
        >
          <PaperclipIcon />
        </span>

        {busy ? (
          <span
            className="grid h-9 w-9 place-items-center rounded-full border border-[var(--glass-border)] text-[var(--ink)]"
            aria-hidden="true"
          >
            <StopIcon />
          </span>
        ) : (
          <span
            className={cn(
              "grid h-9 w-9 place-items-center rounded-full",
              "bg-[var(--control-bg)] text-[var(--control-ink)]",
              // Dimmed while the box is empty, which is how the app draws a
              // disabled Send. The moment text arrives it fills in.
              !value && "opacity-40",
            )}
            aria-hidden="true"
          >
            <ArrowUpIcon />
          </span>
        )}
      </div>
    </div>
  );
}

function Capsule({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <span className="text-soft flex h-8 items-center gap-1.5 rounded-full px-2.5 text-[12.5px]">
      {icon}
      {label}
    </span>
  );
}

/* Glyphs on a 24-unit grid at the app's sizes. */

function glyph(size: number) {
  return {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };
}

function ArrowUpIcon() {
  return (
    <svg {...glyph(17)}>
      <path d="M12 19V5M6 11l6-6 6 6" />
    </svg>
  );
}

function StopIcon() {
  return (
    <svg {...glyph(15)}>
      <rect x="7" y="7" width="10" height="10" rx="2" fill="currentColor" stroke="none" />
    </svg>
  );
}

function PaperclipIcon() {
  return (
    <svg {...glyph(16)}>
      <path d="M17.5 8.5l-7.4 7.4a3 3 0 0 1-4.2-4.2l8-8a4.5 4.5 0 0 1 6.4 6.4l-8 8a6 6 0 0 1-8.5-8.5" />
    </svg>
  );
}

function ModelIcon() {
  return (
    <svg {...glyph(14)}>
      <path d="M12 3.5l8 4.4v8.2l-8 4.4-8-4.4V7.9z" />
      <path d="M12 12l8-4.1M12 12v8.5M12 12L4 7.9" />
    </svg>
  );
}

function ModeIcon() {
  return (
    <svg {...glyph(14)}>
      <path d="M4 7h10M18 7h2M4 17h2M10 17h10" />
      <circle cx="16" cy="7" r="2.2" />
      <circle cx="8" cy="17" r="2.2" />
    </svg>
  );
}
