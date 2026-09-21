import { create } from "zustand";
import { ipc } from "../lib/ipc";
import { isDiffPath } from "../lib/fileTreeOperations";
import type { Branch, GitCommit, GitStatus } from "../types";
import { useChat } from "./chat";
import { useEditor } from "./editor";
import { useSettings } from "./settings";

/** A repository with nothing in it, for a folder that has no `.git`. */
const EMPTY: GitStatus = {
  isRepo: false,
  root: null,
  branch: null,
  detached: false,
  upstream: null,
  ahead: 0,
  behind: 0,
  operation: null,
  files: [],
};

/** Which operation is in flight, so the panel can disable and label its buttons. */
export type GitOp =
  | "fetch"
  | "pull"
  | "push"
  | "commit"
  | "checkout"
  | "discard"
  | "generate"
  | null;

interface GitState {
  /** The folder this status belongs to, so a stale status is never offered. */
  workdir: string | null;
  status: GitStatus;
  branches: Branch[];
  commits: GitCommit[];
  loading: boolean;
  /** Whether `git` can be run at all, resolved once. */
  available: boolean | null;
  /** The operation in flight, if any. */
  op: GitOp;
  /** The last thing that happened, for the panel's confirmation line. */
  notice: string | null;
  error: string | null;

  /** The commit message box. Here rather than in the panel so it survives a
   *  tab switch, which is the same reason a document's text is not component
   *  state. */
  message: string;
  setMessage: (message: string) => void;

  load: (workdir: string | null) => Promise<void>;
  refresh: () => Promise<void>;
  /** Re-reads branches and history, which change less often than status. */
  refreshHistory: () => Promise<void>;

  stage: (paths: string[]) => Promise<void>;
  unstage: (paths: string[]) => Promise<void>;
  discard: (paths: string[]) => Promise<void>;
  commit: () => Promise<boolean>;
  checkout: (name: string) => Promise<void>;
  createBranch: (name: string) => Promise<void>;
  fetch: () => Promise<void>;
  pull: () => Promise<void>;
  push: () => Promise<void>;
  generateMessage: () => Promise<void>;
  clearNotice: () => void;
  clearError: () => void;
  /** Drops state for a folder that is no longer open. */
  reset: () => void;
}

/**
 * The working tree, as this window sees it.
 *
 * A projection, not an owner: git is the truth, and every write here calls the
 * backend and adopts what comes back rather than settling on a guess. A
 * speculative status would have to model what staging does to a two-sided file,
 * which is the sort of reimplementation that is subtly wrong — and wrong in the
 * direction of showing a file as staged when it is not, which is how a commit
 * ends up missing something.
 *
 * ## What drives a refresh
 *
 * Deliberately not a timer. A poll would spawn a `git status` process every few
 * seconds for every open window, on a panel that is often not even visible, and
 * git status on a large working tree is not free. Instead it refreshes on the
 * things that can actually change the answer:
 *
 * - a workdir change, or a window regaining focus (the user may have committed
 *   in a terminal while away);
 * - `loom://fs`, which fires for a save, a checkout, a pull, a discard, a
 *   rename, a create and a delete — and for a tool the model ran;
 * - a finished commit or branch operation;
 * - opening the panel.
 *
 * That list is exhaustive rather than merely typical: everything that can move
 * the working tree either goes through this store, through `useEditor`, or
 * through the engine's `FilesChanged`.
 */
export const useGit = create<GitState>((set, get) => {
  /**
   * Runs an operation, holding the panel's buttons while it is in flight.
   *
   * `describe` is optional because most operations have already put their own
   * sentence in `notice` — a fetch's summary, a commit's id — and only the ones
   * whose result is just a status need a line written for them here.
   */
  const run = async (
    op: GitOp,
    action: () => Promise<GitStatus | null>,
    describe?: (status: GitStatus | null) => string | null,
  ) => {
    if (get().op) return;
    set({ op, error: null });
    try {
      const status = await action();
      if (status) set({ status });
      const notice = describe?.(status) ?? null;
      if (notice) set({ notice });
    } catch (cause) {
      set({ error: cause instanceof Error ? cause.message : String(cause) });
    } finally {
      set({ op: null });
    }
  };

  return {
    workdir: null,
    status: EMPTY,
    branches: [],
    commits: [],
    loading: false,
    available: null,
    op: null,
    notice: null,
    error: null,
    message: "",

    setMessage: (message) => set({ message }),

    load: async (workdir) => {
      // Claim the folder before awaiting, so two callers cannot both walk it —
      // and so a status for the previous folder is not briefly shown against
      // this one.
      if (workdir !== get().workdir) {
        set({ workdir, status: EMPTY, branches: [], commits: [], message: "", error: null, notice: null });
      }
      if (!workdir) {
        set({ loading: false });
        return;
      }

      const available = get().available ?? (await ipc.gitAvailable());
      set({ available: available ?? false, loading: true });
      if (!available) {
        set({ loading: false, status: EMPTY });
        return;
      }

      try {
        const status = await ipc.gitStatus(workdir);
        if (get().workdir !== workdir) return;
        set({ status: status ?? EMPTY, loading: false });
        if (status?.isRepo) void get().refreshHistory();
      } catch (cause) {
        if (get().workdir !== workdir) return;
        set({
          loading: false,
          error: cause instanceof Error ? cause.message : String(cause),
        });
      }
    },

    refresh: async () => {
      const { workdir } = get();
      if (!workdir) return;
      try {
        const status = await ipc.gitStatus(workdir);
        if (get().workdir !== workdir) return;
        if (status) set({ status });
      } catch {
        // A refresh is background work: a failure here must not replace a
        // working panel with an error banner. The next real action will surface
        // whatever is wrong, with a message that means something.
      }
    },

    refreshHistory: async () => {
      const { workdir } = get();
      if (!workdir) return;
      const [branches, commits] = await Promise.all([
        ipc.gitBranches(workdir),
        ipc.gitLog(workdir, 30),
      ]);
      if (get().workdir !== workdir) return;
      if (branches) set({ branches });
      if (commits) set({ commits });
    },

    // No `describe`: staging is its own feedback — the file moves between the
    // two lists, which is louder than any sentence would be.
    //
    // The `workdir` guard is why these are async rather than one-liners: every
    // IPC call takes the folder explicitly, so a status can never be fetched for
    // one workspace and applied to another.
    stage: (paths) =>
      run("commit", async () => {
        const { workdir } = get();
        return workdir ? await ipc.gitStage(workdir, paths) : null;
      }),

    unstage: (paths) =>
      run("commit", async () => {
        const { workdir } = get();
        return workdir ? await ipc.gitUnstage(workdir, paths) : null;
      }),

    // The one destructive action in the panel, and the one the UI always
    // confirms. Nothing here asks: a command cannot show a card, and the panel
    // owning the card is what keeps the question next to the button that asks
    // it.
    discard: (paths) =>
      run(
        "discard",
        async () => {
          const { workdir } = get();
          return workdir ? await ipc.gitDiscard(workdir, paths) : null;
        },
        (status) =>
          status
            ? `Discarded changes to ${paths.length} ${
                paths.length === 1 ? "file" : "files"
              }.`
            : null,
      ),

    commit: async () => {
      const { workdir, message, status } = get();
      if (!workdir) return false;
      if (!message.trim()) {
        set({ error: "A commit needs a message." });
        return false;
      }
      if (!status.files.some((file) => file.staged)) {
        set({ error: "Nothing is staged. Stage a file first." });
        return false;
      }

      set({ op: "commit", error: null });
      try {
        const result = await ipc.gitCommit(workdir, message);
        if (result) {
          set({
            message: "",
            notice: `Committed ${result.id} — ${result.files} ${
              result.files === 1 ? "file" : "files"
            }.`,
          });
        }
        const fresh = await ipc.gitStatus(workdir);
        if (fresh) set({ status: fresh });
        void get().refreshHistory();
        // A commit changes what every open file's diff against HEAD looks like,
        // and the editor may be showing a diff tab that is now out of date.
        useEditor.getState().onExternalChange();
        set({ op: null });
        return true;
      } catch (cause) {
        set({
          op: null,
          error: cause instanceof Error ? cause.message : String(cause),
        });
        return false;
      }
    },

    checkout: (name) =>
      run(
        "checkout",
        async () => {
          const { workdir } = get();
          return workdir ? await ipc.gitCheckout(workdir, name) : null;
        },
        () => `Switched to ${name}.`,
      ),

    createBranch: (name) =>
      run(
        "checkout",
        async () => {
          const { workdir } = get();
          return workdir ? await ipc.gitCreateBranch(workdir, name, true) : null;
        },
        () => `Created and switched to ${name}.`,
      ),

    fetch: () =>
      run("fetch", async () => {
        const { workdir } = get();
        const summary = workdir ? await ipc.gitFetch(workdir) : null;
        set({ notice: summary ?? "Fetched." });
        // The ahead/behind counts come from the status, so re-read it: the whole
        // point of a fetch is that those numbers change.
        return workdir ? await ipc.gitStatus(workdir) : null;
      }),

    pull: () =>
      run("pull", async () => {
        const { workdir } = get();
        const summary = workdir ? await ipc.gitPull(workdir) : null;
        set({ notice: summary ?? "Already up to date." });
        // A pull rewrites files, so every open buffer is potentially stale — the
        // same signal a tool call produces, for the same reason.
        useEditor.getState().onExternalChange();
        return workdir ? await ipc.gitStatus(workdir) : null;
      }),

    push: () =>
      run("push", async () => {
        const { workdir } = get();
        const summary = workdir ? await ipc.gitPush(workdir) : null;
        set({ notice: summary ?? "Pushed." });
        return workdir ? await ipc.gitStatus(workdir) : null;
      }),

    /**
     * Asks the model for a commit message and puts it in the box.
     *
     * Placed in the box rather than committed directly, and that is the whole
     * point: a generated message is a draft the user judges and usually edits.
     * One that committed on its own would make the button a thing you cannot
     * safely press.
     */
    generateMessage: async () => {
      const { workdir, status } = get();
      if (!workdir || get().op) return;

      // The *chat's* model, not the app default's. The backend resolves the
      // provider, model id and variant from this id, which is the only way a
      // chat switched to another model gets its own commit messages written by
      // it — reading `config.chat.*` here was the bug.
      const sessionId = useChat.getState().activeId;
      if (!sessionId) {
        set({ error: "Open a chat first — the message is written by its model." });
        return;
      }

      set({ op: "generate", error: null });
      try {
        const anyStaged = status.files.some((file) => file.staged);
        const message = await ipc.draftCommitMessage(sessionId, anyStaged);
        if (message) set({ message });
      } catch (cause) {
        set({ error: cause instanceof Error ? cause.message : String(cause) });
      } finally {
        set({ op: null });
      }
    },

    clearNotice: () => set({ notice: null }),
    clearError: () => set({ error: null }),
    reset: () =>
      set({
        workdir: null,
        status: EMPTY,
        branches: [],
        commits: [],
        message: "",
        notice: null,
        error: null,
      }),
  };
});

/** Whether a path is currently listed as changed. Used by the file tree. */
export function statusFor(status: GitStatus, path: string) {
  if (!status.isRepo) return null;
  return status.files.find((file) => file.path === path) ?? null;
}

/**
 * Reads the folder's git state on demand, for the workspace chip.
 *
 * A plain function rather than a store action, because the chip wants one fact
 * — the branch — and does not want to subscribe to a working tree it never
 * renders.
 */
export async function branchFor(workdir: string | null): Promise<string | null> {
  if (!workdir) return null;
  const info = await ipc.workspaceInfo(workdir);
  return info?.branch ?? null;
}

/** True when a tab path is a diff rather than a file. Re-exported for callers
 *  that already import this store and would otherwise need `fileTree` too. */
export { isDiffPath };

/** Settings the panel reads, kept in one place so the component stays thin. */
export function useCommitSettings() {
  return useSettings((state) => state.config.commit);
}

/** A one-line description of a file's state, for the panel's rows. */
export function statusLabel(file: {
  conflicted: boolean;
  untracked: boolean;
  staged: boolean;
}): string {
  if (file.conflicted) return "Conflicted";
  if (file.untracked) return "Untracked";
  if (file.staged) return "Staged";
  return "Modified";
}
