/**
 * Pure logic behind the voice-mode surface.
 *
 * Kept out of the component for one reason: the decisions that are easy to get
 * *quietly* wrong — how loud a level looks, how much history the meter keeps,
 * what the status line says — are decisions, and a component's render function
 * is a bad place to keep them. They are all pure functions here, so the tests
 * can state them without a DOM or a microphone.
 */

/**
 * How many level samples the meter keeps.
 *
 * About 3.6 seconds at the 100 ms block size the microphone posts. Long enough
 * that a pause is visible as a shape rather than a gap, short enough that the
 * meter responds to the current word rather than to the last sentence.
 */
export const LEVEL_HISTORY = 36;

/**
 * How much the raw RMS is amplified for display.
 *
 * Speech RMS through a microphone sits around 0.02–0.15, so drawing it on a
 * 0–1 scale shows a meter that barely moves and reads as broken. Four times is
 * the gain the composer's meter already uses, which matters: two meters on
 * screen showing the same microphone at different sensitivities would look like
 * a bug in one of them.
 */
export const LEVEL_GAIN = 4;

/** Appends a level, dropping the oldest so the history stays bounded. */
export function pushSample(
  history: readonly number[],
  level: number,
  cap: number = LEVEL_HISTORY,
): number[] {
  // Non-finite values come from a meter reading an element with no audio yet.
  // Left in, one `NaN` makes every bar on screen `NaN` tall, so it is dropped
  // rather than propagated.
  const safe = Number.isFinite(level) ? level : 0;
  const next = history.length >= cap ? history.slice(1) : history.slice();
  next.push(safe);
  return next;
}

/**
 * A level as a bar height in 0–1.
 *
 * Clamped rather than scaled: a clipped loud passage should pin the bars, not
 * stretch the scale so everything quieter shrinks.
 */
export function barHeight(level: number, gain: number = LEVEL_GAIN): number {
  if (!Number.isFinite(level) || level <= 0) return 0;
  return Math.min(1, level * gain);
}

/** Where the microphone is, as one word for a status line. */
export type DictationPhase = "off" | "starting" | "listening" | "error";

/**
 * The status line under the dial.
 *
 * Separated from the component because it is the entire feedback for "is it
 * hearing me", and the states that matter are the two that look identical on
 * screen — `listening` with no speech, and `listening` with a detector that
 * never fires. Those get different words, because the second one is a problem
 * the user can act on and the first is not.
 */
export function describeListening(
  phase: DictationPhase,
  hearing: boolean,
): string {
  switch (phase) {
    case "starting":
      return "Waking the microphone…";
    case "error":
      return "Something went wrong";
    case "off":
      return "Not listening";
    case "listening":
      return hearing ? "Hearing you" : "Listening";
  }
}

/**
 * What to tell the user when the microphone is on but nothing is arriving.
 *
 * `blocks` is how many audio blocks Rust has accepted. Zero after a session has
 * been running for a while is the specific failure this exists for: the graph
 * is built, the worklet runs, the session is "listening", and no audio ever
 * crosses — a suspended `AudioContext` or a muted input device. Without this
 * the only symptom is silence, which is indistinguishable from nobody talking.
 */
export function silenceHint(blocks: number, secondsListening: number): string | null {
  if (blocks > 0 || secondsListening < 3) return null;
  return (
    "No audio is reaching Loom. Check that the right microphone is selected, " +
    "and that it is not muted."
  );
}

/** A voice as one line of a picker. */
export function voiceLabel(voice: {
  id: string;
  accent: string | null;
  gender: string | null;
}): string {
  const parts = [voice.accent, voice.gender].filter(
    (part): part is string => !!part,
  );
  return parts.length ? `${voice.id} — ${parts.join(", ")}` : voice.id;
}

/**
 * How far through the reply the read-along is.
 *
 * Returns a fraction for a progress line, and 0 rather than a divide-by-zero
 * when the engine reported no sentences — which happens for a reply that was
 * entirely a code block and so had nothing speakable in it.
 */
export function readAlongProgress(index: number, lines: number): number {
  if (lines <= 0) return 0;
  const clamped = Math.min(Math.max(index, 0), lines - 1);
  return (clamped + 1) / lines;
}
