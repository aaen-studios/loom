import { create } from "zustand";
import { ipc } from "../lib/ipc";

interface WorkspaceFilesState {
  /** Folder the list belongs to, so a stale list is never offered. */
  workdir: string | null;
  files: string[];
  loading: boolean;
  error: string | null;
  /** Loads `workdir`'s files once; a second call for the same folder is free. */
  load: (workdir: string | null) => Promise<void>;
}

/**
 * The workspace's file list, for the composer's `@` picker.
 *
 * Cached per folder rather than fetched per keystroke: walking a tree is cheap
 * but not free, and typing `@src` would otherwise run it six times. The list is
 * keyed by the folder it came from, so switching chats cannot offer one
 * workspace's paths for another's.
 */
export const useWorkspaceFiles = create<WorkspaceFilesState>((set, get) => ({
  workdir: null,
  files: [],
  loading: false,
  error: null,

  load: async (workdir) => {
    if (!workdir) {
      set({ workdir: null, files: [], loading: false, error: null });
      return;
    }
    const state = get();
    if (state.workdir === workdir) return;
    if (state.loading && state.workdir === workdir) return;

    // Claim the folder before awaiting, so two keystrokes in flight cannot both
    // start a walk.
    set({ workdir, files: [], loading: true, error: null });
    try {
      const files = await ipc.listWorkspaceFiles(workdir);
      // A later load may have moved on while this one was in flight.
      if (get().workdir !== workdir) return;
      set({ files: files ?? [], loading: false });
    } catch (cause) {
      if (get().workdir !== workdir) return;
      set({
        files: [],
        loading: false,
        error: cause instanceof Error ? cause.message : String(cause),
      });
    }
  },
}));
