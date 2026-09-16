import { describe, expect, test } from "bun:test";
import {
  BEATS,
  GOAL,
  PROMPT,
  REASONING,
  REPLY,
  TASKS,
  compactTokens,
  formatUsage,
  frameSignature,
  greetingFor,
  phaseAt,
  reasoningPreview,
  sceneAt,
  settledScene,
  taskStatusAt,
  thinkingLabel,
  toolStateAt,
  type Beat,
  type TaskStatus,
} from "./scene";

/**
 * Tests for the landing page's scripted turn.
 *
 * These exist because the animation is invisible to every other check this
 * project has. `next build` cannot tell that a beat was edited into the wrong
 * order, and `verify-pages.mjs` only ever sees the *first* frame — which is the
 * empty state, so it would pass happily while the entire turn was unreachable.
 *
 * The failure this suite prevents is therefore a silent one: a demonstration that
 * quietly stops demonstrating anything.
 */

const beatNames = Object.keys(BEATS) as Beat[];

describe("the beat sheet", () => {
  test("is in ascending order", () => {
    // A hand-edited beat list is exactly the kind of thing that rots, and a beat
    // out of order does not look wrong — it makes whole phases unreachable.
    for (let index = 1; index < beatNames.length; index += 1) {
      const previous = BEATS[beatNames[index - 1]];
      const current = BEATS[beatNames[index]];
      expect(
        current,
        `${beatNames[index]} (${current}ms) must come after ` +
          `${beatNames[index - 1]} (${previous}ms)`,
      ).toBeGreaterThan(previous);
    }
  });

  test("starts after zero, so the first frame is the empty state", () => {
    expect(BEATS.typingAt).toBeGreaterThan(0);
    // The prerendered HTML *is* this frame. A zero `typingAt` would mean the
    // server rendered mid-typing text, which hydration would then disagree with.
    expect(sceneAt(0).prompt).toBe("");
    expect(sceneAt(0).phase).toBe("empty");
  });

  test("leaves long enough to read the finished turn", () => {
    // At least two seconds between the turn settling and the loop restarting, or
    // the animation restarts while a visitor is still reading the reply.
    expect(BEATS.loopAt - BEATS.settleAt).toBeGreaterThan(2_000);
  });

  test("gives the prompt time to finish typing before it is sent", () => {
    const typingSpan = BEATS.sentAt - BEATS.typingAt;
    // 24ms per character, so this is the budget the longest possible prompt uses.
    expect(PROMPT.length * 24).toBeLessThan(typingSpan);
    // And it is complete at the moment it is sent.
    expect(sceneAt(BEATS.sentAt).prompt).toBe(PROMPT);
  });

  test("finishes the reply before the turn settles", () => {
    expect(sceneAt(BEATS.settleAt).replyText).toBe(REPLY);
    // And it must *not* be complete a beat early, or the streaming caret would
    // never be drawn.
    expect(sceneAt(BEATS.replyAt).replyText.length).toBeLessThan(REPLY.length);
  });

  test("finishes the reasoning before the tool call starts", () => {
    expect(sceneAt(BEATS.toolAt).reasoningText).toBe(REASONING);
  });
});

describe("the phase", () => {
  test("reaches every phase inside one loop", () => {
    // The bug this catches is a beat pushed so far forward that a phase is skipped
    // entirely: the window jumps from thinking to replying and nobody notices,
    // because a screenshot of either end looks fine.
    const seen = new Set<string>();
    for (let elapsed = 0; elapsed < BEATS.loopAt; elapsed += 25) {
      seen.add(sceneAt(elapsed).phase);
    }
    expect([...seen].sort()).toEqual([
      "empty",
      "replying",
      "settled",
      "thinking",
      "tools",
      "typing",
    ]);
  });

  test("is stable at each boundary", () => {
    expect(phaseAt(0)).toBe("empty");
    expect(phaseAt(BEATS.typingAt)).toBe("typing");
    expect(phaseAt(BEATS.sentAt)).toBe("thinking");
    expect(phaseAt(BEATS.toolAt)).toBe("tools");
    expect(phaseAt(BEATS.replyAt)).toBe("replying");
    expect(phaseAt(BEATS.settleAt)).toBe("settled");
    expect(phaseAt(BEATS.loopAt)).toBe("settled");
  });
});

describe("the composer", () => {
  test("fills, then empties when the turn is sent", () => {
    expect(sceneAt(0).prompt).toBe("");
    // Part-way through typing there is a partial string and a live caret.
    const midway = sceneAt(BEATS.typingAt + Math.round(PROMPT.length * 12));
    expect(midway.prompt.length).toBeGreaterThan(0);
    expect(midway.prompt.length).toBeLessThan(PROMPT.length);
    expect(midway.typing).toBe(true);
    // Once sent, the caret is gone: the text has moved into the transcript.
    expect(sceneAt(BEATS.sentAt).typing).toBe(false);
  });

  test("never runs past the prompt's own length", () => {
    for (let elapsed = 0; elapsed <= BEATS.loopAt; elapsed += 25) {
      expect(sceneAt(elapsed).prompt.length).toBeLessThanOrEqual(PROMPT.length);
    }
  });
});

describe("the reply", () => {
  test("only ever grows, and never past its own length", () => {
    let previous = 0;
    for (let elapsed = 0; elapsed <= BEATS.loopAt; elapsed += 25) {
      const length = sceneAt(elapsed).replyText.length;
      expect(length).toBeGreaterThanOrEqual(previous);
      expect(length).toBeLessThanOrEqual(REPLY.length);
      previous = length;
    }
  });

  test("is absent before its beat", () => {
    expect(sceneAt(BEATS.toolDoneAt).replyText).toBe("");
    expect(sceneAt(BEATS.replyAt).replyText).toBe("");
  });
});

describe("the reasoning panel", () => {
  test("shimmers only while the reasoning is still arriving", () => {
    expect(sceneAt(BEATS.thinkingAt).thinking).toBe(true);
    expect(sceneAt(BEATS.toolAt - 1).thinking).toBe(true);
    // Then it stops, which is what draws the eye to the model having committed to
    // an action and moved on.
    expect(sceneAt(BEATS.toolAt).thinking).toBe(false);
    expect(sceneAt(BEATS.replyAt).thinking).toBe(false);
  });

  test("is empty before the turn is sent", () => {
    expect(sceneAt(0).reasoningText).toBe("");
    expect(sceneAt(BEATS.typingAt).reasoningText).toBe("");
  });
});

describe("the tool call", () => {
  test("runs, then reports a duration", () => {
    expect(toolStateAt(0)).toBe("idle");
    expect(toolStateAt(BEATS.toolAt)).toBe("running");
    expect(toolStateAt(BEATS.toolDoneAt)).toBe("done");
    expect(sceneAt(BEATS.toolAt).tool).toBe("running");
    expect(sceneAt(BEATS.toolDoneAt).tool).toBe("done");
  });
});

describe("the task list", () => {
  const order: readonly TaskStatus[] = ["pending", "in_progress", "completed"];
  const rank = (status: TaskStatus) => order.indexOf(status);

  test("never moves backwards", () => {
    // A task that un-checks itself mid-animation reads as a glitch, and it is the
    // easiest mistake to make when editing beats by hand.
    let previous = taskStatusAt(0);
    for (let elapsed = 0; elapsed <= BEATS.loopAt; elapsed += 25) {
      const current = taskStatusAt(elapsed);
      for (let index = 0; index < current.length; index += 1) {
        expect(rank(current[index])).toBeGreaterThanOrEqual(rank(previous[index]));
      }
      previous = current;
    }
  });

  test("always has exactly one item in progress", () => {
    // The panel draws a `now` marker on the in-progress item. Two at once would
    // draw two markers; none at all leaves the panel looking stalled while the
    // turn is still running.
    for (let elapsed = 0; elapsed <= BEATS.loopAt; elapsed += 25) {
      const statuses = taskStatusAt(elapsed);
      expect(statuses.filter((status) => status === "in_progress")).toHaveLength(1);
      expect(statuses).toHaveLength(TASKS.length);
    }
  });

  test("starts on the first item and ends with work still queued", () => {
    expect(taskStatusAt(0)).toEqual(["in_progress", "pending", "pending"]);
    // Two done and one still running, which is the honest end state: a first pass
    // that finishes with something outstanding is what an agent actually looks
    // like, and a list that ticks itself all the way green would be a claim the
    // product has not made.
    expect(taskStatusAt(BEATS.loopAt)).toEqual([
      "completed",
      "completed",
      "in_progress",
    ]);
  });

  test("reports a count that agrees with the statuses", () => {
    for (let elapsed = 0; elapsed <= BEATS.loopAt; elapsed += 25) {
      const frame = sceneAt(elapsed);
      expect(frame.tasksDone).toBe(
        frame.taskStatus.filter((status) => status === "completed").length,
      );
    }
  });
});

describe("the header totals", () => {
  test("climb with the reply rather than appearing at the end", () => {
    expect(sceneAt(BEATS.replyAt).tokenSummary).toBe("2k in · 0 out");
    const midway = sceneAt((BEATS.replyAt + BEATS.replyDoneAt) / 2).tokenSummary;
    expect(midway).not.toBe("2k in · 0 out");
    expect(sceneAt(BEATS.replyDoneAt).tokenSummary).not.toBe("2k in · 0 out");
  });

  test("uses the app's rounding", () => {
    expect(compactTokens(0)).toBe("0");
    expect(compactTokens(999)).toBe("999");
    expect(compactTokens(1_000)).toBe("1k");
    expect(compactTokens(128_000)).toBe("128k");
    expect(compactTokens(1_048_576)).toBe("1M");
    expect(compactTokens(1_500_000)).toBe("1.5M");
  });
});

describe("formatUsage", () => {
  test("matches the app's message footer", () => {
    expect(formatUsage(2_180, 412)).toBe("2.2k in · 412 out");
    expect(formatUsage(1_000, 1_000)).toBe("1k in · 1k out");
    expect(formatUsage(999, 999)).toBe("999 in · 999 out");
  });

  test("reports whichever side it has when the other is missing", () => {
    expect(formatUsage(1_200, null)).toBe("1.2k in");
    expect(formatUsage(null, 340)).toBe("340 out");
    expect(formatUsage(null, null)).toBe("");
  });

  test("deliberately disagrees with the header's formatter", () => {
    // Not a bug. The app genuinely renders both, a couple of inches apart, and
    // reproducing the inconsistency is the difference between a hero built from
    // the real code and one built from an idea of it.
    expect(compactTokens(2_180)).toBe("2k");
    expect(formatUsage(2_180, null)).toBe("2.2k in");
  });
});

describe("the reasoning preview", () => {
  test("shows the last line while streaming and the first once settled", () => {
    // Asymmetric on purpose, and it is the app's behaviour: mid-stream the newest
    // thought is at the bottom, and afterwards the opening sentence is the useful
    // summary. Backwards is invisible until you read it.
    const text = "First thought.\nSecond thought.\nThird thought.";
    expect(reasoningPreview(text, true)).toBe("Third thought.");
    expect(reasoningPreview(text, false)).toBe("First thought.");
  });

  test("strips heading marks and bullets, and collapses whitespace", () => {
    expect(reasoningPreview("## Heading   here", false)).toBe("Heading here");
    expect(reasoningPreview("-   a bullet", false)).toBe("a bullet");
    expect(reasoningPreview("  spaced \n\n  out  ", false)).toBe("spaced");
  });

  test("ignores blank lines rather than previewing nothing", () => {
    // A trailing newline is normal mid-stream, so the preview must not go empty
    // just because the last line happens to be blank.
    expect(reasoningPreview("content\n\n", true)).toBe("content");
    expect(reasoningPreview("\n\n", false)).toBe("");
    expect(reasoningPreview("", true)).toBe("");
  });

  test("handles the scripted reasoning, which is one paragraph", () => {
    // So the preview is the whole line, clipped by CSS rather than truncated
    // here — which is what the app does.
    expect(reasoningPreview(REASONING, true)).toBe(REASONING);
    expect(reasoningPreview(REASONING, false)).toBe(REASONING);
  });
});

describe("the reasoning label", () => {
  test("is the live word, then a duration computed from the beat sheet", () => {
    expect(thinkingLabel(true)).toBe("Thinking");
    // 3100ms → 5200ms is 2.1s, which rounds to 2. Computed, not hardcoded, so
    // editing the beats cannot leave this claiming the wrong number.
    expect(thinkingLabel(false)).toBe("Thought for 2s");
  });
});

describe("the greeting", () => {
  test("matches the app's boundaries exactly", () => {
    expect(greetingFor(0)).toBe("Still up?");
    expect(greetingFor(4)).toBe("Still up?");
    expect(greetingFor(5)).toBe("Good morning");
    expect(greetingFor(11)).toBe("Good morning");
    expect(greetingFor(12)).toBe("Good afternoon");
    expect(greetingFor(17)).toBe("Good afternoon");
    expect(greetingFor(18)).toBe("Good evening");
    expect(greetingFor(23)).toBe("Good evening");
  });

  test("refuses an hour that is not an hour", () => {
    // Rather than returning a plausible-looking greeting for `-1` or `24`, which
    // would hide the caller's bug behind a nice sentence.
    expect(() => greetingFor(-1)).toThrow(RangeError);
    expect(() => greetingFor(24)).toThrow(RangeError);
    expect(() => greetingFor(9.5)).toThrow(RangeError);
  });
});

describe("the settled frame", () => {
  test("is what a reduced-motion visitor sees, and it is complete", () => {
    const frame = settledScene();
    expect(frame.phase).toBe("settled");
    expect(frame.replyText).toBe(REPLY);
    expect(frame.usageText).not.toBe("");
    expect(frame.tasksDone).toBe(2);
    // Nothing about it is mid-flight: no caret, no shimmer, no running tool.
    expect(frame.typing).toBe(false);
    expect(frame.thinking).toBe(false);
    expect(frame.tool).toBe("done");
  });

  test("agrees with the last frame of the loop", () => {
    // So the animated and reduced-motion paths cannot drift apart.
    expect(frameSignature(settledScene())).toBe(frameSignature(sceneAt(BEATS.loopAt)));
  });
});

describe("the frame signature", () => {
  test("changes when something visible changes", () => {
    // The hero commits a frame only when its signature differs, so a signature
    // that missed a field would freeze that part of the window on screen.
    const start = sceneAt(0);
    expect(frameSignature(sceneAt(BEATS.thinkingAt))).not.toBe(frameSignature(start));
    expect(frameSignature(sceneAt(BEATS.toolAt))).not.toBe(frameSignature(start));
    expect(frameSignature(sceneAt(BEATS.replyAt + 500))).not.toBe(
      frameSignature(sceneAt(BEATS.replyAt)),
    );
    expect(frameSignature(sceneAt(BEATS.settleAt))).not.toBe(
      frameSignature(sceneAt(BEATS.replyDoneAt)),
    );
  });

  test("is stable when nothing visible has changed", () => {
    // Two instants inside the settled tail render identically, which is exactly
    // why the settled seconds cost no renders at all.
    expect(frameSignature(sceneAt(BEATS.settleAt + 100))).toBe(
      frameSignature(sceneAt(BEATS.settleAt + 900)),
    );
  });
});

describe("the goal", () => {
  test("is present, because the panel renders it", () => {
    expect(GOAL.length).toBeGreaterThan(0);
    expect(TASKS.length).toBeGreaterThan(1);
  });
});
