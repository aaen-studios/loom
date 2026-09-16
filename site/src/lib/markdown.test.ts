import { describe, expect, test } from "bun:test";
import { isSubstantial, parseBlocks, parseInline } from "./markdown";

/**
 * Tests for the streamed-markdown parser.
 *
 * These matter more than they look. The parser runs on text that is arriving a
 * few characters at a time, so most of its inputs are malformed by a normal
 * markdown parser's standards — an open fence, an empty language, a half-written
 * span. The states below are the ones that actually occur mid-stream, and each
 * one is a bug if handled wrong: a thrown error blanks the hero, and a fence
 * rendered as prose shows the raw backticks on screen.
 */

describe("parseBlocks", () => {
  test("returns nothing for empty text", () => {
    expect(parseBlocks("")).toEqual([]);
    // Whitespace only, which is what the first frames of a reply look like.
    expect(parseBlocks("\n\n")).toEqual([]);
  });

  test("reads a paragraph", () => {
    const blocks = parseBlocks("One line.");
    expect(blocks).toEqual([{ kind: "prose", text: "One line." }]);
  });

  test("splits paragraphs on blank lines", () => {
    // The bug this caught: trimming the surrounding newlines but leaving a
    // `\n\n` *inside* a block renders two paragraphs as one run-on, because
    // HTML collapses a bare newline to a space.
    const blocks = parseBlocks("First para.\n\nSecond para.");
    expect(blocks).toEqual([
      { kind: "prose", text: "First para." },
      { kind: "prose", text: "Second para." },
    ]);
  });

  test("keeps a soft wrap inside its paragraph", () => {
    // A single newline is a wrapped line, not a paragraph break.
    const blocks = parseBlocks("one\nline\nwrapped");
    expect(blocks).toEqual([{ kind: "prose", text: "one\nline\nwrapped" }]);
  });

  test("handles three paragraphs and stray blank lines", () => {
    const blocks = parseBlocks("\n\na\n\n\n\nb\n\nc\n\n");
    expect(blocks.map((block) => block.kind)).toEqual([
      "prose",
      "prose",
      "prose",
    ]);
  });

  test("reads a complete fenced block", () => {
    const blocks = parseBlocks("Before.\n\n```ts\nconst x = 1;\n```\n\nAfter.");
    expect(blocks.map((block) => block.kind)).toEqual(["prose", "code", "prose"]);

    const code = blocks[1];
    expect(code.kind).toBe("code");
    if (code.kind !== "code") return;
    expect(code.language).toBe("ts");
    expect(code.code).toBe("const x = 1;");
    // The whole block arrived, so the copy action is live.
    expect(code.incomplete).toBe(false);
  });

  test("marks a fence whose closing marker has not arrived", () => {
    // This is the state a streaming reply spends most of its time in, and it is
    // the only thing that distinguishes it from a finished block.
    const blocks = parseBlocks("```ts\nconst x = 1;");
    expect(blocks).toHaveLength(1);
    const code = blocks[0];
    if (code.kind !== "code") throw new Error("expected a code block");
    expect(code.incomplete).toBe(true);
    expect(code.code).toBe("const x = 1;");
  });

  test("copes with a fence that has just opened", () => {
    // The pause right after ``` — the language has not been typed yet.
    const blocks = parseBlocks("```");
    const code = blocks[0];
    if (code.kind !== "code") throw new Error("expected a code block");
    expect(code.incomplete).toBe(true);
    expect(code.language).toBe("");
    expect(code.code).toBe("");
  });

  test("copes with a fence that has opened but has no newline yet", () => {
    const blocks = parseBlocks("```ts");
    const code = blocks[0];
    if (code.kind !== "code") throw new Error("expected a code block");
    expect(code.language).toBe("ts");
    expect(code.code).toBe("");
  });

  test("keeps blank lines inside a code block", () => {
    // The app numbers every line, so dropping a blank one would shift every
    // number below it.
    const blocks = parseBlocks("```\na\n\nb\n```");
    const code = blocks[0];
    if (code.kind !== "code") throw new Error("expected a code block");
    expect(code.code).toBe("a\n\nb");
  });

  test("handles two fenced blocks in one reply", () => {
    const blocks = parseBlocks("```ts\na\n```\n\nmiddle\n\n```rs\nb\n```");
    expect(blocks.map((block) => block.kind)).toEqual([
      "code",
      "prose",
      "code",
    ]);
    expect(blocks.filter((block) => block.kind === "code")).toHaveLength(2);
    // Neither is incomplete: the fences balance.
    for (const block of blocks) {
      if (block.kind === "code") expect(block.incomplete).toBe(false);
    }
  });

  test("treats a fence with no language as an empty language, not as code", () => {
    const blocks = parseBlocks("```\nplain\n```");
    const code = blocks[0];
    if (code.kind !== "code") throw new Error("expected a code block");
    expect(code.language).toBe("");
    expect(code.code).toBe("plain");
  });

  test("survives the whole reply arriving one character at a time", () => {
    // The strongest statement that can be made about a streaming parser: no
    // prefix of real input may throw or lose text. Run against the reply the
    // hero actually renders, so it cannot drift from the page.
    const { REPLY } = require("./timeline") as typeof import("./timeline");
    for (let length = 0; length <= REPLY.length; length += 1) {
      const partial = REPLY.slice(0, length);
      expect(() => parseBlocks(partial)).not.toThrow();

      // Every fence marker that has arrived must be accounted for: either the
      // block is closed, or it is flagged incomplete.
      const blocks = parseBlocks(partial);
      const fences = (partial.match(/```/g) ?? []).length;
      const codeBlocks = blocks.filter((block) => block.kind === "code").length;
      expect(codeBlocks).toBe(Math.ceil(fences / 2));
    }
  });
});

describe("parseInline", () => {
  test("alternates prose and code", () => {
    expect(parseInline("a `b` c")).toEqual([
      { code: false, text: "a " },
      { code: true, text: "b" },
      { code: false, text: " c" },
    ]);
  });

  test("treats an unterminated span as code", () => {
    // Which is what makes a half-typed `span` render as code rather than
    // snapping in when the closing backtick arrives.
    expect(parseInline("see `weave")).toEqual([
      { code: false, text: "see " },
      { code: true, text: "weave" },
    ]);
  });

  test("handles text with no backticks", () => {
    expect(parseInline("plain")).toEqual([{ code: false, text: "plain" }]);
  });
});

describe("isSubstantial", () => {
  test("is false for a fence with no body yet", () => {
    expect(
      isSubstantial({ kind: "code", language: "ts", code: "", incomplete: true }),
    ).toBe(false);
    expect(
      isSubstantial({ kind: "code", language: "", code: "  ", incomplete: true }),
    ).toBe(false);
  });

  test("is true once there is code", () => {
    expect(
      isSubstantial({ kind: "code", language: "ts", code: "x", incomplete: true }),
    ).toBe(true);
  });
});
