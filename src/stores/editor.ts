import { create } from "zustand";
import { ipc } from "../lib/ipc";
import {
  ancestorsOf,
  isDiffPath,
  languageFor,
  parseDiffPath,
  tabLabel,
  unusedName,
} from "../lib/fileTreeOperations";
import type { LineEnding, SaveOutcome } from "../types";

/** One open tab, and everything the editor needs to draw and save it. */
export interface EditorTab {
  /** Workspace-relative path, or a `diff:` pseudo-path. */
  path: string;
  /** The name shown on the tab. */
  label: string;
  /** Monaco's language id. */
  language: string;
  /** True while the file's first read is in flight. */
  loading: boolean;
  /** Set when the read failed, so the tab can show why instead of a blank pane. */
  error: string | null;

  /** The text as loaded, and what the write guard checks against. */
  hash: string;
  /** Line ending to write back, matching the file's own. */
  eol: LineEnding;
  /** Whether the file began with a byte order mark. */
  bom: boolean;
  readOnly: boolean;
  /** The file was not valid UTF-8. Writable still, but worth saying so. */
  lossy: boolean;

  /**
   * The freshness hint for the version this buffer was loaded from.
   *
   * A cheap "has this plausibly changed" — size and mtime together — so the
   * per-tool-call check is one string compare rather than a hash of every open
   * file. Only a buffer whose hint differs pays for the exact comparison, and
   * the hint never decides anything on its own: a difference means "look
   * closer", never "reload".
   */
  hashHint: string;

  /** The buffer differs from what is on disk. */
  dirty: boolean;
  /**
   * The file changed underneath a dirty buffer.
   *
   * A conflict is never resolved automatically in this direction: the user has
   * keystrokes that exist nowhere else, so the choice is theirs. The other
   * direction — clean buffer, file changed — reloads silently.
   */
  conflict: boolean;
  /** The hash on disk at the moment the conflict was noticed. */
  conflictHash: string | null;
  /** When that version was written, in unix milliseconds. */
  conflictAt: number;
}

interface EditorState {
  /** The folder these tabs belong to. A change closes them all. */
  workdir: string | null;
  tabs: EditorTab[];
  activePath: string | null;
  /** Folders expanded in the tree, as `/`-separated paths. */
  expanded: string[];
  /** The tree's selection, which is not the same as the open tab. */
  selected: string | null;
  /** Set when a file operation fails, for the tree's inline error line. */
  treeError: string | null;

  /** Points the editor at a folder; closes everything when it changes. */
  setWorkdir: (workdir: string | null) => void;
  /** Opens a file, or focuses its tab when it is already open. */
  open: (path: string) => Promise<void>;
  /** Opens a diff of one file, as a tab. */
  openDiff: (file: string, staged: boolean) => void;
  close: (path: string) => void;
  closeAll: () => void;
  closeOthers: (path: string) => void;
  setActive: (path: string) => void;

  /** Records the text typed into a buffer. */
  setDirty: (path: string, dirty: boolean) => void;
  /** Saves one buffer. Returns false when the write was refused. */
  save: (path: string, text: string, force?: boolean) => Promise<boolean>;
  saveAll: () => Promise<void>;

  /**
   * Re-stats every open buffer after something might have changed on disk.
   *
   * The single entry point for that question, called from `loom://fs`, from the
   * engine's `FilesChanged`, and from a window regaining focus. One function
   * means one place that decides between "reload quietly" and "raise a
   * conflict", which is the decision that matters and is easy to get wrong in
   * two places.
   */
  recheck: () => Promise<void>;
  /** Same, for callers that know the file has certainly changed. */
  onExternalChange: () => void;
  /** The conflict banner's two choices. */
  resolveConflict: (path: string, choice: "reload" | "keep") => Promise<void>;

  toggleFolder: (path: string) => void;
  expandTo: (path: string) => void;
  setSelected: (path: string | null) => void;
  setTreeError: (error: string | null) => void;
}

/**
 * Empty array, hoisted.
 *
 * A `?? []` inside a selector hands `useSyncExternalStore` a brand-new array on
 * every read, so it reports "changed" forever and React throws "Maximum update
 * depth exceeded". `scripts/audit-selectors.mjs` fails the build on exactly this
 * shape, and this is the documented fix.
 */
export const NO_TABS: EditorTab[] = [];

/**
 * The open buffers.
 *
 * ## What this store does not own
 *
 * The Monaco models. They live in `lib/editors.ts`, outside React, because a
 * panel switch unmounts the editor and a model in component state would be
 * destroyed with unsaved edits in it. What is here is the *description* of each
 * buffer — its hash, its line ending, whether it is dirty — which is small,
 * plain data that React can subscribe to freely.
 *
 * ## Why autosave is safe
 *
 * A save sends the hash of what was loaded, and `edit::write_text` refuses if
 * the file has moved on, returning a conflict instead of writing. That refusal
 * is the entire reason autosave can run without being asked: it cannot discard
 * work, because it will not write over a version it has not seen. The conflict
 * then waits for the user rather than resolving itself — the one direction an
 * editor must never guess in.
 */
export const useEditor = create<EditorState>((set, get) => {
  /** Replaces one tab, leaving the rest alone. */
  const patch = (path: string, change: Partial<EditorTab>) =>
    set((state) => ({
      tabs: state.tabs.map((tab) =>
        tab.path === path ? { ...tab, ...change } : tab,
      ),
    }));

  const find = (path: string) => get().tabs.find((tab) => tab.path === path) ?? null;

  /** A tab for a diff pseudo-path. No file is read; the view fetches both sides. */
  const diffTab = (path: string): EditorTab => ({
    path,
    label: tabLabel(path),
    language: "diff",
    loading: false,
    error: null,
    hash: "",
    hashHint: "",
    eol: "lf",
    bom: false,
    // Read-only in the sense that matters: a diff has no "save". Marking it
    // read-only is what stops the autosave pass and the save key from treating
    // it as a buffer with content to write back to a path that is not a file.
    readOnly: true,
    lossy: false,
    dirty: false,
    conflict: false,
    conflictHash: null,
    conflictAt: 0,
  });

  return {
    workdir: null,
    tabs: [],
    activePath: null,
    expanded: [],
    selected: null,
    treeError: null,

    setWorkdir: (workdir) => {
      if (workdir === get().workdir) return;
      // Tabs do not survive a folder change, and should not: a relative path
      // means something different in another repository, so keeping them would
      // be offering to save one project's file under another's name.
      set({
        workdir,
        tabs: [],
        activePath: null,
        expanded: [],
        selected: null,
        treeError: null,
      });
    },

    open: async (path) => {
      const existing = find(path);
      if (existing) {
        set({ activePath: path, selected: path });
        // A tab that failed earlier gets another chance, because the failure may
        // have been the file not existing yet.
        if (existing.error) await get().open(path);
        return;
      }

      if (isDiffPath(path)) {
        set((state) => ({
          tabs: [...state.tabs, diffTab(path)],
          activePath: path,
        }));
        return;
      }

      const tab: EditorTab = {
        path,
        label: tabLabel(path),
        language: languageFor(path),
        loading: true,
        error: null,
        hash: "",
        hashHint: "",
        eol: "lf",
        bom: false,
        readOnly: false,
        lossy: false,
        dirty: false,
        conflict: false,
        conflictHash: null,
        conflictAt: 0,
      };
      set((state) => ({ tabs: [...state.tabs, tab], activePath: path, selected: path }));

      try {
        const file = await ipc.fileRead(get().workdir, path);
        if (!file) throw new Error("the file could not be read");
        // The tab may have been closed while this was in flight.
        if (!find(path)) return;
        patch(path, {
          loading: false,
          hash: file.hash,
          hashHint: file.hashHint,
          eol: file.eol,
          bom: file.bom,
          readOnly: file.readOnly,
          lossy: file.lossy,
        });
        // The text is handed to the model by the panel, which owns the Monaco
        // instance — see `EditorPane`. Nothing async writes into a model here,
        // so a read that lands after the pane unmounted simply updates the
        // description and the next mount reads it.
      } catch (cause) {
        patch(path, {
          loading: false,
          error: cause instanceof Error ? cause.message : String(cause),
        });
      }
    },

    openDiff: (file, staged) => {
      const path = `diff:${staged ? "staged/" : "worktree/"}${file}`;
      if (find(path)) {
        set({ activePath: path });
        return;
      }
      set((state) => ({ tabs: [...state.tabs, diffTab(path)], activePath: path }));
    },

    close: (path) =>
      set((state) => {
        const index = state.tabs.findIndex((tab) => tab.path === path);
        if (index === -1) return state;
        const tabs = state.tabs.filter((tab) => tab.path !== path);
        // Focus moves to the neighbour, the way every editor does it: the one to
        // the right if there is one, otherwise the one to the left.
        const active =
          state.activePath === path
            ? (tabs[index] ?? tabs[index - 1] ?? null)?.path ?? null
            : state.activePath;
        return { tabs, activePath: active };
      }),

    closeAll: () => set({ tabs: [], activePath: null }),

    closeOthers: (path) =>
      set((state) => ({
        tabs: state.tabs.filter((tab) => tab.path === path),
        activePath: path,
      })),

    setActive: (path) => set({ activePath: path }),

    setDirty: (path, dirty) => {
      const tab = find(path);
      if (!tab || tab.dirty === dirty) return;
      patch(path, { dirty });
    },

    save: async (path, text, force = false) => {
      const { workdir, tabs } = get();
      const tab = tabs.find((entry) => entry.path === path);
      if (!tab || tab.readOnly || isDiffPath(path)) return false;

      try {
        const outcome: SaveOutcome | null = await ipc.fileSave({
          workdir,
          path,
          text,
          // `force` is what the conflict banner's "Keep mine" passes: it is the
          // user saying they know the file moved on and want their version
          // written anyway.
          expectedHash: force ? null : tab.hash,
          eol: tab.eol,
          bom: tab.bom,
        });
        if (!outcome) return false;

        if (outcome.kind === "conflict") {
          patch(path, {
            conflict: true,
            conflictHash: outcome.currentHash,
            conflictAt: outcome.modified,
          });
          return false;
        }

        patch(path, {
          hash: outcome.hash,
          // The hint comes from the backend, which stat'd the file it just
          // wrote. Deriving it here instead — `Date.now()` against a value the
          // backend computes from the filesystem's mtime — is what made every
          // save look like an external change: the two clocks never agree, so
          // the next `recheck` saw a mismatch and, on a dirty buffer, raised a
          // conflict on the file Loom had written itself a moment earlier.
          hashHint: outcome.hashHint,
          dirty: false,
          conflict: false,
          conflictHash: null,
          conflictAt: 0,
        });
        return true;
      } catch (cause) {
        patch(path, {
          error: cause instanceof Error ? cause.message : String(cause),
        });
        return false;
      }
    },

    /**
     * Saves every dirty buffer.
     *
     * The text comes from the Monaco models rather than from store state,
     * because the store deliberately does not hold buffer text — so this reaches
     * into `lib/editors` for it. That is the one place the two have to meet, and
     * keeping it to one function is why the split stays comprehensible.
     */
    saveAll: async () => {
      const { tabs } = get();
      const editors = await import("../lib/editors");
      for (const tab of tabs) {
        if (!tab.dirty || tab.readOnly || isDiffPath(tab.path)) continue;
        const model = editors.modelFor(tab.path);
        if (!model) continue;
        await get().save(tab.path, model.getValue());
      }
    },

    recheck: async () => {
      const { workdir, tabs } = get();
      if (!workdir) return;
      const open = tabs.filter((tab) => !isDiffPath(tab.path));
      if (open.length === 0) return;

      const stats = await ipc.fileStatMany(
        workdir,
        open.map((tab) => tab.path),
      );
      if (!stats) return;

      const byPath = new Map(stats.map((stat) => [stat.path, stat]));

      for (const tab of open) {
        const stat = byPath.get(tab.path);
        if (!stat) continue;
        if (!stat.exists) continue;

        // Cheap freshness gate first: an exact hash compare means reading the
        // whole file, and this runs on every tool call and every window focus.
        // Only a file that looks different is worth the read.
        if (tab.conflict) continue;
        const maybeChanged = stat.hashHint !== tab.hashHint;
        if (!maybeChanged) continue;

        if (tab.dirty) {
          // The direction that matters: their keystrokes exist nowhere else, so
          // this is a question rather than an action.
          patch(tab.path, { conflict: true, conflictAt: stat.modified });
        } else {
          await get().resolveConflict(tab.path, "reload");
        }
      }
    },

    onExternalChange: () => {
      // A deliberate nudge rather than a signature: the caller knows something
      // happened and does not know what. Debounced by a microtask so a tool that
      // wrote ten files does not restat ten times.
      queueMicrotask(() => {
        void get().recheck();
      });
    },

    resolveConflict: async (path, choice) => {
      const tab = find(path);
      if (!tab) return;

      if (choice === "keep") {
        // The user has decided: write their version over what arrived.
        const editors = await import("../lib/editors");
        const model = editors.modelFor(path);
        if (!model) return;
        await get().save(path, model.getValue(), true);
        return;
      }

      // Reload: re-read and adopt what is on disk. The buffer goes back to
      // clean, and whatever was typed is gone — which is why the button says
      // "Reload" and not "Resolve", and why this is the user's choice.
      try {
        const file = await ipc.fileRead(get().workdir, path);
        if (!file) return;
        // The text goes into the model first, then the description is updated.
        // In that order on purpose: `lib/editors` owns the buffer, and this
        // store owns the hash that a save is checked against, so updating the
        // hash before the text would briefly describe a buffer that does not
        // exist yet.
        const editors = await import("../lib/editors");
        editors.setText(path, file.text);
        patch(path, {
          hash: file.hash,
          hashHint: file.hashHint,
          eol: file.eol,
          bom: file.bom,
          readOnly: file.readOnly,
          lossy: file.lossy,
          dirty: false,
          conflict: false,
          conflictHash: null,
          conflictAt: 0,
          error: null,
        });
      } catch (cause) {
        patch(path, {
          error: cause instanceof Error ? cause.message : String(cause),
        });
      }
    },

    toggleFolder: (path) =>
      set((state) => ({
        expanded: state.expanded.includes(path)
          ? state.expanded.filter((entry) => entry !== path)
          : [...state.expanded, path],
      })),

    expandTo: (path) =>
      set((state) => {
        const needed = ancestorsOf(path);
        const merged = new Set([...state.expanded, ...needed]);
        return { expanded: [...merged] };
      }),

    setSelected: (selected) => set({ selected }),
    setTreeError: (treeError) => set({ treeError }),
  };
});

/**
 * The tree's own view state, apart from the buffers.
 *
 * Apart because it changes for different reasons and at a different rate: the
 * expanded set changes when you click a chevron, the buffers change when the
 * agent writes a file. Mixing them would put every tab on the re-render path of
 * every folder toggle.
 */
interface TreeState {
  /** Directory listings, keyed by folder path, filled on first expand. */
  listings: Record<string, import("../lib/ipc").TreeEntry[]>;
  loading: string[];
  load: (workdir: string | null, path: string, force?: boolean) => Promise<void>;
  /** Drops the cached listing for a folder and its parent, after a change. */
  invalidate: (path: string) => void;
  reset: () => void;
}

export const useTree = create<TreeState>((set, get) => ({
  listings: {},
  loading: [],

  load: async (workdir, path, force = false) => {
    if (!workdir) return;
    if (!force && get().listings[path]) return;
    if (get().loading.includes(path)) return;
    set((state) => ({ loading: [...state.loading, path] }));
    try {
      const entries = await ipc.dirList(workdir, path);
      set((state) => ({
        listings: { ...state.listings, [path]: entries ?? [] },
        loading: state.loading.filter((entry) => entry !== path),
      }));
    } catch (cause) {
      set((state) => ({
        loading: state.loading.filter((entry) => entry !== path),
      }));
      useEditor
        .getState()
        .setTreeError(cause instanceof Error ? cause.message : String(cause));
    }
  },

  invalidate: (path) =>
    set((state) => {
      // The folder itself and its parent: creating or deleting `a/b.txt`
      // changes both `a` and the level above.
      const parent = path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
      const next = { ...state.listings };
      delete next[path];
      delete next[parent];
      return { listings: next };
    }),

  reset: () => set({ listings: {}, loading: [] }),
}));

/** A name for a new entry in a folder, avoiding what is already listed. */
export function suggestedName(
  entries: import("../lib/ipc").TreeEntry[],
  isDir: boolean,
): string {
  return unusedName(entries, isDir ? "new-folder" : "untitled");
}

/** Re-exported so a component can ask without importing two modules. */
export { parseDiffPath };
