/**
 * The scripted turn the landing page replays, as pure data.
 *
 * There is deliberately no React in this module. The whole demonstration is a
 * function of one number — milliseconds since the loop started — so every
 * invariant the animation depends on can be asserted without a DOM, a timer or
 * a renderer. `scene.test.ts` covers the ordering, the monotonicity of the
 * streamed text, and the states that must be reachable inside a single loop.
 *
 * That separation is the point. The failure mode this guards against is silent:
 * a beat edited into the wrong order makes an entire phase unreachable, and
 * nothing about a successful build would say so.
 */

/**
 * The beat sheet. Values are millisecond offsets from the start of a loop and
 * must ascend — a test enforces that, because a hand-edited beat list rots.
 */
export const BEATS = {
  /** The prompt begins typing itself into the composer. */
  typingAt: 800,
  /** The turn is sent; the empty state is replaced by the transcript. */
  sentAt: 3300,
  /** The reasoning panel appears and starts shimmering. */
  thinkingAt: 3600,
  /** Reasoning settles and the tool call starts running. */
  toolAt: 5700,
  /** The tool call reports its result. */
  toolDoneAt: 6900,
  /** The reply starts streaming. */
  replyAt: 7200,
  /** The reply is complete. */
  replyDoneAt: 11100,
  /** The turn ends: Stop reverts to Send, the usage line appears. */
  settleAt: 11400,
  /** Long enough to read the finished turn before the loop restarts. */
  loopAt: 20000,
} as const;

export type Beat = keyof typeof BEATS;

/** What the visitor watches being typed. A real task, phrased plainly. */
export const PROMPT =
  "The rename left four call sites behind. Fix them, and prove the fallback still resolves.";

/**
 * What Loom answers.
 *
 * Written to be representative rather than aspirational: it names a file that
 * exists in this repository, gives a plan, and contains a fenced block — which
 * matters, because a *streaming* reply spends real time holding half a code
 * fence, and that is a state worth drawing rather than avoiding.
 *
 * It stops mid-thought on purpose. A streamed reply that ends in a tidy summary
 * is a screenshot of a finished turn; this one is still arriving, which is the
 * only moment worth animating.
 */
export const REPLY = [
  "Four call sites, and two of them rebuild the gradient strings by hand instead of calling the shared helper. The first is the rename; the second is the part that will actually drift.",
  "So the fix has two halves — repoint the callers, then make the fallback an explicit branch rather than an accident:",
  [
    "```ts",
    "export function presetById(id: string): BackgroundPreset {",
    "  return BACKGROUND_PRESETS.find((preset) => preset.id === id)",
    "    ?? BACKGROUND_PRESETS[0];",
    "}",
    "```",
  ].join("\n"),
  "The fallback is load-bearing: it is how a preset that was renamed or retired",
].join("\n\n");

/**
 * The reasoning panel's text. Shorter than the reply, as thinking usually is,
 * and worded as a judgement rather than a summary — which is what makes a
 * reasoning panel worth reading.
 */
export const REASONING =
  "Two of these call sites are copy-paste rather than renames, so fixing the import alone would leave the duplication in place. The fallback in `presetById` looks like defensive coding but something depends on it — check the tests before changing its behaviour.";

/** The task list the model maintains, rendered the way the app renders it. */
export const TASKS = [
  { id: "t1", label: "Repoint the four call sites" },
  { id: "t2", label: "Route both hand-built gradients through weave()" },
  { id: "t3", label: "Cover presetById's fallback with a test" },
] as const;

/** The objective, shown above the task list. */
export const GOAL = "Fix the rename and protect the preset fallback";

/** Characters per millisecond while the prompt is typing. */
const PROMPT_RATE = 1 / 24;

/** Characters per millisecond while the reply streams. */
const REPLY_RATE = REPLY.length / (BEATS.replyDoneAt - BEATS.replyAt);

/** Token counts the turn reports: context in, reply out. */
const USAGE_IN = 2_180;
const USAGE_OUT = 412;

export type TaskStatus = "pending" | "in_progress" | "completed";
export type ToolState = "idle" | "running" | "done";
export type Phase = "empty" | "typing" | "thinking" | "tools" | "replying" | "settled";

/** Everything the rendered window needs, as plain data. */
export interface Frame {
  elapsed: number;
  phase: Phase;
  /** The composer's text at this instant. */
  prompt: string;
  /** Whether the composer's caret should be blinking. */
  typing: boolean;
  /** Whether the turn has been sent at all. */
  sent: boolean;
  replyText: string;
  reasoningText: string;
  /** True while reasoning is still arriving — drives the shimmer. */
  thinking: boolean;
  tool: ToolState;
  taskStatus: readonly TaskStatus[];
  tasksDone: number;
  /** `2k in · 412 out` — the chat header's totals, rounded down to thousands. */
  tokenSummary: string;
  /** `2.2k in · 412 out` — the footer's usage line, one decimal. Empty until the turn settles. */
  usageText: string;
}

/**
 * The greeting, matching the app's boundaries exactly.
 *
 * Takes the hour rather than reading the clock, so it is pure and every boundary
 * is testable. The caller passes the *local* hour, and only after mount: the
 * server cannot know what time it is where the visitor is, and a greeting that is
 * confidently wrong is worse than one that arrives a moment late.
 */
export function greetingFor(hour: number): string {
  if (!Number.isInteger(hour) || hour < 0 || hour > 23) {
    throw new RangeError(`hour must be an integer 0–23, got ${hour}`);
  }
  if (hour < 5) return "Still up?";
  if (hour < 12) return "Good morning";
  if (hour < 18) return "Good afternoon";
  return "Good evening";
}

/**
 * Which state each task has reached.
 *
 * The sequence only moves forward, and the last task is still running when the
 * loop ends. That is the honest place to stop: a first pass that finishes with
 * work queued is what an agent actually looks like, and a list that ticks itself
 * all the way green would be a claim the product has not made.
 *
 * There is no frame with nothing in progress. `todo_write` replaces the whole
 * list at once, so the app never has that gap either — and a panel with no "now"
 * marker reads as stalled rather than as finished.
 */
export function taskStatusAt(elapsed: number): readonly TaskStatus[] {
  if (elapsed < BEATS.toolDoneAt) return ["in_progress", "pending", "pending"];
  if (elapsed < BEATS.settleAt) return ["completed", "in_progress", "pending"];
  return ["completed", "completed", "in_progress"];
}

export function phaseAt(elapsed: number): Phase {
  if (elapsed < BEATS.typingAt) return "empty";
  if (elapsed < BEATS.sentAt) return "typing";
  if (elapsed < BEATS.toolAt) return "thinking";
  if (elapsed < BEATS.replyAt) return "tools";
  if (elapsed < BEATS.settleAt) return "replying";
  return "settled";
}

export function toolStateAt(elapsed: number): ToolState {
  if (elapsed < BEATS.toolAt) return "idle";
  if (elapsed < BEATS.toolDoneAt) return "running";
  return "done";
}

/**
 * `128000` → `128k`, `1500000` → `1.5M`.
 *
 * Lifted from the app's `format.ts`, because a token count that rounds
 * differently here than in the product would be quietly wrong rather than
 * obviously wrong.
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
 * `2.2k in · 412 out` — the app's `formatUsage` from `messageExtra.ts`.
 *
 * Deliberately a *different* formatter from the header's (`compactTokens`
 * rounds to whole thousands, this keeps one decimal). The app genuinely renders
 * both, two inches apart, and reproducing the disagreement is the difference
 * between a hero built from the real code and one built from an idea of it.
 */
export function formatUsage(
  inputTokens: number | null,
  outputTokens: number | null,
): string {
  const compact = (value: number | null) =>
    value === null ? null : value >= 1000 ? `${Math.round(value / 100) / 10}k` : String(value);

  const input = compact(inputTokens);
  const output = compact(outputTokens);
  if (input && output) return `${input} in · ${output} out`;
  if (input) return `${input} in`;
  if (output) return `${output} out`;
  return "";
}

/**
 * The reasoning panel's one-line preview.
 *
 * Asymmetric on purpose, and this is the app's behaviour: while text is still
 * arriving the newest thought is at the bottom, so the *last* line is the useful
 * one; once it has settled the opening sentence is the summary. Getting this
 * backwards is invisible until you read it carefully.
 */
export function reasoningPreview(text: string, streaming: boolean): string {
  const lines = text
    .split("\n")
    .map((line) =>
      line
        // Strip a heading's hashes and a list bullet, so the preview reads as a
        // sentence rather than as markup with the sentence inside it.
        .replace(/^#+\s*/, "")
        .replace(/^[-*]\s+/, "")
        // Collapse each line's internal whitespace. A reasoning block is often
        // wrapped hard, and the newlines that wrap it are not part of the
        // sentence.
        .replace(/\s+/g, " ")
        .trim(),
    )
    .filter(Boolean);

  if (lines.length === 0) return "";
  return streaming ? lines[lines.length - 1] : lines[0];
}

/** `Thinking`, then `Thought for 2s` once the reasoning has stopped. */
export function thinkingLabel(live: boolean): string {
  if (live) return "Thinking";
  const seconds = Math.round((BEATS.toolAt - BEATS.thinkingAt) / 1000);
  return `Thought for ${seconds}s`;
}

/**
 * The whole frame at one instant.
 *
 * Pure: the same `elapsed` always produces the same frame, with no clock and no
 * DOM. That is what lets the test suite pin down the ordering, and it is why the
 * React side of the hero is nothing but a clock.
 */
export function sceneAt(elapsed: number): Frame {
  const phase = phaseAt(elapsed);
  const sent = elapsed >= BEATS.sentAt;

  // The prompt stops the instant it is sent, and must be complete by then — the
  // send beat has to be reachable inside the typing beat, which a test asserts.
  const promptChars = sent
    ? PROMPT.length
    : Math.min(
        PROMPT.length,
        Math.floor(Math.max(0, elapsed - BEATS.typingAt) * PROMPT_RATE),
      );

  // Reasoning runs from its own beat until the tool starts, then stops: the
  // panel collapses before it would have finished saying everything, which is
  // what thinking looks like when a model commits to an action.
  const reasoningElapsed = elapsed - BEATS.thinkingAt;
  const reasoningSpan = BEATS.toolAt - BEATS.thinkingAt;
  const reasoningText =
    reasoningElapsed <= 0
      ? ""
      : REASONING.slice(
          0,
          Math.floor(Math.min(1, reasoningElapsed / reasoningSpan) * REASONING.length),
        );

  const replyElapsed = elapsed - BEATS.replyAt;
  const replyChars =
    replyElapsed <= 0 ? 0 : Math.min(REPLY.length, Math.floor(replyElapsed * REPLY_RATE));
  const replyText = REPLY.slice(0, replyChars);

  const taskStatus = taskStatusAt(elapsed);
  const settled = elapsed >= BEATS.settleAt;

  // The header's totals climb with the reply rather than appearing at the end,
  // so they are never claiming tokens for text that has not arrived. The output
  // figure is scaled from what has actually streamed.
  const streamed = Math.round((replyChars / REPLY.length) * USAGE_OUT);

  return {
    elapsed,
    phase,
    prompt: PROMPT.slice(0, promptChars),
    typing: !sent && promptChars > 0,
    sent,
    replyText,
    reasoningText,
    // `>= 0`, not `> 0`: the instant the panel appears is the instant it should
    // already be shimmering, or the first frame of it renders settled and then
    // starts moving, which reads as a flicker.
    thinking: reasoningElapsed >= 0 && elapsed < BEATS.toolAt,
    tool: toolStateAt(elapsed),
    taskStatus,
    tasksDone: taskStatus.filter((status) => status === "completed").length,
    tokenSummary: `${compactTokens(USAGE_IN)} in · ${compactTokens(streamed)} out`,
    // The app's message footer renders usage only once the turn has stopped
    // streaming; a running turn has no final count to report.
    usageText: settled ? formatUsage(USAGE_IN, USAGE_OUT) : "",
  };
}

/**
 * The frame a reduced-motion visitor sees.
 *
 * Everything the animation would have revealed, none of the theatre: the turn is
 * over, the task list is at its end state, and the usage line is present.
 * Rendering this instead of `sceneAt(loopAt)` keeps the two from drifting apart
 * if the beat sheet changes.
 */
export function settledScene(): Frame {
  return sceneAt(BEATS.loopAt);
}

/**
 * A cheap string that changes exactly when the *rendered output* changes.
 *
 * The hero ticks on every animation frame but should not re-render on every one:
 * most frames produce byte-identical output, and the settled seconds produce
 * nothing but the same frame over and over. Comparing signatures keeps the clock
 * at 60fps while React does roughly a third of the work.
 */
export function frameSignature(frame: Frame): string {
  return [
    frame.phase,
    frame.prompt,
    frame.typing ? "1" : "0",
    frame.replyText,
    frame.reasoningText,
    frame.thinking ? "1" : "0",
    frame.tool,
    frame.taskStatus.join(""),
    frame.tokenSummary,
    frame.usageText,
  ].join("|");
}
