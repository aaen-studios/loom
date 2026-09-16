import { describe, expect, it } from "vitest";
import { groupToolRuns, segmentMessage } from "./messageExtra";
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
