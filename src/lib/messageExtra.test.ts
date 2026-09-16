import { describe, expect, it } from "vitest";
import {
  groupToolRuns,
  parseCondensed,
  segmentMessage,
  withCondensed,
} from "./messageExtra";
import type { ToolCallRecord } from "../types";

function call(id: string, after: number): ToolCallRecord {
  return { id, name: "web_search", arguments: "{}", status: "ok", output: "", after };
}

function tool(id: string, name: string): ToolCallRecord {
  return { id, name, arguments: "{}", status: "ok", output: "" };
}

describe("groupToolRuns", () => {
  it("keeps a lone call as its own run", () => {
    const runs = groupToolRuns([tool("a", "read_file")]);
    expect(runs.map((run) => run.map((entry) => entry.id))).toEqual([["a"]]);
  });

  it("folds a run of the same tool into one group", () => {
    const runs = groupToolRuns([
      tool("a", "read_file"),
      tool("b", "read_file"),
      tool("c", "read_file"),
    ]);
    expect(runs.map((run) => run.length)).toEqual([3]);
  });

  it("starts a new group when the tool changes", () => {
    const runs = groupToolRuns([
      tool("a", "read_file"),
      tool("b", "read_file"),
      tool("c", "grep"),
      tool("d", "read_file"),
    ]);
    expect(runs.map((run) => run.map((entry) => entry.name))).toEqual([
      ["read_file", "read_file"],
      ["grep"],
      ["read_file"],
    ]);
  });

  it("returns nothing for no calls", () => {
    expect(groupToolRuns([])).toEqual([]);
  });
});

describe("segmentMessage", () => {
  it("returns one text segment when there are no calls", () => {
    expect(segmentMessage("hello", [])).toEqual([
      { kind: "text", text: "hello" },
    ]);
  });

  it("interleaves text and tool runs in stream order", () => {
    const segments = segmentMessage("Let me look this up. Found it.", [
      call("a", 20),
    ]);
    expect(segments).toEqual([
      { kind: "text", text: "Let me look this up." },
      { kind: "tools", calls: [call("a", 20)] },
      { kind: "text", text: " Found it." },
    ]);
  });

  it("keeps calls that ran in the same round in one stack", () => {
    const segments = segmentMessage("Checking.", [call("a", 9), call("b", 9)]);
    expect(segments.map((segment) => segment.kind)).toEqual(["text", "tools"]);
    const tools = segments[1];
    if (tools.kind !== "tools") throw new Error("expected tools");
    expect(tools.calls.map((entry) => entry.id)).toEqual(["a", "b"]);
  });

  it("puts calls with no text before them first", () => {
    const segments = segmentMessage("Done.", [call("a", 0)]);
    expect(segments[0]).toEqual({ kind: "tools", calls: [call("a", 0)] });
    expect(segments[1]).toEqual({ kind: "text", text: "Done." });
  });

  it("counts code points, so astral characters do not desync offsets", () => {
    const segments = segmentMessage("hi 🚀 there", [call("a", 4)]);
    expect(segments).toEqual([
      { kind: "text", text: "hi 🚀" },
      { kind: "tools", calls: [call("a", 4)] },
      { kind: "text", text: " there" },
    ]);
  });

  it("clamps offsets past the end of the text", () => {
    const segments = segmentMessage("short", [call("a", 99)]);
    expect(segments).toEqual([
      { kind: "text", text: "short" },
      { kind: "tools", calls: [call("a", 99)] },
    ]);
  });

  it("treats legacy records without an offset as coming first", () => {
    const legacy: ToolCallRecord = {
      id: "a",
      name: "read_file",
      arguments: "{}",
      status: "ok",
      output: "",
    };
    const segments = segmentMessage("text", [legacy]);
    expect(segments[0].kind).toBe("tools");
  });
});

describe("condensed replies", () => {
  it("reads the fold the engine recorded on a reply", () => {
    const extra = JSON.stringify({
      toolCalls: [],
      usage: { inputTokens: 1, outputTokens: 2 },
      condensed: { covered: 24, source: "summary", tokens: 3_000 },
    });
    expect(parseCondensed(extra)).toEqual({
      covered: 24,
      source: "summary",
      tokens: 3_000,
    });
  });

  /// A reply that was not condensed must have no line, which is every reply in
  /// a short chat.
  it("returns nothing for a reply that was not condensed", () => {
    expect(parseCondensed(null)).toBeNull();
    expect(parseCondensed("")).toBeNull();
    expect(parseCondensed("{")).toBeNull();
    expect(parseCondensed(JSON.stringify({ usage: {} }))).toBeNull();
    // A fold covering nothing is not a fold worth a line.
    expect(
      parseCondensed(JSON.stringify({ condensed: { covered: 0, source: "digest" } })),
    ).toBeNull();
    expect(parseCondensed(JSON.stringify({ condensed: "yes" }))).toBeNull();
  });

  /// An unrecognised source reads as the fallback, which is the honest reading:
  /// it is what a block is built from when nothing wrote one.
  it("treats an unknown source as the digest", () => {
    const parsed = parseCondensed(
      JSON.stringify({ condensed: { covered: 5, source: "something-else" } }),
    );
    expect(parsed?.source).toBe("digest");
    expect(parsed?.tokens).toBe(0);
  });

  it("writes the fold onto a message without losing what was there", () => {
    const before = JSON.stringify({ usage: { inputTokens: 9, outputTokens: 9 } });
    const after = withCondensed(before, {
      covered: 12,
      source: "digest",
      tokens: 500,
    });
    const parsed = JSON.parse(after ?? "{}");
    expect(parsed.usage).toEqual({ inputTokens: 9, outputTokens: 9 });
    expect(parseCondensed(after)).toEqual({
      covered: 12,
      source: "digest",
      tokens: 500,
    });
  });

  it("leaves the extra alone when there was no fold", () => {
    const before = JSON.stringify({ usage: { inputTokens: 1, outputTokens: 1 } });
    expect(withCondensed(before, null)).toBe(before);
    expect(withCondensed(before, undefined)).toBe(before);
    // A reply with no extra at all and no fold stays without one.
    expect(withCondensed(null, null)).toBeNull();
  });

  /// Unreadable to start with: replace it rather than compound the damage.
  it("replaces an unparseable extra rather than losing the fold", () => {
    const parsed = withCondensed("not json", {
      covered: 3,
      source: "summary",
      tokens: 10,
    });
    expect(parseCondensed(parsed)).toEqual({
      covered: 3,
      source: "summary",
      tokens: 10,
    });
  });

  /// User messages store a bare attachment array, so the base cannot be assumed
  /// to be an object.
  it("does not merge into an array-shaped extra", () => {
    const parsed = withCondensed(JSON.stringify([{ name: "a.png" }]), {
      covered: 2,
      source: "digest",
      tokens: 5,
    });
    expect(parseCondensed(parsed)?.covered).toBe(2);
  });
});
