/**
 * The scripted turn the hero replays, as pure data.
 *
 * This module deliberately has no React in it. The scene is derived from one
 * number — elapsed milliseconds — and everything else is a function of that
 * number, which makes the whole animation testable without a DOM, a fake timer,
 * or a renderer. `timeline.test.ts` asserts the ordering and the invariants that
 * matter (the composer empties before the reply starts, the task list never
 * jumps backwards, every beat is reachable inside one loop).
 *
 * The React side (`use-scene.ts`) is then reduced to a clock and a ref.
 */

/**
 * The beat sheet. Every value is a millisecond offset from the start of a loop,
 * and they must stay in ascending order — a test enforces it, because a
 * hand-edited beat list is exactly the kind of thing that silently rots.
 */
export const BEATS = {
  /** The prompt starts typing itself in. */
  typingStart: 700,
  /** The turn is sent: the layout crossfades and the empty screen goes. */
  sentAt: 2900,
  /** The reasoning panel appears and shimmers. */
  thinkingAt: 3250,
  /** Thinking settles; the tool row starts running. */
  toolStartAt: 5400,
  /** The tool row reports its result. */
  toolDoneAt: 6700,
  /** The reply begins streaming. */
  replyStartAt: 6950,
  /** The reply finishes. */
  replyEndAt: 11400,
  /** Everything settles: the turn ends and Stop reverts to Send. */
  settleAt: 11700,
  /** Long enough to read the finished state before the loop restarts. */
  loopAt: 20000,
} as const;

export type Beat = keyof typeof BEATS;

/** What the visitor types. A real task, phrased the way a person would. */
export const PROMPT =
  "Refactor the background presets into their own module, and cover the fallback with a test.";

/**
 * What Loom answers.
 *
 * Written to be representative rather than aspirational: it names a real file,
 * gives a real plan, and the snippet in the middle is a simplified version of a
 * helper that genuinely exists in this repository — so the code block on the
 * page is real code rather than decorative pseudo-code.
 *
 * It is also deliberately *unfinished*: there is no summary paragraph and no
 * sign-off, because a streamed reply that stops mid-thought is what a real turn
 * looks like while it is still running, and the task list still has an item in
 * progress.
 *
 * The fenced block matters for the same reason. A real reply contains code, and
 * a streaming one contains *half* a code block for as long as it takes to send
 * the closing fence — which is the state the app marks `data-incomplete` so its
 * copy button dims. A reply with no fence at all would never exercise any of
 * that.
 */
export const REPLY = [
  "Four call sites, and `src/lib/background.ts` builds gradient strings by hand in two of them. That is the part worth fixing — extracting the list is mechanical, but the duplicated helper is what will drift.",
  "Move the presets into `src/lib/presets.ts` and export the shared helper from there:",
  [
    "```ts",
    "export function weave(period: number, angle: number) {",
    "  return [",
    "    `repeating-linear-gradient(${angle}deg, #fff 0 1px, transparent 1px ${period}px)`,",
    "    `repeating-linear-gradient(${angle + 90}deg, #0001 0 1px, transparent 1px ${period + 2}px)`,",
    "  ].join(\", \");",
    "}",
    "```",
  ].join("\n"),
  "Then `backgroundStyle()` reads the module, so the canvas and the settings grid cannot disagree about what a preset is.",
].join("\n\n");

/** The reasoning panel's text, shorter than the reply, as thinking usually is. */
export const REASONING =
  "Four call sites, two of which rebuild the gradient strings by hand. Extracting the module is the easy part. The risk is the fallback in `presetById`, which looks like an accident and is load-bearing — check who depends on it before touching anything.";

/** The task list the model keeps checked off, as the goal panel renders it. */
export const TASKS = [
  { id: "t1", label: "Extract the presets into src/lib/presets.ts" },
  { id: "t2", label: "Export the shared weave() helper" },
  { id: "t3", label: "Cover presetById's fallback with a test" },
] as const;

/** The chat's objective, shown above the task list. */
export const GOAL = "Extract the presets and protect the fallback";

/** Roughly how many characters a streamed reply advances per millisecond. */
const REPLY_RATE = REPLY.length / (BEATS.replyEndAt - BEATS.replyStartAt);

/**
 * The token counts the turn reports. Realistic for this conversation: the
 * prompt, the folder's context and the system prompt are the input; the reply
 * is the output.
 *
 * The two surfaces that show these use *different* formatters, copied from the
 * app — the header rounds to whole thousands ("2k") and the message footer
 * keeps one decimal ("2.2k"). Reproducing both is the point: it is exactly the
 * kind of inconsistency that proves the hero was built from the real code
 * rather than from an idea of it.
 */
const USAGE_INPUT = 2_180;
const USAGE_OUTPUT = 412;

/**
 * `2.2k in · 412 out` — the app's `formatUsage` from `messageExtra.ts`.
 *
 * Returns an empty string for no usage at all, which is how the app signals
 * that the message footer has nothing to render.
 */
export function formatUsage(inputTokens: number | null, outputTokens: number | null): string {
  const compact = (value: number | null) => {
    if (value === null) return null;
    return value >= 1000 ? `${Math.round(value / 100) / 10}k` : String(value);
  };

  const input = compact(inputTokens);
  const output = compact(outputTokens);
  if (input && output) return `${input} in · ${output} out`;
  if (input) return `${input} in`;
  if (output) return `${output} out`;
  return "";
}

/** Characters per millisecond while the prompt is being typed. */
const PROMPT_RATE = 1 / 24;

export type TaskStatus = "pending" | "in_progress" | "completed";

export type Phase =
  | "empty"
  | "typing"
  | "thinking"
  | "tools"
  | "replying"
  | "settled";

export type ToolState = "idle" | "running" | "done";

/** Everything the window renders, as plain data. */
export interface SceneFrame {
  /** Milliseconds into the loop. */
  elapsed: number;
  phase: Phase;
  /** The prompt text at this instant. */
  prompt: string;
  /** True while the caret should blink in the composer. */
  typing: boolean;
  /** Whether the turn has been sent. */
  sent: boolean;
  /** The reply text streamed so far. */
  replyText: string;
  /** The reasoning text shown so far. */
  reasoningText: string;
  /** Whether the reasoning is still arriving (drives the shimmer). */
  thinking: boolean;
  tool: ToolState;
  taskStatus: readonly TaskStatus[];
  tasksDone: number;
  /** `2k in · 412 out` — the chat header's totals, rounded to whole thousands. */
  tokenSummary: string;
  /**
   * `2.2k in · 412 out` — the usage line under the reply, one decimal place.
   *
   * Empty until the turn *finishes*, because the app's `MessageRow` renders it
   * only when `!streaming`. A running turn has no final token count to report,
   * so showing one would be inventing a number.
   */
  usageText: string;
}

/**
 * The app's greeting, verbatim from `ChatCanvas.tsx`.
 *
 * Called with an hour from the caller rather than reading the clock here, so it
 * is a pure function and can be tested at every boundary. The hero passes the
 * local hour only after mount, because the server has no idea what time it is
 * where the visitor is — and a wrong greeting ("Good evening" at 9am) is far
 * more noticeable than one that appears a moment late.
 */
export function greetingFor(hour: number): string {
  if (hour < 0 || hour > 23 || !Number.isInteger(hour)) {
    throw new RangeError(`hour must be an integer 0–23, got ${hour}`);
  }
  if (hour < 5) return "Still up?";
  if (hour < 12) return "Good morning";
  if (hour < 18) return "Good afternoon";
  return "Good evening";
}

/**
 * Marks the state each task has reached at a given moment.
 *
 * The sequence only ever moves forward, and the last task stays in progress at
 * the end of the turn. That is deliberate: a turn that finishes with work still
 * queued is what a real plan looks like at the end of a first pass, and it is
 * the honest note to finish on — the agent is not pretending to be done.
 */
export function taskStatusAt(elapsed: number): readonly TaskStatus[] {
  if (elapsed < BEATS.toolDoneAt) return ["in_progress", "pending", "pending"];
  if (elapsed < BEATS.settleAt) return ["completed", "in_progress", "pending"];
  // Straight from the second task to the third, with no moment where nothing is
  // in progress. An intermediate `[done, done, pending]` state reads as a
  // stalled panel — the "now" marker disappears and the list looks finished
  // while the turn is still running. The app never has that gap, because
  // `todo_write` replaces the whole list at once, so neither does this.
  return ["completed", "completed", "in_progress"];
}

/** The phase the window is in, derived from the beats. */
export function phaseAt(elapsed: number): Phase {
  if (elapsed < BEATS.typingStart) return "empty";
  if (elapsed < BEATS.sentAt) return "typing";
  if (elapsed < BEATS.toolStartAt) return "thinking";
  if (elapsed < BEATS.replyStartAt) return "tools";
  if (elapsed < BEATS.settleAt) return "replying";
  return "settled";
}

export function toolStateAt(elapsed: number): ToolState {
  if (elapsed < BEATS.toolStartAt) return "idle";
  if (elapsed < BEATS.toolDoneAt) return "running";
  return "done";
}

/**
 * `128000` → `128k`, `1048576` → `1M`. Lifted from the app's `format.ts`: the
 * header's numbers are meaningless without the same rounding, so this is a copy
 * rather than a reinvention.
 */
export function compactTokens(tokens: number): string {
  if (!tokens) return "0";
  if (tokens >= 1_000_000) {
    const millions = Math.round((tokens / 1_000_000) * 10) / 10;
    return `${Number.isInteger(millions) ? millions : millions.toFixed(1)}M`;
  }
  if (tokens >= 1_000) return `${Math.round(tokens / 1_000)}k`;
  return String(tokens);
}

/**
 * The whole scene at one instant.
 *
 * Pure: same `elapsed` in, same frame out, with no clock and no DOM. That is
 * what lets the tests nail down the ordering the animation depends on, and it
 * is why the React hook is only a clock.
 */
export function sceneAt(elapsed: number): SceneFrame {
  const phase = phaseAt(elapsed);
  const sent = elapsed >= BEATS.sentAt;

  // The prompt stops the instant it is sent, and is complete by then: the send
  // beat has to be reachable inside the typing beat, which a test asserts.
  const promptChars = sent
    ? PROMPT.length
    : Math.min(
        PROMPT.length,
        Math.floor(Math.max(0, elapsed - BEATS.typingStart) * PROMPT_RATE),
      );

  // Reasoning runs from its own beat until the tool starts, then stops — the
  // panel collapses before it would have finished saying everything.
  const reasoningElapsed = elapsed - BEATS.thinkingAt;
  const reasoningSpan = BEATS.toolStartAt - BEATS.thinkingAt;
  const reasoningText =
    reasoningElapsed <= 0
      ? ""
      : REASONING.slice(
          0,
          Math.floor(Math.min(1, reasoningElapsed / reasoningSpan) * REASONING.length),
        );

  const replyElapsed = elapsed - BEATS.replyStartAt;
  const replyChars =
    replyElapsed <= 0
      ? 0
      : Math.min(REPLY.length, Math.floor(replyElapsed * REPLY_RATE));
  const replyText = REPLY.slice(0, replyChars);

  const taskStatus = taskStatusAt(elapsed);

  // The header's totals climb with the reply, so it is never claiming tokens for
  // text that has not arrived. The output figure is derived from what has
  // actually streamed, scaled to end near the real count.
  const streamed = Math.round((replyChars / REPLY.length) * USAGE_OUTPUT);
  const finish = elapsed >= BEATS.settleAt;

  return {
    elapsed,
    phase,
    prompt: PROMPT.slice(0, promptChars),
    typing: !sent && promptChars > 0 && promptChars < PROMPT.length,
    sent,
    replyText,
    reasoningText,
    thinking: phase === "thinking",
    tool: toolStateAt(elapsed),
    taskStatus,
    tasksDone: taskStatus.filter((status) => status === "completed").length,
    tokenSummary: `${compactTokens(USAGE_INPUT)} in · ${compactTokens(streamed)} out`,
    // Only once the turn is over: the app renders this when `!streaming`, and a
    // running turn has no final count to report.
    usageText: finish ? formatUsage(USAGE_INPUT, USAGE_OUTPUT) : "",
  };
}

/**
 * The reasoning panel's collapsed preview.
 *
 * Verbatim from `ChatCanvas.tsx`: the text is split into lines, each stripped of
 * leading markdown bullets and repeated spaces, and the panel shows the **last**
 * line while streaming and the **first** line once the spell is over. That
 * asymmetry matters — mid-stream the newest thought is at the bottom, and once
 * the spell ends the opening sentence is the useful summary.
 *
 * A one-line preview is then clipped by CSS (`whitespace-nowrap` plus a mask),
 * so this returns the whole line rather than truncating it here.
 */
export function reasoningPreview(text: string, streaming: boolean): string {
  const lines = text
    .split("\n")
    .map((line) =>
      line
        .replace(/^[#>*\-\s]+/, "")
        .replace(/\s+/g, " ")
        .trim(),
    )
    .filter(Boolean);

  if (lines.length === 0) return "";
  return lines[streaming ? lines.length - 1 : 0];
}

/**
 * The reasoning header's label.
 *
 * The repo's `ChatCanvas.tsx` renders the fixed word "Thinking" in both states;
 * the app as the user runs it shows a duration once the spell is over. This
 * takes the live word from the source and the settled form from the running app,
 * with the duration computed from the beat sheet rather than invented — the
 * scripted spell is 2.15s, so it reports 2s.
 */
export function thinkingLabel(streaming: boolean): string {
  if (streaming) return "Thinking";
  const seconds = Math.round((BEATS.toolStartAt - BEATS.thinkingAt) / 1000);
  return `Thought for ${seconds}s`;
}

/**
 * The settled frame: the turn finished, with the last task still in progress.
 *
 * This is what a visitor who prefers reduced motion sees. It is the completed
 * content with none of the theatre — not a slower reveal, no reveal at all —
 * because the animation is decoration and the preference is asking to skip it.
 */
export function settledScene(): SceneFrame {
  return sceneAt(BEATS.settleAt + 500);
}

/**
 * A cheap fingerprint of everything the window actually renders.
 *
 * This exists to keep the animation off React's critical path. The clock ticks
 * on every animation frame, but the *output* only changes when a character
 * advances or a phase flips — which is maybe twenty times a second while text is
 * streaming, and not at all once the turn settles. Comparing this string lets
 * the React side skip a re-render for every frame in between, so a 60fps clock
 * drives roughly 20 renders rather than 60.
 *
 * It must cover every field any component reads, or a change would be dropped
 * and the window would freeze. `elapsed` and `playing` are deliberately absent:
 * nothing renders from them, and including `elapsed` would defeat the whole
 * point by making every frame unique. A test asserts that a change to any
 * rendered field changes the signature.
 */
export function frameSignature(frame: SceneFrame): string {
  return [
    frame.phase,
    frame.prompt.length,
    frame.replyText.length,
    frame.reasoningText.length,
    frame.thinking ? "t" : "-",
    frame.tool,
    frame.taskStatus.join(","),
    frame.typing ? "caret" : "-",
    // The header and footer both render from these; omitting either would leave
    // the numbers frozen while the reply streamed past them.
    frame.tokenSummary,
    frame.usageText,
  ].join("|");
}


