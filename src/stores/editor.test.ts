import { beforeEach, describe, expect, it, vi } from "vitest";
import type { EditorTab } from "./editor";

/**
 * The conflict transitions.
 *
 * This is the behaviour that makes autosave acceptable, and it is the one thing
 * an editor must never guess about. The two directions are asymmetric on purpose,
 * and getting them backwards is silent data loss in one case and a nuisance in
 * the other:
 *
 * * **Dirty buffer, file changed on disk** — the user has keystrokes that exist
 *   nowhere else, so this is a *question*. Nothing is written and nothing is
 *   reloaded until they answer.
 * * **Clean buffer, file changed on disk** — there is nothing to lose, so this
 *   reloads quietly. Asking would be asking about nothing.
 *
 * The test drives the store directly with a stubbed IPC layer, because the
 * decision lives in `recheck` rather than in any component: a component test
 * would exercise React's rendering and prove nothing about which branch ran.
 */

/** A tab as the store holds one, with the fields the decision reads. */
function tab(overrides: Partial<EditorTab> = {}): EditorTab {
  return {
    path: "src/a.ts",
    label: "a.ts",
    language: "typescript",
    loading: false,
    error: null,
    hash: "hash-1",
    hashHint: "10:1000",
    eol: "lf",
    bom: false,
    readOnly: false,
    lossy: false,
    dirty: false,
    conflict: false,
    conflictHash: null,
    conflictAt: 0,
    ...overrides,
  };
}

/** The calls the store makes, recorded. */
const calls = {
  fileStatMany: vi.fn(),
  fileRead: vi.fn(),
  gitStatus: vi.fn(),
};

vi.mock("../lib/ipc", () => ({
  ipc: {
    fileStatMany: (...args: unknown[]) => calls.fileStatMany(...args),
    fileRead: (...args: unknown[]) => calls.fileRead(...args),
    gitStatus: (...args: unknown[]) => calls.gitStatus(...args),
    fileSave: vi.fn(),
    gitAvailable: vi.fn(async () => true),
    gitBranches: vi.fn(async () => []),
    gitLog: vi.fn(async () => []),
  },
}));

// `lib/editors` reaches for Monaco, which a node test cannot instantiate. Only
// `setText` and `modelFor` are touched on this path, so both are stubbed.
const setText = vi.fn();
vi.mock("../lib/editors", () => ({
  setText: (...args: unknown[]) => setText(...args),
  modelFor: () => null,
}));

const { useEditor } = await import("./editor");

describe("the conflict decision", () => {
  beforeEach(() => {
    calls.fileStatMany.mockReset();
    calls.fileRead.mockReset();
    setText.mockReset();
    useEditor.setState({
      workdir: "/repo",
      tabs: [],
      activePath: null,
      expanded: [],
      selected: null,
      treeError: null,
    });
  });

  it("reloads a clean buffer without asking", async () => {
    // Nothing to lose, so a banner here would be a question about nothing.
    useEditor.setState({ tabs: [tab({ dirty: false })] });
    calls.fileStatMany.mockResolvedValue([
      { path: "src/a.ts", exists: true, bytes: 11, modified: 2000, hashHint: "11:2000" },
    ]);
    calls.fileRead.mockResolvedValue({
      path: "src/a.ts",
      absolute: "/repo/src/a.ts",
      text: "new contents",
      hash: "hash-2",
      hashHint: "11:2000",
      bytes: 11,
      lines: 1,
      eol: "lf",
      bom: false,
      lossy: false,
      readOnly: false,
    });

    await useEditor.getState().recheck();

    const after = useEditor.getState().tabs[0];
    // Adopted, clean, and no conflict — the file simply changed and we took it.
    expect(after.hash).toBe("hash-2");
    expect(after.hashHint).toBe("11:2000");
    expect(after.conflict).toBe(false);
    expect(after.dirty).toBe(false);
    // The text went into the model, which is the only thing holding it.
    expect(setText).toHaveBeenCalledWith("src/a.ts", "new contents");
  });

  it("raises a conflict on a dirty buffer and writes nothing", async () => {
    // The direction that matters: their keystrokes exist nowhere else.
    useEditor.setState({ tabs: [tab({ dirty: true })] });
    calls.fileStatMany.mockResolvedValue([
      { path: "src/a.ts", exists: true, bytes: 11, modified: 2000, hashHint: "11:2000" },
    ]);

    await useEditor.getState().recheck();

    const after = useEditor.getState().tabs[0];
    expect(after.conflict).toBe(true);
    expect(after.conflictAt).toBe(2000);
    // And crucially: the buffer was not reloaded over the top of the edits, and
    // the file was not even read.
    expect(after.hash).toBe("hash-1");
    expect(after.dirty).toBe(true);
    expect(calls.fileRead).not.toHaveBeenCalled();
    expect(setText).not.toHaveBeenCalled();
  });

  it("does nothing when the hint is unchanged", async () => {
    // The common case, and the reason the hint exists: an open buffer whose file
    // did not change must not cost a file read on every tool call.
    useEditor.setState({ tabs: [tab({ hashHint: "10:1000" })] });
    calls.fileStatMany.mockResolvedValue([
      { path: "src/a.ts", exists: true, bytes: 10, modified: 1000, hashHint: "10:1000" },
    ]);

    await useEditor.getState().recheck();

    expect(calls.fileRead).not.toHaveBeenCalled();
    expect(useEditor.getState().tabs[0].conflict).toBe(false);
  });

  it("leaves a file that no longer exists alone", async () => {
    // A deleted file is not a conflict to resolve in the editor — the tree and
    // the git panel are where that is legible, and reloading here would blank a
    // buffer the user may still want to read from.
    useEditor.setState({ tabs: [tab()] });
    calls.fileStatMany.mockResolvedValue([
      { path: "src/a.ts", exists: false, bytes: 0, modified: 0, hashHint: "gone" },
    ]);

    await useEditor.getState().recheck();

    const after = useEditor.getState().tabs[0];
    expect(after.conflict).toBe(false);
    expect(after.hash).toBe("hash-1");
    expect(calls.fileRead).not.toHaveBeenCalled();
  });

  it("never touches a diff tab", async () => {
    // A diff has no file to restat, and its path is not a path. Asking about one
    // would be asking the backend to stat a string that cannot exist.
    useEditor.setState({ tabs: [tab({ path: "diff:worktree/src/a.ts" })] });
    calls.fileStatMany.mockResolvedValue([]);

    await useEditor.getState().recheck();

    expect(calls.fileStatMany).not.toHaveBeenCalled();
  });

  it("does nothing at all without a workspace folder", async () => {
    useEditor.setState({ workdir: null, tabs: [tab()] });
    await useEditor.getState().recheck();
    expect(calls.fileStatMany).not.toHaveBeenCalled();
  });

  it("does not re-raise a conflict that is already showing", async () => {
    // Otherwise every tool call would overwrite `conflictAt` with a new time,
    // and the banner would read "just now" forever while nothing was resolved.
    useEditor.setState({ tabs: [tab({ dirty: true, conflict: true, conflictAt: 1000 })] });
    calls.fileStatMany.mockResolvedValue([
      { path: "src/a.ts", exists: true, bytes: 11, modified: 9999, hashHint: "11:9999" },
    ]);

    await useEditor.getState().recheck();

    expect(useEditor.getState().tabs[0].conflictAt).toBe(1000);
  });
});

describe("keeping mine", () => {
  beforeEach(() => {
    useEditor.setState({
      workdir: "/repo",
      tabs: [tab({ dirty: true, conflict: true })],
      activePath: "src/a.ts",
      expanded: [],
      selected: null,
      treeError: null,
    });
  });

  it("clears the conflict once the write is accepted", async () => {
    // "Keep mine" is the user deciding, so the guard is deliberately bypassed —
    // see the `force` argument. What has to be true afterwards is that the tab
    // is clean and no longer conflicted, or the banner would never go away.
    const { ipc } = await import("../lib/ipc");
    (ipc.fileSave as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
      kind: "written",
      hash: "hash-3",
      bytes: 12,
    });

    const ok = await useEditor.getState().save("src/a.ts", "mine", true);

    expect(ok).toBe(true);
    const after = useEditor.getState().tabs[0];
    expect(after.conflict).toBe(false);
    expect(after.dirty).toBe(false);
    expect(after.hash).toBe("hash-3");
  });
});
