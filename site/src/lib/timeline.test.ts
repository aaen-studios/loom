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
  reasoningPreview,
  thinkingLabel,
  phaseAt,
  sceneAt,
  settledScene,
  taskStatusAt,
  toolStateAt,
  type Beat,
  type TaskStatus,
} from "./timeline";

/**
 * Tests for the hero's scripted turn.
 *
 * These exist because the animation is invisible to every other check the
 * project has. `next build` cannot tell that a beat was edited into the wrong
 * order, and `verify-pages.mjs` sees only the first frame — which is the empty
 * state, so it would happily pass while the whole turn was unreachable. The
 * failure this suite prevents is a silent one: a scripted turn that never
 * actually plays.
 */

const beatNames = Object.keys(BEATS) as Beat[];

describe("the beat sheet", () => {
  test("is in ascending order", () => {
    // A hand-edited beat list is exactly the kind of thing that rots, and a
    // beat out of order makes entire phases unreachable rather than looking
    // merely wrong.
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
    expect(BEATS.typingStart).toBeGreaterThan(0);
    // The prerendered HTML is this frame. A zero `typingStart` would mean the
    // server rendered mid-typing text, which hydration would then disagree
    // with.
    expect(sceneAt(0).prompt).toBe("");
    expect(sceneAt(0).phase).toBe("empty");
  });

  test("finishes the loop with room to read the result", () => {
    // At least two seconds between the turn settling and the loop restarting,
    // or the animation would restart while a visitor is reading the reply.
    expect(BEATS.loopAt - BEATS.settleAt).toBeGreaterThan(2_000);
  });

  test("leaves the prompt time to finish typing before it is sent", () => {
    const typingSpan = BEATS.sentAt - BEATS.typingStart;
    // 24ms per character, so this is the budget the longest prompt could use.
    expect(PROMPT.length * 24).toBeLessThan(typingSpan);
    // And the prompt is complete at the moment it is sent.
    expect(sceneAt(BEATS.sentAt).prompt).toBe(PROMPT);
  });

  test("finishes the reply before the turn settles", () => {
    expect(sceneAt(BEATS.settleAt).replyText).toBe(REPLY);
    // The reply must not be complete a moment before it is due, or the
    // streaming caret would never be drawn.
    expect(sceneAt(BEATS.replyStartAt).replyText.length).toBeLessThan(REPLY.length);
  });

  test("finishes the reasoning before the tool starts", () => {
    expect(sceneAt(BEATS.toolStartAt).reasoningText).toBe(REASONING);
  });
});

describe("the phase", () => {
  test("reaches every phase inside one loop", () => {
    // The bug this catches is a beat edited so far forward that a phase is
    // skipped entirely — the window would jump from thinking to replying and
    // nobody would notice from a screenshot.
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

  test("is stable at each phase boundary", () => {
    expect(phaseAt(0)).toBe("empty");
    expect(phaseAt(BEATS.typingStart)).toBe("typing");
    expect(phaseAt(BEATS.sentAt)).toBe("thinking");
    expect(phaseAt(BEATS.toolStartAt)).toBe("tools");
    expect(phaseAt(BEATS.replyStartAt)).toBe("replying");
    expect(phaseAt(BEATS.settleAt)).toBe("settled");
    expect(phaseAt(BEATS.loopAt)).toBe("settled");
  });
});

describe("the composer", () => {
  test("is empty, then fills, then empties when sent", () => {
    expect(sceneAt(0).prompt).toBe("");
    // Part-way through typing there is a partial string and a live caret.
    const midway = sceneAt(BEATS.typingStart + Math.round(PROMPT.length * 12));
    expect(midway.prompt.length).toBeGreaterThan(0);
    expect(midway.prompt.length).toBeLessThan(PROMPT.length);
    expect(midway.typing).toBe(true);
    // Once sent, the caret is gone — the text moved into the transcript.
    expect(sceneAt(BEATS.sentAt).typing).toBe(false);
  });

  test("never exceeds the prompt length", () => {
    for (let elapsed = 0; elapsed <= BEATS.loopAt; elapsed += 25) {
      expect(sceneAt(elapsed).prompt.length).toBeLessThanOrEqual(PROMPT.length);
    }
  });
});

describe("the reply", () => {
  test("grows monotonically and never past its own length", () => {
    let previous = 0;
    for (let elapsed = 0; elapsed <= BEATS.loopAt; elapsed += 25) {
      const length = sceneAt(elapsed).replyText.length;
      expect(length).toBeGreaterThanOrEqual(previous);
      expect(length).toBeLessThanOrEqual(REPLY.length);
      previous = length;
    }
  });

  test("is not present before its beat", () => {
    expect(sceneAt(BEATS.toolDoneAt).replyText).toBe("");
    expect(sceneAt(BEATS.replyStartAt).replyText).toBe("");
  });
});

describe("the reasoning panel", () => {
  test("shimmers only while thinking, and stops after", () => {
    expect(sceneAt(BEATS.thinkingAt).thinking).toBe(true);
    expect(sceneAt(BEATS.toolStartAt - 1).thinking).toBe(true);
    // Settled: the shimmer stops, which is what draws the eye to the collapse.
    expect(sceneAt(BEATS.toolStartAt).thinking).toBe(false);
    expect(sceneAt(BEATS.replyStartAt).thinking).toBe(false);
  });

  test("is absent before the turn is sent", () => {
    expect(sceneAt(0).reasoningText).toBe("");
    expect(sceneAt(BEATS.typingStart).reasoningText).toBe("");
  });
});

describe("the tool row", () => {
  test("runs, then reports", () => {
    expect(toolStateAt(BEATS.toolStartAt)).toBe("running");
    expect(toolStateAt(BEATS.toolDoneAt)).toBe("done");
    expect(toolStateAt(0)).toBe("idle");
    expect(sceneAt(BEATS.toolStartAt).tool).toBe("running");
    expect(sceneAt(BEATS.toolDoneAt).tool).toBe("done");
  });
});

describe("the task list", () => {
  const order: readonly TaskStatus[] = ["pending", "in_progress", "completed"];
  const rank = (status: TaskStatus) => order.indexOf(status);

  test("never moves backwards", () => {
    // A task that un-checks itself mid-animation reads as a glitch, and is the
    // easiest mistake to make when editing the beats by hand.
    let previous = taskStatusAt(0);
    for (let elapsed = 0; elapsed <= BEATS.loopAt; elapsed += 25) {
      const current = taskStatusAt(elapsed);
      for (let index = 0; index < current.length; index += 1) {
        expect(rank(current[index])).toBeGreaterThanOrEqual(rank(previous[index]));
      }
      previous = current;
    }
  });

  test("always has exactly one task in progress", () => {
    // The app's panel shows a "now" marker on the in-progress item; two at once
    // would render two markers, and none would leave the panel looking stalled.
    for (let elapsed = 0; elapsed <= BEATS.loopAt; elapsed += 25) {
      const statuses = taskStatusAt(elapsed);
      expect(statuses.filter((status) => status === "in_progress")).toHaveLength(1);
      expect(statuses).toHaveLength(TASKS.length);
    }
  });

  test("starts on the first task and ends with work still queued", () => {
    expect(taskStatusAt(0)).toEqual(["in_progress", "pending", "pending"]);
    // Two done and one still going, which is the honest end state.
    expect(taskStatusAt(BEATS.loopAt)).toEqual([
      "completed",
      "completed",
      "in_progress",
    ]);
  });

  test("reports a count that matches the statuses", () => {
    for (let elapsed = 0; elapsed <= BEATS.loopAt; elapsed += 25) {
      const frame = sceneAt(elapsed);
      expect(frame.tasksDone).toBe(
        frame.taskStatus.filter((status) => status === "completed").length,
      );
    }
  });
});

describe("the header totals", () => {
  test("climb with the reply and never catch up early", () => {
    expect(sceneAt(BEATS.replyStartAt).tokenSummary).toBe("2k in · 0 out");
    const midway = sceneAt(
      (BEATS.replyStartAt + BEATS.replyEndAt) / 2,
    ).tokenSummary;
    expect(midway).not.toBe("2k in · 0 out");
    expect(sceneAt(BEATS.replyEndAt).tokenSummary).not.toBe("2k in · 0 out");
  });

  test("uses the app's rounding", () => {
    expect(compactTokens(0)).toBe("0");
    expect(compactTokens(999)).toBe("999");
    expect(compactTokens(1_000)).toBe("1k");
    expect(compactTokens(128_000)).toBe("128k");
    expect(compactTokens(1_000_000)).toBe("1M");
    expect(compactTokens(1_500_000)).toBe("1.5M");
  });
});

describe("the reasoning preview", () => {
  test("shows the last line while streaming and the first once settled", () => {
    // The asymmetry is the app's, and it is deliberate: mid-stream the newest
    // thought is at the bottom, and afterwards the opening sentence is the
    // useful summary. Getting this backwards is invisible until you read it.
    const text = "First thought.\nSecond thought.\nThird thought.";
    expect(reasoningPreview(text, true)).toBe("Third thought.");
    expect(reasoningPreview(text, false)).toBe("First thought.");
  });

  test("strips markdown bullets and collapses whitespace", () => {
    expect(reasoningPreview("## Heading   here", false)).toBe("Heading here");
    expect(reasoningPreview("-   a bullet", false)).toBe("a bullet");
    expect(reasoningPreview("  spaced \n\n  out  ", false)).toBe("spaced");
  });

  test("ignores blank lines rather than previewing nothing", () => {
    // A trailing newline is normal mid-stream; the preview must not become
    // empty just because the last line is blank.
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
  test("is the app's live word, then a duration from the beat sheet", () => {
    expect(thinkingLabel(true)).toBe("Thinking");
    // 3250ms → 5400ms, so 2.15s reports as 2s. Computed, not hardcoded.
    expect(thinkingLabel(false)).toBe("Thought for 2s");
  });
});

describe("formatUsage", () => {
  test("matches the app's message footer", () => {
    // Verbatim expectations from the app's own test file (`usage.test.ts` for
    // the badge, `messageExtra.ts` for this), so the site cannot quietly invent
    // a different format.
    expect(formatUsage(2_180, 412)).toBe("2.2k in · 412 out");
    expect(formatUsage(1_000, 1_000)).toBe("1k in · 1k out");
    expect(formatUsage(999, 999)).toBe("999 in · 999 out");
  });

  test("reports one side when the other is missing", () => {
    expect(formatUsage(1_200, null)).toBe("1.2k in");
    expect(formatUsage(null, 340)).toBe("340 out");
    expect(formatUsage(null, null)).toBe("");
  });
});

describe("the greeting", () => {
  test("matches the app's boundaries", () => {
    // Verbatim from ChatCanvas.tsx, so the site and the app greet identically.
    expect(greetingFor(0)).toBe("Still up?");
    expect(greetingFor(4)).toBe("Still up?");
    expect(greetingFor(5)).toBe("Good morning");
    expect(greetingFor(11)).toBe("Good morning");
    expect(greetingFor(12)).toBe("Good afternoon");
    expect(greetingFor(17)).toBe("Good afternoon");
    expect(greetingFor(18)).toBe("Good evening");
    expect(greetingFor(23)).toBe("Good evening");
  });

  test("refuses an impossible hour rather than guessing", () => {
    // A silent fallback here would render the wrong greeting with no signal,
    // which is worse than a loud failure in a function nothing else calls.
    expect(() => greetingFor(24)).toThrow(RangeError);
    expect(() => greetingFor(-1)).toThrow(RangeError);
    expect(() => greetingFor(9.5)).toThrow(RangeError);
  });
});

describe("the settled frame", () => {
  test("holds the finished content, including the unfinished task", () => {
    const settled = settledScene();
    expect(settled.prompt).toBe(PROMPT);
    expect(settled.replyText).toBe(REPLY);
    expect(settled.reasoningText).toBe(REASONING);
    expect(settled.thinking).toBe(false);
    expect(settled.tool).toBe("done");
    expect(settled.sent).toBe(true);
    expect(settled.tasksDone).toBe(2);
    expect(settled.phase).toBe("settled");
  });

  test("is what a reduced-motion visitor sees, and is fully populated", () => {
    // The failure this guards: shipping a reduced-motion path that renders an
    // empty window, which would be worse than the animation.
    const settled = settledScene();
    expect(settled.replyText.length).toBeGreaterThan(200);
    expect(settled.reasoningText.length).toBeGreaterThan(100);
    expect(settled.prompt.length).toBeGreaterThan(50);
  });
});

describe("the scripted content", () => {
  test("names a real file from this repository", () => {
    // The reply is a claim about the product, and it names an existing path so
    // the hero cannot quietly start lying after a refactor. `background.ts` is
    // the file the tool row above reads, so the two halves of the animation
    // agree about what is being worked on.
    expect(REPLY).toContain("src/lib/background.ts");
    expect(REPLY).toContain("presets.ts");
    expect(GOAL.length).toBeGreaterThan(10);
  });

  test("exercises inline code while streaming", () => {
    // The transcript renderer splits on backticks, so a reply with no code spans
    // would never exercise that path on the page.
    expect(REPLY).toContain("`src/lib/presets.ts`");
    expect(REPLY.split("`").length).toBeGreaterThan(4);
  });

  test("contains a fenced code block", () => {
    // The strongest reason for this: a fenced block is the only thing that
    // exercises the incomplete-fence state, which is what most of the streaming
    // animation actually spends its time in. A reply with no fence would make
    // that whole branch dead code on the page.
    const fences = (REPLY.match(/```/g) ?? []).length;
    expect(fences).toBe(2);
    expect(REPLY).toContain("```ts");
  });

  test("the fence stays open for a readable stretch of the stream", () => {
    // If the closing fence arrived immediately after the opening one, the
    // incomplete state would flash past. It should be visible for a good part of
    // the reply.
    const openAt = REPLY.indexOf("```ts");
    const closeAt = REPLY.lastIndexOf("```");
    const spanChars = closeAt - openAt;
    expect(spanChars).toBeGreaterThan(150);
    // And a real amount of the stream happens after the block closes, so the
    // "finished block" state is visible too.
    expect(REPLY.length - closeAt).toBeGreaterThan(100);
  });

  test("is not one long paragraph", () => {
    // Streamed text renders block-by-block; a single paragraph would make the
    // "reply is streaming" effect far less legible.
    expect(REPLY.split("\n\n").length).toBeGreaterThanOrEqual(3);
  });

  test("reports usage only once the turn has finished", () => {
    // The app renders the message footer when `!streaming`, so a running turn
    // must not claim a final token count.
    expect(sceneAt(BEATS.replyEndAt - 100).usageText).toBe("");
    expect(sceneAt(BEATS.settleAt).usageText).toBe("2.2k in · 412 out");
  });

  test("uses the app's two different token formatters", () => {
    // The header rounds to whole thousands and the footer keeps a decimal. They
    // are genuinely inconsistent in the app, and copying both is deliberate.
    const settled = settledScene();
    expect(settled.tokenSummary).toBe("2k in · 412 out");
    expect(settled.usageText).toBe("2.2k in · 412 out");
  });
});

describe("the frame signature", () => {
  /**
   * The React side uses this to decide whether to re-render. It is only sound if
   * it changes whenever anything visible changes — a field it misses would be
   * dropped and that part of the window would freeze mid-animation, which is
   * exactly the kind of bug that is invisible in a screenshot and obvious in
   * motion.
   *
   * So this test is deliberately written as "no two distinct frames share a
   * signature", sampled densely across the whole loop. It fails the moment a
   * rendered field is added to `SceneFrame` without being added to the
   * signature.
   */
  test("distinguishes every distinct frame in a loop", () => {
    const bySignature = new Map<string, string>();

    for (let ms = 0; ms <= BEATS.loopAt; ms += 5) {
      const frame = sceneAt(ms);
      const key = frameSignature(frame);
      // `elapsed` is excluded on purpose — nothing renders from it, and
      // including it would make every frame unique and defeat the throttling.
      const rendered = JSON.stringify({ ...frame, elapsed: 0 });
      const existing = bySignature.get(key);

      if (existing !== undefined && existing !== rendered) {
        throw new Error(
          `two different frames share a signature at ${ms}ms:\n` +
            `  ${existing}\n  ${rendered}`,
        );
      }
      bySignature.set(key, rendered);
    }

    // Sanity: the loop really does have many distinct frames, so the check
    // above is not passing because everything collapsed to one signature.
    expect(bySignature.size).toBeGreaterThan(400);
  });

  test("is stable while nothing is changing", () => {
    // The settled seconds are exactly when the throttling should do nothing at
    // all: the turn is over, the text is complete, and the clock is only waiting
    // to restart. One signature across that whole window means zero renders.
    const settledFrom = frameSignature(sceneAt(BEATS.settleAt));
    for (let ms = BEATS.settleAt; ms < BEATS.loopAt; ms += 50) {
      expect(frameSignature(sceneAt(ms))).toBe(settledFrom);
    }
  });

  test("changes at every beat that alters the output", () => {
    // Each of these beats is meant to be *visible*. If a signature is unchanged
    // across one, the animation is doing something the visitor cannot see.
    const beats = [
      BEATS.typingStart,
      BEATS.sentAt,
      BEATS.thinkingAt,
      BEATS.toolStartAt,
      BEATS.toolDoneAt,
      BEATS.replyStartAt,
    ];

    for (const beat of beats) {
      const before = frameSignature(sceneAt(beat - 20));
      const after = frameSignature(sceneAt(beat + 20));
      expect(after, `nothing changes at ${beat}ms`).not.toBe(before);
    }
  });
});
