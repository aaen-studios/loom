import { describe, expect, it } from "vitest";
import { BUILTIN_COMMANDS, parseCommand, parseUserPrefix } from "./commands";

describe("parseCommand", () => {
  it("reads a built-in with its arguments", () => {
    const parsed = parseCommand("/goal fix the rename");
    expect(parsed?.command.id).toBe("goal");
    expect(parsed?.args).toBe("fix the rename");
  });

  it("reads a bare command with no arguments", () => {
    expect(parseCommand("/todos")?.args).toBe("");
  });

  it("requires a lowercase name", () => {
    // Not a preference: the leading token is matched as `[a-z0-9-]`, so an
    // uppercase name is not a command at all rather than a command with a
    // different case. Asserted so that stays a decision rather than a surprise.
    expect(parseCommand("/GOAL x")).toBeNull();
  });

  it("keeps a multi-line argument", () => {
    expect(parseCommand("/goal one\ntwo")?.args).toBe("one\ntwo");
  });

  it("ignores an unknown name", () => {
    expect(parseCommand("/nope")).toBeNull();
  });

  it("ignores a slash that is not a command", () => {
    expect(parseCommand("/usr/local/bin is a path")).toBeNull();
    expect(parseCommand("hello /goal")).toBeNull();
  });
});

describe("parseUserPrefix", () => {
  it("reads a goal message", () => {
    expect(parseUserPrefix("Goal: fix the rename")).toEqual({
      kind: "goal",
      label: "Goal",
      rest: "fix the rename",
    });
  });

  it("reads a task message", () => {
    expect(parseUserPrefix("Task: add tests")).toEqual({
      kind: "task",
      label: "Task",
      rest: "add tests",
    });
  });

  it("reads a slash command left at the head of a message", () => {
    // What an older chat contains, and what a send past a dismissed menu does.
    expect(parseUserPrefix("/plan do the thing")).toEqual({
      kind: "command",
      label: "/plan",
      rest: "do the thing",
    });
  });

  it("recognises every built-in, so the badge list cannot drift", () => {
    for (const command of BUILTIN_COMMANDS) {
      const parsed = parseUserPrefix(`/${command.id} body`);
      // `/new` and `/todos` take no arguments, so they are consumed as commands
      // rather than sent — but if one ever does appear in a transcript, the
      // badge must still name it.
      expect(parsed?.label).toBe(`/${command.id}`);
    }
  });

  it("leaves ordinary prose alone", () => {
    expect(parseUserPrefix("just a message")).toBeNull();
    expect(parseUserPrefix("Goals: many")).toBeNull();
    expect(parseUserPrefix("the Goal: prefix")).toBeNull();
  });

  it("ignores a slash that names nothing", () => {
    expect(parseUserPrefix("/usr/local is a path")).toBeNull();
  });

  it("keeps a multi-line body", () => {
    expect(parseUserPrefix("Goal: one\ntwo")?.rest).toBe("one\ntwo");
  });
});
