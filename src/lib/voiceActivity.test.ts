/**
 * Tests for the voice-mode surface's pure logic.
 *
 * Everything here is a decision the surface makes, extracted so it can be
 * stated rather than looked at: how loud a level draws, how much history the
 * meter keeps, what the status line says, and which of the two silent-looking
 * states the user is actually in.
 *
 * The level tests carry more weight than their size suggests. `barHeight` is
 * now the single definition of "how loud is this" for *both* the voice surface
 * and the composer's meter — before that the composer used a literal `× 400`
 * while the surface used `× 4`, so the same microphone drew two different
 * meters depending on which screen you were looking at.
 */
import { describe, expect, it } from "vitest";
import {
  LEVEL_HISTORY,
  barHeight,
  describeListening,
  pushSample,
  readAlongProgress,
  silenceHint,
  voiceLabel,
} from "./voiceActivity";

describe("the level meter", () => {
  it("keeps a bounded history, dropping the oldest", () => {
    let history: number[] = [];
    for (let i = 0; i < LEVEL_HISTORY + 10; i += 1) {
      history = pushSample(history, i / 100);
    }
    expect(history).toHaveLength(LEVEL_HISTORY);
    // The oldest ten are gone, so the first survivor is the eleventh value.
    expect(history[0]).toBeCloseTo(10 / 100, 5);
  });

  it("does not mutate the history it is given", () => {
    // A store that mutated its own state in place would not re-render, so the
    // meter would freeze at whatever it drew first.
    const before = [0.1, 0.2];
    const after = pushSample(before, 0.3);
    expect(before).toEqual([0.1, 0.2]);
    expect(after).toHaveLength(3);
  });

  it("drops a non-finite level rather than poisoning the meter", () => {
    // One `NaN` makes every bar `NaN` tall, which renders as a meter that has
    // stopped moving — the same symptom as a dead microphone, for an unrelated
    // reason.
    expect(pushSample([], Number.NaN)).toEqual([0]);
    expect(pushSample([], Number.POSITIVE_INFINITY)).toEqual([0]);
  });

  it("scales speech up to something visible", () => {
    // Typical speech RMS through a microphone is 0.02–0.15. Drawn raw on a 0–1
    // scale the meter barely moves and reads as broken.
    expect(barHeight(0.1)).toBeCloseTo(0.4, 5);
    expect(barHeight(0.02)).toBeCloseTo(0.08, 5);
  });

  it("clamps a loud passage rather than rescaling the whole meter", () => {
    // A clipped peak should pin the bars. Scaling by the maximum instead would
    // make everything quieter shrink whenever someone raised their voice.
    expect(barHeight(0.5)).toBe(1);
    expect(barHeight(4)).toBe(1);
  });

  it("treats silence and nonsense as no signal", () => {
    expect(barHeight(0)).toBe(0);
    expect(barHeight(-0.2)).toBe(0);
    expect(barHeight(Number.NaN)).toBe(0);
  });
});

describe("the status line", () => {
  it("distinguishes hearing you from listening", () => {
    // The two states that look identical on screen, and the whole reason the
    // line exists: "Listening" with no speech and "Listening" with a detector
    // that never fires are different problems.
    expect(describeListening("listening", true)).toBe("Hearing you");
    expect(describeListening("listening", false)).toBe("Listening");
  });

  it("says something specific for every phase it is given", () => {
    for (const phase of ["off", "starting", "listening", "error"] as const) {
      const line = describeListening(phase, false);
      expect(line.length).toBeGreaterThan(0);
      // A blank or developer-facing string here would leave the user with no
      // feedback at all in the one place they look for it.
      expect(line).not.toMatch(/undefined|NaN|\[object/);
    }
  });
});

describe("the no-audio hint", () => {
  it("says nothing while audio is arriving", () => {
    expect(silenceHint(1, 60)).toBeNull();
    expect(silenceHint(500, 60)).toBeNull();
  });

  it("says nothing in the first moments, when there is nothing to explain", () => {
    // Cryptic advice in the first second would be the most confusing possible
    // time for it: the user has only just pressed the button.
    expect(silenceHint(0, 0)).toBeNull();
    expect(silenceHint(0, 2)).toBeNull();
  });

  it("explains itself once silence stops being plausible", () => {
    const hint = silenceHint(0, 5);
    expect(hint).not.toBeNull();
    // Names the two things a person can actually check.
    expect(hint).toMatch(/microphone/i);
    expect(hint).toMatch(/muted/i);
  });
});

describe("the voice picker", () => {
  it("labels a fully described voice", () => {
    expect(
      voiceLabel({ id: "bf_emma", accent: "British English", gender: "female" }),
    ).toBe("bf_emma — British English, female");
  });

  it("falls back to the id alone when nothing else is known", () => {
    // Better a bare id than "null, null", which is what a naive join gives.
    expect(voiceLabel({ id: "zf_xiaobei", accent: null, gender: null })).toBe(
      "zf_xiaobei",
    );
  });

  it("uses whatever single detail it has", () => {
    expect(voiceLabel({ id: "am_michael", accent: null, gender: "male" })).toBe(
      "am_michael — male",
    );
  });
});

describe("read-along progress", () => {
  it("reports a fraction through the reply", () => {
    expect(readAlongProgress(0, 4)).toBeCloseTo(0.25, 5);
    expect(readAlongProgress(3, 4)).toBe(1);
  });

  it("never divides by zero for a reply with nothing speakable", () => {
    // A reply that was entirely a code block produces no sentences at all, and
    // this is the value that reaches a `width` style.
    expect(readAlongProgress(0, 0)).toBe(0);
    expect(Number.isFinite(readAlongProgress(-1, 0))).toBe(true);
  });

  it("stays inside the bar when the index runs past the list", () => {
    // A late `chunk` from a superseded utterance can outlive its `started`.
    expect(readAlongProgress(99, 4)).toBe(1);
    expect(readAlongProgress(-5, 4)).toBeCloseTo(0.25, 5);
  });
});
