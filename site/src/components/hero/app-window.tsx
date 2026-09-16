"use client";

import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/cn";
import { LoomMark } from "@/components/loom-mark";
import { parseInline } from "@/lib/markdown";
import { BEATS, reasoningPreview, thinkingLabel } from "@/lib/timeline";
import { Caret, Prose } from "./prose";
import type { Scene } from "./use-scene";

/**
 * The app's window, rebuilt in real DOM from the app's own tokens.
 *
 * Not a screenshot and not a video. Every measurement is the app's, read from
 * `TitleBar.tsx` and `ChatCanvas.tsx`: a 56px chrome bar, a 44px chat header
 * with a hairline under it, a `max-w-4xl` frame holding a `max-w-3xl` transcript
 * column inset by 32px, and the app's `panel` glass for the frame itself. The
 * point is that at any device pixel ratio this stays crisp, repaints correctly in
 * both palettes, and follows the app's glass when a token changes — none of which
 * a screenshot can do.
 *
 * ---------------------------------------------------------------------------
 * One layout, not two, and the composer is outside the frame
 *
 * In the app the composer sits *inside* the window and moves: centred under the
 * greeting in an empty chat, then docked at the bottom edge once there are
 * messages. An earlier version of this hero reproduced both layouts and
 * crossfaded between them — more faithful to the product, and worse as a hero:
 * two nested glass surfaces inside a window that also changed height read as
 * busy, and the movement pulled attention off the reply.
 *
 * So the window holds the transcript, and the composer sits below the frame. The
 * frame keeps a fixed height so it never resizes mid-turn, and what you are meant
 * to watch — reasoning, a tool call, the streaming answer — is what is on screen.
 * ---------------------------------------------------------------------------
 */
export function AppWindow({ scene }: { scene: Scene }) {
  return (
    <div className="panel rounded-window mx-auto flex w-full max-w-4xl flex-col overflow-hidden">
      <TitleBar scene={scene} />

      {/* The chat header only exists once there are messages — the app's empty
          state returns before it is rendered. Putting it in both states, which
          an earlier version of this did, shows a chat title and token totals for
          a conversation that has not started. */}
      {scene.sent && <ChatHeader scene={scene} />}

      <div className="flex h-[380px] flex-col sm:h-[440px]">
        {scene.sent ? <Transcript scene={scene} /> : <EmptyState scene={scene} />}
      </div>
    </div>
  );
}

/* ---------------------------------------------------------------------------
   Chrome

   Two floating pills — navigation left, window controls right. Icon-only, which
   is what the app's title bar is: the current workspace and persona live behind
   the buttons rather than inside them, so the bar stays 56px tall and says
   nothing twice.
--------------------------------------------------------------------------- */

function TitleBar({ scene }: { scene: Scene }) {
  // The app badges running tasks *plus* running shell commands, and a turn that
  // calls a tool raises that number. One is accurate for this scene.
  const runs = scene.tool === "idle" ? 0 : 1;
  // The Chats pill's dot marks chats that are *currently* running, not chats that
  // have ever been sent — the app keys it on `busyCount > 0`. A dot that stayed
  // lit for the rest of the loop would be telling the wrong story.
  const busy = scene.tool === "running";

  return (
    // The app's title bar carries its `.chrome` class — `user-select: none;
    // cursor: default` — which lives in the app's base styles, outside the shared
    // token regions, so the two declarations are repeated here. Without them the
    // demo's chrome is selectable text, which is the fastest way to tell that
    // this is a page rather than a window.
    <header className="relative z-20 flex h-14 shrink-0 cursor-default items-center justify-between px-3 select-none">
      {/* The drag strip sits behind the pills, so the space between them is
          draggable in the real app while the pills stay interactive. Invisible,
          and the thing whose absence makes a fake window feel like a picture of
          one. */}
      <div className="absolute inset-0 -z-10" data-tauri-drag-region />

      <div className="flex items-center gap-2">
        <div className="pill rounded-capsule flex h-10 items-center gap-0.5 p-1">
          <PillButton label="Chats">
            <span className="relative">
              <PanelLeftIcon size={17} />
              {busy && (
                <span className="bg-[var(--accent)] absolute -top-0.5 -right-0.5 h-1.5 w-1.5 rounded-full" />
              )}
            </span>
          </PillButton>
          <PillButton label="New chat">
            <PlusIcon size={17} />
          </PillButton>
        </div>
        <div className="pill rounded-capsule flex h-10 items-center gap-0.5 p-1">
          <PillButton label="Runs">
            <span className="relative">
              <RunsIcon size={17} />
              {runs > 0 && (
                <span className="bg-[var(--accent)] absolute -top-1 -right-1.5 grid h-3.5 min-w-3.5 place-items-center rounded-full px-0.5 text-[9px] font-semibold text-white">
                  {runs}
                </span>
              )}
            </span>
          </PillButton>
        </div>
      </div>

      <div className="flex items-center gap-2">
        <div className="pill rounded-capsule flex h-10 items-center gap-0.5 p-1">
          <PillButton label="Workspace">
            <FolderIcon size={16} />
          </PillButton>
          <PillButton label="Persona">
            <PersonIcon size={16} />
          </PillButton>
        </div>
        {/* Hidden on the narrowest screens: at that width the chrome would crowd
            the transcript, and the window controls are the least informative part
            of the frame. */}
        <div className="pill rounded-capsule hidden h-10 items-center gap-0.5 p-1 sm:flex">
          <PillButton label="Minimize">
            <MinimizeIcon size={16} />
          </PillButton>
          <PillButton label="Maximize">
            <MaximizeIcon size={14} />
          </PillButton>
          <PillButton label="Close" danger>
            <CloseIcon size={16} />
          </PillButton>
        </div>
      </div>
    </header>
  );
}

/**
 * The chat header: the session title, the token totals, and the provider usage
 * badge. `h-11 shrink-0`, a hairline underneath, `px-5`.
 *
 * The totals are hidden until there is usage, which is why the timeline reports
 * them from the first frame of the reply rather than from the start — the app
 * guards the span with `totals.input > 0 || totals.output > 0`.
 *
 * Two formatters appear in this window and they genuinely disagree: this header
 * rounds to whole thousands ("2k in") while the message footer keeps a decimal
 * ("2.2k in"). Both are reproduced rather than harmonised, because tidying them
 * into one would make the hero a slightly nicer thing than the product.
 */
function ChatHeader({ scene }: { scene: Scene }) {
  return (
    <div className="flex h-11 shrink-0 items-center gap-3 border-b border-[var(--glass-border)] px-5">
      <span className="text-soft min-w-0 truncate text-[13px]">
        Refactor the background presets
      </span>
      <span
        className="text-faint shrink-0 text-[11.5px]"
        title="Tokens used in this chat"
      >
        {scene.tokenSummary}
      </span>
      <div className="flex-1" />
      {/* `align="down"` in the app, because the badge sits at the top edge and
          its menu has to open downward. Rendered closed: the popover is a large
          piece of UI that would distract from the reply. */}
      <UsageBadge />
    </div>
  );
}

/**
 * The provider usage badge.
 *
 * Read off `UsageBadge.tsx`: a bordered capsule with a 1.5px status dot and a
 * label from `metricBadge` — `61%` for a subscription window, `$74.50` for
 * remaining credit. The border and text carry the tone.
 *
 * The detail worth keeping is what it does *not* do: the app renders nothing at
 * all unless the active provider exposes a usage endpoint. It is not a context
 * window indicator, so the "128k"-style chip it is often mistaken for would be
 * an invention. "61%" is the app's own test fixture, and `metricTone` returns
 * `ok` for it, which is the neutral glass tone rather than a warning colour.
 */
function UsageBadge() {
  return (
    <span
      title="opencode-go · Weekly: 61% left"
      className="border-[var(--glass-border)] text-soft flex shrink-0 items-center gap-1.5 rounded-full border px-2.5 py-1 text-[12.5px]"
    >
      <span className="h-1.5 w-1.5 rounded-full bg-current" aria-hidden="true" />
      61%
    </span>
  );
}

/* ---------------------------------------------------------------------------
   Empty chat

   The screen the app opens to on every launch. The mark weaves itself in, then
   the greeting and the subline follow on the app's own stagger — 180ms and
   260ms, the numbers from `ChatCanvas.tsx`, so the site and the product open the
   same way.
--------------------------------------------------------------------------- */

function EmptyState({ scene }: { scene: Scene }) {
  return (
    <section className="flex min-w-0 flex-1 items-center justify-center px-6 pb-16">
      <div className="w-full max-w-2xl -translate-y-6">
        <div className="mb-6 flex flex-col items-center text-center">
          <LoomMark
            size={32}
            weaving
            className="text-[var(--accent)] mb-3 inline-block"
          />
          {/* An `h2`, not the app's `h1`. The app's greeting is the only heading
              on its screen; here it sits inside a landing page that already has
              an `h1`, and a second one would break the document outline for
              anyone navigating by heading. */}
          <h2
            className="intro-step text-[24px] font-medium tracking-tight text-[var(--ink)]"
            style={{ animationDelay: "180ms" }}
          >
            {/* The app greets by the hour. Rendered only after mount, because the
                server has no idea what time it is where the visitor is — and a
                wrong greeting is more noticeable than a late one. The mark above
                weaves over 640ms, so this lands while the threads are still
                drawing. */}
            {scene.greeting ?? "\u00a0"}
          </h2>
          <p
            className="intro-step text-faint mt-1.5 max-w-lg text-[13px] leading-5"
            style={{ animationDelay: "260ms" }}
          >
            {/* The app shows the workspace here when the chat has one, and the
                generic line when it does not. This scene's tool call reads a
                file from the repository, so there genuinely is one — naming it
                is both more accurate and more specific than the fallback. */}
            Working in <span className="text-soft font-medium">{WORKSPACE}</span>
          </p>
        </div>
      </div>
    </section>
  );
}

/* ---------------------------------------------------------------------------
   The conversation
--------------------------------------------------------------------------- */

/**
 * The transcript, and the follow behaviour.
 *
 * The app tracks whether the view is *pinned* to the bottom rather than jumping
 * unconditionally: `distance = scrollHeight - scrollTop - clientHeight`, and
 * anything under 90px counts as pinned. Scrolling up unpins it, the follow stops,
 * and a "Jump to latest" button appears. Without that, anyone who scrolls back to
 * reread a message gets yanked forward again on the next token.
 *
 * ---------------------------------------------------------------------------
 * The one adaptation the app does not need
 *
 * The app scrolls with `endRef.current.scrollIntoView(...)`. `scrollIntoView`
 * walks up and scrolls **every** scrollable ancestor, which on a desktop window
 * means only the transcript — but on a page it also scrolls the document, so the
 * visitor's own scroll position would be hijacked by the animation. Setting
 * `scrollTop` on the container has the same effect inside the window and touches
 * nothing outside it.
 *
 * The app also has an `alwaysFollow` setting that forces the follow regardless.
 * Not reproduced: with it on, the jump button could never appear, and the button
 * demonstrating *that the follow is interruptible* is the more useful thing to
 * show.
 * ---------------------------------------------------------------------------
 */
function Transcript({ scene }: { scene: Scene }) {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const [pinned, setPinned] = useState(true);

  const onScroll = () => {
    const element = scrollRef.current;
    if (!element) return;
    const distance =
      element.scrollHeight - element.scrollTop - element.clientHeight;
    setPinned(distance < 90);
  };

  // Follow new output.
  //
  // `scene.elapsed` is the right dependency *because* of how the clock commits:
  // a frame is only published when its rendered output changes, so `elapsed`
  // advances exactly when the transcript grows rather than once per animation
  // frame. Depending on a raw clock would scroll sixty times a second — and
  // before this existed at all, the reply streamed out of sight below the fold.
  useEffect(() => {
    if (!pinned) return;
    const element = scrollRef.current;
    if (!element) return;
    element.scrollTop = element.scrollHeight;
  }, [scene.elapsed, pinned]);

  const jumpToLatest = () => {
    setPinned(true);
    const element = scrollRef.current;
    if (!element) return;
    element.scrollTo({ top: element.scrollHeight, behavior: "smooth" });
  };

  const waiting = scene.phase === "thinking" || scene.phase === "tools";

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div
        ref={scrollRef}
        onScroll={onScroll}
        className="loom-scroll min-h-0 flex-1 overflow-y-auto px-5 pt-7 pb-2 sm:px-8"
      >
        <div className="mx-auto flex w-full max-w-3xl flex-col gap-6">
          {/* The user's turn: right-aligned, `panel-strong rounded-sheet`, 14.5px
              over `leading-6`, capped at 85% — the app's user-message surface
              verbatim. */}
          <div className="flex flex-col items-end">
            <div className="panel-strong rounded-sheet max-w-[85%] px-3.5 py-2.5 text-[14.5px] leading-6 whitespace-pre-wrap select-text">
              Refactor the background presets into their own module, and cover the
              fallback with a test.
            </div>
          </div>

          <div className="flex gap-3">
            <div className="text-soft mt-0.5 grid h-7 w-7 shrink-0 place-items-center rounded-full border border-[var(--glass-border)]">
              <LoomMark size={15} />
            </div>

            <div className="min-w-0 flex-1 pt-0.5">
              <Reasoning scene={scene} />
              <ToolRow scene={scene} />
              <Prose text={scene.replyText} streaming={scene.phase === "replying"} />

              {/* The app's bare caret, shown while a turn runs but before it has
                  produced any text. */}
              {waiting && <Caret />}

              {/* The message footer. Rendered only when the turn is over, which is
                  why the timeline returns an empty string while it streams — the
                  app guards this with `!streaming`, and a running turn has no
                  final token count to report. */}
              {scene.usageText && (
                <p className="text-faint mt-1.5 text-[11.5px]">{scene.usageText}</p>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* The app puts this at `bottom-28` because its composer sits inside the
          frame and the button has to clear it. Here the composer is below the
          window, so the scroll area reaches the frame's bottom edge and a smaller
          offset is correct. */}
      {!pinned && (
        <button
          type="button"
          onClick={jumpToLatest}
          className="panel-strong rounded-capsule text-soft absolute bottom-4 left-1/2 z-20 -translate-x-1/2 px-3 py-1.5 text-[12px]"
        >
          Jump to latest ↓
        </button>
      )}
    </div>
  );
}

/**
 * The reasoning panel.
 *
 * A spine, a small label, and one line of text — the app's collapsed display,
 * which is its default. The spine is the app's `loom-thinking` border and runs
 * the height of the block, which is what makes reasoning read as an aside rather
 * than as part of the answer.
 *
 * While the text is still arriving the paragraph carries `thinking-shimmer`: a
 * gradient clipped to the glyphs and swept across them, so the words look lit
 * from within rather than accompanied by a spinner. When the spell ends the
 * shimmer stops and the line is masked by `thinking-preview`, so it trails off
 * instead of ending in an ellipsis.
 */
function Reasoning({ scene }: { scene: Scene }) {
  if (scene.phase === "empty" || scene.phase === "typing") return null;

  const live = scene.thinking;
  // Live, the panel shows what has arrived; settled, it shows the opening
  // sentence. Both come from `@/lib/timeline`, where the asymmetry is tested —
  // mid-thought the newest line is at the bottom, and afterwards the first line
  // is the useful summary.
  const preview = live
    ? scene.reasoningText
    : reasoningPreview(scene.reasoningText, false);

  if (!preview) return null;

  return (
    <div className="loom-thinking mb-2.5">
      <div className="text-faint flex items-center gap-1.5 text-[12px]">
        <BrainIcon size={13} />
        <span>{thinkingLabel(live)}</span>
      </div>
      <p
        className={cn(
          "mt-1 text-[13px] leading-[1.68]",
          live ? "thinking-shimmer" : "thinking-preview",
        )}
      >
        <InlineText text={preview} />
      </p>
    </div>
  );
}

/**
 * The workspace the scripted turn works in.
 *
 * Named rather than generic so the empty state's subline and the tool row's path
 * describe the same folder — the app shows the workspace name here when the chat
 * has one, and the fallback line when it does not.
 */
const WORKSPACE = "loom";

/** The file the scripted call reads, relative to that workspace. */
const TOOL_PATH = "src/lib/background.ts";

/**
 * A tool call, as the app renders one in its default collapsed display: a quiet
 * inline line rather than a card — icon, verb, the file being read, and how long
 * it took.
 *
 * The duration is computed from the beat sheet rather than written by hand, so it
 * cannot drift when the beats are retimed.
 */
function ToolRow({ scene }: { scene: Scene }) {
  if (scene.tool === "idle") return null;
  const running = scene.tool === "running";
  const seconds = ((BEATS.toolDoneAt - BEATS.toolStartAt) / 1000).toFixed(1);

  return (
    <div className="mb-2.5 flex items-center gap-2 text-[12.5px]">
      <span className="text-faint">
        <FileIcon size={13} />
      </span>
      <span className={cn("text-faint", running && "thinking-shimmer")}>
        {running ? "Reading" : "Read"}
      </span>
      <span className="text-faint font-mono text-[11.5px]">{TOOL_PATH}</span>
      {!running && <span className="text-faint text-[11.5px]">· {seconds}s</span>}
    </div>
  );
}

/**
 * Renders `code spans` inline, tolerating an unterminated one mid-stream — the
 * same treatment the reply gets, since the app renders reasoning and replies
 * through one markdown pipeline.
 */
function InlineText({ text }: { text: string }) {
  return (
    <>
      {parseInline(text).map((part, index) =>
        part.code ? (
          <code
            key={index}
            className="bg-[var(--ink-ghost)] rounded-[6px] px-[0.4em] py-[0.12em] font-mono text-[0.9em]"
          >
            {part.text}
          </code>
        ) : (
          <span key={index}>{part.text}</span>
        ),
      )}
    </>
  );
}

/* ---------------------------------------------------------------------------
   Chrome glyphs, drawn at the app's sizes on a 24-unit grid.

   The reply's own rendering — including the half-arrived code fence — lives in
   `prose.tsx`, and its parsing in `@/lib/markdown` where it is tested.
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

function PillButton({
  label,
  danger,
  children,
}: {
  label: string;
  danger?: boolean;
  children: React.ReactNode;
}) {
  return (
    <span
      title={label}
      className={cn(
        "text-soft grid h-8 w-9 place-items-center rounded-full",
        danger && "hover:bg-[var(--danger)] hover:text-white",
      )}
    >
      {children}
    </span>
  );
}

function PanelLeftIcon({ size = 17 }: { size?: number }) {
  return (
    <svg {...glyph(size)}>
      <rect x="3" y="4" width="18" height="16" rx="3" />
      <path d="M9 4v16" />
    </svg>
  );
}

function PlusIcon({ size = 17 }: { size?: number }) {
  return (
    <svg {...glyph(size)}>
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}

function RunsIcon({ size = 17 }: { size?: number }) {
  return (
    <svg {...glyph(size)}>
      <path d="M5 6h14M5 12h10M5 18h6" />
    </svg>
  );
}

function FolderIcon({ size = 16 }: { size?: number }) {
  return (
    <svg {...glyph(size)}>
      <path d="M3 7.5A2.5 2.5 0 0 1 5.5 5h3.2l2 2.2h7.8A2.5 2.5 0 0 1 21 9.7v7.3a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17z" />
    </svg>
  );
}

function PersonIcon({ size = 16 }: { size?: number }) {
  return (
    <svg {...glyph(size)}>
      <circle cx="12" cy="8.5" r="3.5" />
      <path d="M5 20a7 7 0 0 1 14 0" />
    </svg>
  );
}

function MinimizeIcon({ size = 16 }: { size?: number }) {
  return (
    <svg {...glyph(size)}>
      <path d="M6 12h12" />
    </svg>
  );
}

function MaximizeIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...glyph(size)}>
      <rect x="5.5" y="5.5" width="13" height="13" rx="2.5" />
    </svg>
  );
}

function CloseIcon({ size = 16 }: { size?: number }) {
  return (
    <svg {...glyph(size)}>
      <path d="M6 6l12 12M18 6L6 18" />
    </svg>
  );
}

function BrainIcon({ size = 13 }: { size?: number }) {
  return (
    <svg {...glyph(size)} className="shrink-0">
      <path d="M12 5.5a3 3 0 0 0-5.7 1.3A3 3 0 0 0 5 12a3 3 0 0 0 1.6 2.6A3 3 0 0 0 12 18.5z" />
      <path d="M12 5.5a3 3 0 0 1 5.7 1.3A3 3 0 0 1 19 12a3 3 0 0 1-1.6 2.6A3 3 0 0 1 12 18.5z" />
    </svg>
  );
}

function FileIcon({ size = 13 }: { size?: number }) {
  return (
    <svg {...glyph(size)}>
      <path d="M13.5 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8.5z" />
      <path d="M13.5 3v5.5H19" />
    </svg>
  );
}
