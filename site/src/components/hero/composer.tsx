"use client";

import { cn } from "@/lib/cn";

/**
 * The composer, matching the app's `Composer.tsx` measurement for measurement.
 *
 * The wrapper is always `panel-strong relative w-full rounded-sheet p-2.5`,
 * whether the chat is empty or mid-conversation — the app only *moves* it
 * between the two layouts, it does not restyle it. That is why this component
 * has no layout opinions of its own: the caller decides where it sits.
 *
 * The controls are the app's too: a 15px textarea on `leading-6`, a footer row
 * with the model and mode chips at the left, attach and send at the right, and
 * a 36px circular send button filled with the control ink. While a turn runs
 * the send button is replaced by a bordered Stop button.
 *
 * One deliberate omission: the app's composer also carries the iridescent
 * "thread" that sweeps its edge while a turn runs, but that belongs to the
 * quick-ask overlay (`Ctrl+Shift+Space`), not the main window. Using it here
 * would advertise a detail in a place a user will never see it, so the hero
 * animates the window's real signals instead — the Stop button and the badge.
 */
export function HeroComposer({
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
        <Chip icon={<ModelIcon />} label="gpt-5.2" />
        <Chip icon={<ModeIcon />} label="Build · Auto all" />

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
              // Dimmed while the composer is empty, which is how the app
              // disables Send. The moment text arrives it fills in.
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

/**
 * A composer chip. The app's `ModelPicker` and `ModeChip` are both capsule
 * buttons with a 14—16px glyph and 12.5px text, so they are one component here.
 */
function Chip({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <span className="text-soft flex h-8 items-center gap-1.5 rounded-full px-2.5 text-[12.5px]">
      {icon}
      {label}
    </span>
  );
}

/* ---------------------------------------------------------------------------
   Glyphs, drawn at the app's sizes.
--------------------------------------------------------------------------- */

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
      <rect
        x="7"
        y="7"
        width="10"
        height="10"
        rx="2"
        fill="currentColor"
        stroke="none"
      />
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
