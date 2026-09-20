import { create } from "zustand";
import {
  ipc,
  type BrowserDownload,
  type BrowserProfile,
  type BrowserSlot,
  type BrowserTab,
  type BrowserWire,
} from "../lib/ipc";
import { isTauri } from "../lib/tauri";

const EMPTY: BrowserWire = { tabs: [], downloads: [] };

/** One entry in the activity rail: what the model did, and where. */
export interface BrowserActivity {
  id: string;
  /** The tool name, or "you" for the user's own action. */
  actor: string;
  summary: string;
  url: string;
  sessionId: string | null;
  at: number;
}

interface BrowserState {
  /**
   * The tabs, as the shell reports them.
   *
   * A projection, not an owner. The tabs are child webviews the process holds,
   * and a panel can be torn off into a second window — so the truth lives in
   * Rust and arrives on `loom://browser`, exactly as the dock's layout does. The
   * consequence worth stating: unmounting this panel destroys nothing.
   */
  tabs: BrowserTab[];
  downloads: BrowserDownload[];
  loaded: boolean;
  /** The activity rail, newest last. View state, so it is not persisted. */
  activity: BrowserActivity[];
  /** The window label this panel is rendered in. Every slot report is for it. */
  host: string | null;
  railOpen: boolean;

  applyRemote: (wire: BrowserWire) => void;
  load: () => Promise<void>;
  setHost: (host: string | null) => void;
  setRailOpen: (open: boolean) => void;

  open: (url: string, profile?: BrowserProfile, sessionId?: string) => Promise<void>;
  close: (id: number) => Promise<void>;
  focus: (id: number, sessionId?: string | null) => Promise<void>;
  navigate: (
    id: number,
    action: "goto" | "back" | "forward" | "reload" | "stop",
    url?: string,
  ) => Promise<void>;
  /** Reports where the page goes; `null` parks every tab in this host. */
  reportSlot: (active: number | null, slot: BrowserSlot | null) => void;
  note: (entry: Omit<BrowserActivity, "id" | "at">) => void;
  clearActivity: () => void;
}

/** Resolves "the window this panel is in", so a torn-off panel reports itself. */
async function currentHost(): Promise<string | null> {
  if (!isTauri) return null;
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    return getCurrentWindow().label;
  } catch {
    return null;
  }
}

export const useBrowser = create<BrowserState>((set, get) => ({
  tabs: EMPTY.tabs,
  downloads: EMPTY.downloads,
  loaded: false,
  activity: [],
  host: null,
  railOpen: true,

  applyRemote: (wire) =>
    set({
      tabs: wire.tabs ?? [],
      downloads: wire.downloads ?? [],
      loaded: true,
    }),

  load: async () => {
    const wire = await ipc.browserTabs();
    if (wire) {
      set({ tabs: wire.tabs, downloads: wire.downloads, loaded: true });
    }
    // The host is resolved once: it is the label of the window this panel is
    // rendered in, and it is what every slot report is addressed to.
    if (!get().host) {
      const host = await currentHost();
      if (host) set({ host });
    }
  },

  setHost: (host) => set({ host }),
  setRailOpen: (open) => set({ railOpen: open }),

  open: async (url, profile = "normal", sessionId) => {
    const host = get().host ?? (await currentHost()) ?? null;
    if (host && !get().host) set({ host });
    const tab = await ipc.browserOpenTab({ url, host, profile, sessionId });
    if (tab) {
      // The shell broadcasts the list, but a panel that opened a tab should not
      // wait a round trip to show it.
      set((state) => ({ tabs: [...state.tabs.filter((t) => t.id !== tab.id), tab] }));
      get().note({
        actor: "you",
        summary: profile === "ghost" ? "opened a private tab" : "opened",
        url: tab.url,
        sessionId: sessionId ?? null,
      });
    }
  },

  close: async (id) => {
    const tab = get().tabs.find((entry) => entry.id === id);
    set((state) => ({ tabs: state.tabs.filter((entry) => entry.id !== id) }));
    void ipc.browserCloseTab(id);
    if (tab) {
      get().note({
        actor: "you",
        summary: "closed",
        url: tab.url,
        sessionId: tab.sessionId,
      });
    }
  },

  focus: async (id, sessionId) => {
    set((state) => ({
      tabs: state.tabs.map((entry) => ({ ...entry, active: entry.id === id })),
    }));
    await ipc.browserFocusTab(id, sessionId ?? null);
  },

  navigate: async (id, action, url) => {
    const tab = get().tabs.find((entry) => entry.id === id);
    await ipc.browserNavigate({ id, action, url: url ?? null });
    if (url && tab) {
      set((state) => ({
        tabs: state.tabs.map((entry) =>
          entry.id === id ? { ...entry, url, loading: true } : entry,
        ),
      }));
      get().note({ actor: "you", summary: "went to", url, sessionId: tab.sessionId });
    }
  },

  /*
   * The slot report is fired from a ResizeObserver, so it re-runs on every frame
   * of a drag and every window move. It is deliberately *not* stored in this
   * store: a value that changes 60 times a second would re-render the tab strip
   * and the rail on every frame for a rectangle neither of them shows.
   */
  reportSlot: (active, slot) => {
    const host = get().host;
    if (!host) return;
    void ipc.browserSetSlot({ host, active, slot });
  },

  note: (entry) =>
    set((state) => ({
      activity: [
        ...state.activity.slice(-499),
        { ...entry, id: crypto.randomUUID(), at: Date.now() },
      ],
    })),

  clearActivity: () => set({ activity: [] }),
}));
