import { describe, expect, test } from "bun:test";
import { parseBlocks, parseInline } from "./markdown";
import { REPLY } from "./scene";

/**
 * Tests for the streamed-markdown parser.
 *
 * These matter more than they look. The parser runs on text that arrives a few
 * characters at a time, so most of its inputs are malformed by an ordinary
 * markdown parser's standards — an open fence, an empty language, a half-written
 * span. Every state below is one that genuinely occurs mid-stream, and each is a
 * bug if handled wrong: a thrown error blanks the reply, and a fence rendered as
 * prose puts raw backticks on the page.
 */

describe("parseBlocks", () => {
  test("returns nothing for empty text", () => {
    expect(parseBlocks("")).toEqual([]);
    // Whitespace only, which is what the first frames of a reply look like.
    expect(parseBlocks("\n\n")).toEqual([]);
  });

  test("reads a paragraph", () => {
    expect(parseBlocks("One line.")).toEqual([{ kind: "prose", text: "One line." }]);
  });

  test("splits paragraphs on blank lines", () => {
    // The bug this catches: trimming the surrounding newlines but leaving a `\n\n`
    // *inside* a block renders two paragraphs as one run-on, because HTML
    // collapses a bare newline to a space.
    expect(parseBlocks("First para.\n\nSecond para.")).toEqual([
      { kind: "prose", text: "First para." },
      { kind: "prose", text: "Second para." },
    ]);
  });

  test("keeps a soft wrap inside its paragraph", () => {
    // A single newline is a wrapped line, not a paragraph break.
    expect(parseBlocks("one\nline\nwrapped")).toEqual([
      { kind: "prose", text: "one\nline\nwrapped" },
    ]);
  });

  test("survives stray and repeated blank lines", () => {
    const blocks = parseBlocks("\n\na\n\n\n\nb\n\nc\n\n");
    expect(blocks.map((block) => block.kind)).toEqual(["prose", "prose", "prose"]);
    expect(blocks.map((block) => (block.kind === "prose" ? block.text : ""))).toEqual([
      "a",
      "b",
      "c",
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
    // The whole thing arrived, so the copy action is live.
    expect(code.incomplete).toBe(false);
  });

  test("marks a fence whose closing marker has not arrived", () => {
    // This is the state a streaming reply spends most of its time in, and the
    // only thing distinguishing it from a finished block.
    const blocks = parseBlocks("```ts\nconst x = 1;");
    expect(blocks).toHaveLength(1);
    const code = blocks[0];
    if (code.kind !== "code") throw new Error("expected a code block");
    expect(code.incomplete).toBe(true);
    expect(code.code).toBe("const x = 1;");
  });

  test("copes with a fence that has only just opened", () => {
    // The pause immediately after ```, before the language has been typed.
    const blocks = parseBlocks("```");
    const code = blocks[0];
    if (code.kind !== "code") throw new Error("expected a code block");
    expect(code.incomplete).toBe(true);
    expect(code.language).toBe("");
    expect(code.code).toBe("");
  });

  test("copes with a fence whose language has arrived but whose newline has not", () => {
    const blocks = parseBlocks("```ts");
    const code = blocks[0];
    if (code.kind !== "code") throw new Error("expected a code block");
    expect(code.language).toBe("ts");
    expect(code.code).toBe("");
  });

  test("keeps blank lines inside a code block", () => {
    // The page numbers every line, so dropping a blank one would shift every
    // number below it — a silent lie about where the code is.
    const blocks = parseBlocks("```\na\n\nb\n```");
    const code = blocks[0];
    if (code.kind !== "code") throw new Error("expected a code block");
    expect(code.code).toBe("a\n\nb");
  });

  test("handles two fenced blocks in one reply", () => {
    const blocks = parseBlocks("```ts\na\n```\n\nmiddle\n\n```rs\nb\n```");
    expect(blocks.map((block) => block.kind)).toEqual(["code", "prose", "code"]);
    expect(blocks.filter((block) => block.kind === "code")).toHaveLength(2);
    // Neither is incomplete: the fences balance.
    for (const block of blocks) {
      if (block.kind === "code") expect(block.incomplete).toBe(false);
    }
  });

  test("treats a fence with no language as an empty language", () => {
    const blocks = parseBlocks("```\nplain\n```");
    const code = blocks[0];
    if (code.kind !== "code") throw new Error("expected a code block");
    expect(code.language).toBe("");
    expect(code.code).toBe("plain");
  });

  test("strips exactly one trailing newline from a fence body", () => {
    // One, not all: the closing fence's own newline would otherwise render as an
    // extra numbered blank line, but a deliberate blank line at the end of the
    // body is the author's.
    const blocks = parseBlocks("```\na\n\n```");
    const code = blocks[0];
    if (code.kind !== "code") throw new Error("expected a code block");
    expect(code.code).toBe("a\n");
  });

  test("survives the whole reply arriving one character at a time", () => {
    // The strongest statement that can be made about a streaming parser: no prefix
    // of real input may throw or lose text. Run against the reply the landing page
    // actually renders, so this cannot drift away from what is on screen.
    for (let length = 0; length <= REPLY.length; length += 1) {
      const partial = REPLY.slice(0, length);
      expect(() => parseBlocks(partial)).not.toThrow();

      // Every fence marker that has arrived must be accounted for: either its
      // block is closed, or it is flagged incomplete.
      const blocks = parseBlocks(partial);
      const fences = (partial.match(/```/g) ?? []).length;
      const codeBlocks = blocks.filter((block) => block.kind === "code").length;
      expect(codeBlocks).toBe(Math.ceil(fences / 2));
    }
  });

  test("never loses prose while the reply streams", () => {
    // The other half of the streaming property: text that has arrived must be
    // somewhere in the output, not silently dropped between two blocks.
    for (let length = 0; length <= REPLY.length; length += 25) {
      const partial = REPLY.slice(0, length);
      const blocks = parseBlocks(partial);
      const rendered = blocks
        .map((block) => (block.kind === "prose" ? block.text : block.code))
        .join("\n");
      // Fence markers are the only thing a reader should never see, and the
      // parser is what removes them.
      expect(rendered).not.toContain("```");
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
    // Which is what stops a half-typed `span` from rendering as prose and then
    // snapping into place when the closing backtick shows up.
    expect(parseInline("see `weave")).toEqual([
      { code: false, text: "see " },
      { code: true, text: "weave" },
    ]);
  });

  test("handles text with no backticks at all", () => {
    expect(parseInline("plain")).toEqual([{ code: false, text: "plain" }]);
  });

  test("handles an empty string", () => {
    expect(parseInline("")).toEqual([{ code: false, text: "" }]);
  });
});
