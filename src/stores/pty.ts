import { create } from "zustand";
import { ipc } from "../lib/ipc";
import type { PtyProfile } from "../types";
import { useDock } from "./dock";

/**
 * The shells.
 *
 * Metadata only — the xterm instances live in `lib/terminals.ts`, because a
 * `Terminal` is a large object with its own DOM and putting it in a store would
 * make every keystroke in a shell re-render whatever subscribes. What belongs
 * here is what the rest of the app needs to know: which shells exist, which one
 * is showing, and which have ended.
 *
 * Output does not pass through this store either. It arrives on `loom://pty` at
 * whatever rate the shell produces it and goes straight to xterm, because a
 * store update per chunk would re-render the tab strip for every prompt redraw.
 */
export interface PtySession {
  id: string;
  /** Profile id it was started with. */
  profile: string | null;
  /** Display name, from the profile the backend found. */
  name: string;
  workdir: string | null;
  alive: boolean;
}

interface OpenArgs {
  workdir: string | null;
  profile?: string | null;
  name?: string;
  forceNew?: boolean;
}

interface PtyState {
  sessions: PtySession[];
  /** The shell whose tab is showing. */
  activeId: string | null;
  /** The shells this machine has, probed once. */
  profiles: PtyProfile[];
  profilesLoaded: boolean;
  /** Set while the first shell of a folder is starting. */
  opening: boolean;
  error: string | null;

  loadProfiles: () => Promise<PtyProfile[]>;
  /** Opens a shell for a folder, reusing the folder's existing one. */
  open: (args: OpenArgs) => Promise<string | null>;
  setActive: (id: string | null) => void;
  close: (id: string) => Promise<void>;
  /** Called by the event listener when a shell ends on its own. */
  markExited: (id: string) => void;
}

/**
 * Session ids, derived from the folder so the same workspace always addresses
 * the same shell.
 *
 * The counter is module-level and monotonic: a closed tab's id is never reused,
 * so a late chunk from a dying shell cannot land in a new one.
 */
let counter = 0;

/**
 * Opens already in flight, keyed by folder.
 *
 * `open` performs an `await` between checking for an existing shell and
 * recording the new one, so two callers that arrive in the same tick both see
 * "no shell yet" and both start one. That is not hypothetical: React
 * `StrictMode` mounts an effect twice, so the dock's "start a shell if the
 * folder has none" effect fired twice and produced **two identical tabs** for
 * one folder.
 *
 * Storing the promise makes the second caller await the first one's result
 * instead of starting a race. `forceNew` opts out, because the `+` button
 * asking twice really does mean two shells.
 */
const inFlight = new Map<string, Promise<string | null>>();

function openKey(workdir: string | null, profile: string | null | undefined): string {
  return `${workdir ?? ""}::${profile ?? ""}`;
}

export const usePty = create<PtyState>((set, get) => ({
  sessions: [],
  activeId: null,
  profiles: [],
  profilesLoaded: false,
  opening: false,
  error: null,

  loadProfiles: async () => {
    if (get().profilesLoaded) return get().profiles;
    const found = (await ipc.ptyProfiles()) ?? [];
    set({ profiles: found, profilesLoaded: true });
    return found;
  },

  open: async (args) => {
    const { workdir, profile, name, forceNew } = args;

    // A folder gets one persistent shell, as asked. "Plus" passes `forceNew`
    // and gets another alongside it.
    if (!forceNew) {
      const existing = get().sessions.find(
        (session) => session.workdir === workdir && session.alive,
      );
      if (existing) {
        set({ activeId: existing.id });
        return existing.id;
      }
      // Somebody else is already starting this folder's shell. Join them
      // rather than starting a second.
      const pending = inFlight.get(openKey(workdir, profile));
      if (pending) return pending;
    }

    const start = async (): Promise<string | null> => {
      const profiles = get().profilesLoaded ? get().profiles : await get().loadProfiles();

      // The folder's remembered shell wins over the machine's default:
      // reopening a project should land in the shell you were using on it.
      const chosen =
        profile ??
        useDock.getState().layout.shell ??
        profiles.find((entry) => entry.default)?.id ??
        profiles[0]?.id ??
        null;
      if (!chosen) {
        set({ error: "No shell is available on this machine." });
        return null;
      }
      const label = name ?? profiles.find((entry) => entry.id === chosen)?.name ?? "Shell";

      // Re-check under the lock: the promise that resolved just before us may
      // have created the very shell we are about to duplicate.
      if (!forceNew) {
        const raced = get().sessions.find(
          (session) => session.workdir === workdir && session.alive,
        );
        if (raced) {
          set({ activeId: raced.id });
          return raced.id;
        }
      }

      counter += 1;
      const id = `sh-${counter}`;
      set({ opening: true, error: null });
      try {
        await ipc.ptyOpen({ id, profile: chosen, workdir });
        set((state) => ({
          sessions: [
            ...state.sessions,
            { id, profile: chosen, name: label, workdir, alive: true },
          ],
          activeId: id,
        }));
        return id;
      } catch (cause) {
        const message = cause instanceof Error ? cause.message : String(cause);
        // A torn-off terminal window can race the main window for the same id.
        // Rather than showing an error, reuse the shell that already exists.
        if (message.includes("already open")) {
          set({ activeId: id });
          return id;
        }
        set({ error: message });
        return null;
      } finally {
        set({ opening: false });
      }
    };

    if (forceNew) return start();

    const key = openKey(workdir, profile);
    const promise = start().finally(() => inFlight.delete(key));
    inFlight.set(key, promise);
    return promise;
  },

  setActive: (activeId) => set({ activeId }),

  close: async (id) => {
    await ipc.ptyClose(id);
    set((state) => {
      const sessions = state.sessions.filter((session) => session.id !== id);
      // Closing the showing tab moves to its neighbour rather than to nothing,
      // which is what makes closing a shell not also close the dock.
      const activeId =
        state.activeId === id ? (sessions[sessions.length - 1]?.id ?? null) : state.activeId;
      return { sessions, activeId };
    });
  },

  markExited: (id) =>
    set((state) => ({
      sessions: state.sessions.map((session) =>
        session.id === id ? { ...session, alive: false } : session,
      ),
    })),
}));
