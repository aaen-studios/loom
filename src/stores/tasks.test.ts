import { describe, expect, it, beforeEach } from "vitest";
import { activeCommandCount, activeTaskCount, useTasks } from "./tasks";
import type { CommandRun, Task } from "../types";

function command(over: Partial<CommandRun> & { id: string }): CommandRun {
  return {
    sessionId: "s1",
    label: "tests",
    command: "cargo test",
    cwd: "C:/work",
    pid: 1234,
    status: "running",
    exitCode: null,
    logPath: "C:/logs/cmd.log",
    background: true,
    createdAt: 1_000,
    finishedAt: null,
    ...over,
  };
}

function task(over: Partial<Task> & { id: string }): Task {
  return {
    sessionId: "s1",
    originSession: null,
    jobId: null,
    title: "a run",
    prompt: "do something",
    providerId: null,
    modelId: null,
    status: "running",
    detail: null,
    result: null,
    notify: true,
    createdAt: 1_000,
    startedAt: 1_000,
    finishedAt: null,
    ...over,
  };
}

beforeEach(() => {
  useTasks.setState({ tasks: [], jobs: [], commands: [], loaded: false, loading: false });
});

describe("applyCommand", () => {
  it("prepends a command it has never seen", () => {
    const { applyCommand } = useTasks.getState();
    applyCommand(command({ id: "a", createdAt: 1_000 }));
    applyCommand(command({ id: "b", createdAt: 2_000 }));

    // Newest first, so the panel reads top-down like a log.
    expect(useTasks.getState().commands.map((entry) => entry.id)).toEqual(["b", "a"]);
  });

  it("replaces a command in place instead of duplicating it", () => {
    const { applyCommand } = useTasks.getState();
    applyCommand(command({ id: "a", status: "running" }));
    applyCommand(command({ id: "a", status: "failed", exitCode: 101, finishedAt: 2_000 }));

    const commands = useTasks.getState().commands;
    expect(commands).toHaveLength(1);
    expect(commands[0].status).toBe("failed");
    expect(commands[0].exitCode).toBe(101);
  });

  it("re-sorts when a status change arrives out of order", () => {
    const { applyCommand } = useTasks.getState();
    applyCommand(command({ id: "old", createdAt: 1_000 }));
    applyCommand(command({ id: "new", createdAt: 5_000 }));
    // A late event about an older command must not push it above the newer one.
    applyCommand(command({ id: "old", createdAt: 1_000, status: "done" }));
    expect(useTasks.getState().commands.map((entry) => entry.id)).toEqual(["new", "old"]);
  });
});

describe("active counts", () => {
  it("counts only running commands, and queued/running tasks", () => {
    const commands = [
      command({ id: "a", status: "running" }),
      command({ id: "b", status: "running" }),
      command({ id: "c", status: "done" }),
      command({ id: "d", status: "orphaned" }),
    ];
    expect(activeCommandCount(commands)).toBe(2);

    const tasks = [task({ id: "1" }), task({ id: "2", status: "queued" }), task({ id: "3", status: "done" })];
    expect(activeTaskCount(tasks)).toBe(2);
  });
});
