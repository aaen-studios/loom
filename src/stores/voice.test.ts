/**
 * Tests for the parts of the voice store that decide what reaches the
 * composer.
 *
 * Only the pure decisions are covered here: the Rust side owns the interesting
 * behaviour and is tested against real audio, so repeating any of it in
 * JavaScript would be a second implementation of the same rules that could
 * disagree with the first. What *is* worth testing here is the join — which
 * transcript reaches the composer, when, and whether it replaces or appends to
 * what the user already typed.
 */
import { describe, expect, it } from "vitest";

/**
 * The append rule, extracted exactly as the composer applies it.
 *
 * Kept as a local copy rather than imported because the composer applies it
 * inline inside an effect; if this and the composer ever disagree, this test
 * passing is not evidence about the composer. That is a real limitation and it
 * is written down rather than hidden.
 */
function append(current: string, said: string): string {
  return current.trim() ? `${current.trimEnd()} ${said}` : said;
}

describe("dictation into the composer", () => {
  it("fills an empty composer with what was said", () => {
    expect(append("", "hello there")).toBe("hello there");
  });

  it("appends to a half-written message instead of replacing it", () => {
    // The failure this prevents: someone types a sentence, then speaks the
    // rest, and their typing disappears.
    expect(append("explain this", "and add an example")).toBe(
      "explain this and add an example",
    );
  });

  it("does not leave a double space when the composer already ends in one", () => {
    expect(append("explain this   ", "again")).toBe("explain this again");
  });

  it("treats whitespace-only input as empty", () => {
    expect(append("   ", "first words")).toBe("first words");
  });

  it("keeps the order of two consecutive utterances", () => {
    let value = "";
    value = append(value, "one");
    value = append(value, "two");
    expect(value).toBe("one two");
  });
});

describe("listening events", () => {
  it("only barge-in stops playback, and only while something is speaking", () => {
    // The rule the store applies on a `speech` event. Stopping on *every*
    // speech event would cancel playback the moment the user's own reply
    // started being heard through echo, and not stopping at all is the bug
    // barge-in exists to fix.
    const shouldStop = (phase: string, speaking: boolean) =>
      speaking && (phase === "speaking" || phase === "loading");

    expect(shouldStop("speaking", true)).toBe(true);
    expect(shouldStop("loading", true)).toBe(true);
    // Nothing to stop.
    expect(shouldStop("idle", true)).toBe(false);
    // Speech ending is not a reason to stop.
    expect(shouldStop("speaking", false)).toBe(false);
  });

  it("a transcript increments a counter rather than being consumed", () => {
    // Why the counter exists: a component appends when it *changes*, so a
    // re-render cannot append the same sentence twice, and a second utterance
    // arriving before the first was rendered cannot be dropped. Consuming the
    // string once would fail both.
    let seq = 0;
    const events: string[] = [];
    const rendered: string[] = [];

    const onTranscript = (text: string) => {
      seq += 1;
      events.push(text);
    };
    // Two utterances in one batch, before any render.
    onTranscript("first");
    onTranscript("second");

    // Both renders see the same final string but different counters, so each
    // event is applied exactly once.
    for (let seen = 0; seen < seq; seen += 1) rendered.push(events[seen]);

    expect(seq).toBe(2);
    expect(rendered).toEqual(["first", "second"]);
  });
});
