import { create } from "zustand";
import { ZONE_SIZE, clampStoredSize } from "../lib/dockGeometry";
import { ipc } from "../lib/ipc";
import { isTauri } from "../lib/tauri";
import type { DockEdge, DockLayout, DockZone } from "../types";
import { useSettings } from "./settings";

const EMPTY: DockLayout = { zones: [], shell: null };

interface DockState {
  /** The projected arrangement. Authoritative values come from the backend. */
  layout: DockLayout;
  /** Which folder this arrangement belongs to; `null` is the default. */
  workdir: string | null;
  loaded: boolean;

  load: (workdir: string | null) => Promise<void>;
  applyRemote: (workdir: string | null, layout: DockLayout) => void;

  toggleZone: (id: string) => void;
  /**
   * Resizes a zone locally, without persisting.
   *
   * Called once per pointermove during a resize drag. Writing the config on
   * every frame would lag behind the pointer, so the store leads and `persist`
   * catches up when the drag ends — the one place local state is allowed to run
   * ahead of the backend.
   */
  resizeZone: (id: string, size: number) => void;
  /** Persists the current layout. Called once, when a drag ends. */
  persist: () => void;
  /** The one keystroke toggle: closes every zone, or opens the primary one. */
  toggleDock: () => void;
  /**
   * Moves a panel onto an edge, creating a zone there if that edge has none.
   *
   * This is what a drag onto the window edge means. Routing it through
   * `openPanel` would be wrong: that reuses the *first* zone when no edge
   * matches, so dropping a tab on the bottom edge would silently move it to the
   * left dock.
   */
  dropOnEdge: (panel: string, edge: DockEdge, index: number) => void;
  /** Closes every open zone, keeping their panels and sizes. */
  collapseAll: () => void;
  openPanel: (panel: string, edge?: DockEdge) => void;
  closePanel: (zoneId: string, panel: string) => void;
  movePanel: (panel: string, zoneId: string, index: number) => void;
  setActive: (zoneId: string, index: number) => void;
  setShell: (shell: string | null) => void;
}

/**
 * The dock's arrangement.
 *
 * A projection, not an owner. Rust holds the truth (see `dock.rs` for why), so
 * every change here writes through and then waits for the broadcast rather than
 * settling on its own value — with one exception: a live drag updates this
 * store immediately and skips the write until the pointer is released. That is
 * the only place local state is allowed to lead, and it is what makes resizing
 * feel direct.
 */
export const useDock = create<DockState>((set, get) => {
  /** Writes the current layout to the backend, ignoring failures. */
  const write = (layout: DockLayout) => {
    if (!isTauri) return;
    const { workdir } = get();
    void ipc.setDockLayout(workdir, layout).then((updated) => {
      // The config carries the resolved layout, so the store can settle
      // exactly on what the backend stored rather than on what it guessed.
      if (updated) useSettings.getState().applyRemote(updated);
    });
  };

  /**
   * Brings a layout from a config file inside the band the UI can render.
   *
   * Rust clamps on every write, so this only ever fires for a `config.json`
   * edited by hand — but a size of `99999` there would paint a zone wider than
   * the window, and a size of `0` a strip with no header. Cheap to guard, and
   * the alternative is a broken-looking window with no obvious cause.
   */
  const sanitize = (layout: DockLayout): DockLayout => ({
    ...layout,
    zones: layout.zones.map((zone) => ({
      ...zone,
      size: clampStoredSize(zone.size),
    })),
  });

  /** Applies a change locally and writes it. */
  const commit = (change: (layout: DockLayout) => DockLayout) => {
    const layout = change(get().layout);
    set({ layout });
    write(layout);
  };

  return {
    layout: EMPTY,
    workdir: null,
    loaded: false,

    load: async (workdir) => {
      // A config load may already hold this folder's layout; the explicit
      // fetch is what makes a torn-off window come up correct on its own.
      const remote = await ipc.dockLayout(workdir);
      if (remote) {
        set({ layout: remote, workdir, loaded: true });
        return;
      }
      // In a plain browser there is no backend, so fall back to the config's
      // default and let the UI still be usable.
      const config = useSettings.getState().config;
      const fallback = (workdir ? config.dock[workdir] : null) ?? config.dockDefault;
      set({ layout: sanitize(fallback ?? EMPTY), workdir, loaded: true });
    },

    applyRemote: (workdir, layout) => {
      // A broadcast for a folder this window is not showing is not this
      // window's business; applying it would move a torn-off panel's dock to
      // the arrangement of whatever folder was last edited elsewhere.
      if (workdir !== get().workdir) return;
      set({ layout });
    },

    toggleZone: (id) =>
      commit((layout) => ({
        ...layout,
        zones: layout.zones.map((zone) =>
          zone.id === id ? { ...zone, open: !zone.open } : zone,
        ),
      })),

    resizeZone: (id, size) =>
      set((state) => ({
        layout: {
          ...state.layout,
          zones: state.layout.zones.map((zone) =>
            zone.id === id ? { ...zone, size } : zone,
          ),
        },
      })),

    // `write` rather than `commit`: the layout is already in the store, so there
    // is nothing to change — only to save. This is the one write a drag makes,
    // however many frames it took.
    persist: () => write(get().layout),

    collapseAll: () =>
      commit((layout) => ({
        ...layout,
        zones: layout.zones.map((zone) => ({ ...zone, open: false })),
      })),

    dropOnEdge: (panel, edge, index) => {
      const { layout } = get();
      const existing = layout.zones.find((zone) => zone.edge === edge);
      if (existing) {
        get().movePanel(panel, existing.id, index);
        return;
      }
      // No zone on that edge yet, so this is a split. The id is derived from the
      // edge and a counter rather than random, so a layout stays readable in
      // `config.json` and two splits do not collide.
      let suffix = 2;
      while (layout.zones.some((zone) => zone.id === `${edge}-${suffix}`)) suffix += 1;
      const zone: DockZone = {
        id: `${edge}-${suffix}`,
        edge,
        // Opens at the edge's own size — see `ZONE_SIZE`. A starting point
        // rather than a fixed size: the resize handle takes it from here.
        size: ZONE_SIZE[edge],
        open: true,
        panels: [panel],
        active: 0,
      };
      // Take the panel out of wherever it was first, then add the new zone.
      let zones = layout.zones.map((entry) => {
        if (!entry.panels.includes(panel)) return entry;
        const panels = entry.panels.filter((p) => p !== panel);
        return {
          ...entry,
          panels,
          open: panels.length === 0 ? false : entry.open,
          active: Math.min(entry.active, Math.max(0, panels.length - 1)),
        };
      });
      zones = [...zones, zone];
      commit((current) => ({ ...current, zones }));
    },

    toggleDock: () => {
      const { layout } = get();
      const open = layout.zones.filter((zone) => zone.open);
      // Every open zone closes together: one keystroke has to mean one thing,
      // and "put everything away" is the useful direction once anything is out.
      if (open.length > 0) {
        commit((current) => ({
          ...current,
          zones: current.zones.map((zone) => ({ ...zone, open: false })),
        }));
        return;
      }
      const target =
        layout.zones.find((zone) => zone.edge === "right") ?? layout.zones[0];
      if (!target) return;
      commit((current) => ({
        ...current,
        zones: current.zones.map((zone) =>
          zone.id === target.id ? { ...zone, open: true } : zone,
        ),
      }));
    },

    openPanel: (panel, edge) => {
      const { layout } = get();
      // Already docked somewhere: just show it, rather than adding a second
      // copy. There is one shell, one runs list, one file tree.
      const home = layout.zones.find((zone) => zone.panels.includes(panel));
      if (home) {
        commit((current) => ({
          ...current,
          zones: current.zones.map((zone) =>
            zone.id === home.id
              ? { ...zone, open: true, active: zone.panels.indexOf(panel) }
              : zone,
          ),
        }));
        return;
      }
      const zone =
        (edge ? layout.zones.find((entry) => entry.edge === edge) : null) ??
        layout.zones[0];
      if (!zone) return;
      void get().movePanel(panel, zone.id, zone.panels.length);
    },

    closePanel: (zoneId, panel) =>
      commit((layout) => ({
        ...layout,
        zones: layout.zones.map((zone) => {
          if (zone.id !== zoneId) return zone;
          const index = zone.panels.indexOf(panel);
          if (index === -1) return zone;
          const panels = zone.panels.filter((entry) => entry !== panel);
          return {
            ...zone,
            panels,
            // Closing the last tab closes the zone: an open zone with nothing
            // in it is a strip of chrome over a gap.
            open: panels.length === 0 ? false : zone.open,
            active:
              panels.length === 0
                ? 0
                : index <= zone.active
                  ? Math.max(0, Math.min(zone.active - 1, panels.length - 1))
                  : zone.active,
          };
        }),
      })),

    movePanel: (panel, zoneId, index) => {
      const { layout } = get();
      if (!layout.zones.some((zone) => zone.id === zoneId)) return;
      const zones: DockZone[] = layout.zones.map((zone) => {
        const position = zone.panels.indexOf(panel);
        if (position === -1) return zone;
        const panels = zone.panels.filter((entry) => entry !== panel);
        return {
          ...zone,
          panels,
          open: panels.length === 0 ? false : zone.open,
          active:
            position === zone.active
              ? Math.min(zone.active, Math.max(0, panels.length - 1))
              : position < zone.active
                ? zone.active - 1
                : zone.active,
        };
      });

      const target = zones.find((zone) => zone.id === zoneId);
      if (!target) return;
      const at = Math.max(0, Math.min(index, target.panels.length));
      const moved = zones.map((zone) =>
        zone.id === zoneId
          ? {
              ...zone,
              panels: [
                ...zone.panels.slice(0, at),
                panel,
                ...zone.panels.slice(at),
              ],
              // Moving a panel to a zone is also asking to see it there.
              active: at,
              open: true,
            }
          : zone,
      );
      commit((current) => ({ ...current, zones: moved }));
    },

    setActive: (zoneId, index) =>
      commit((layout) => ({
        ...layout,
        zones: layout.zones.map((zone) =>
          zone.id === zoneId
            ? { ...zone, active: Math.max(0, Math.min(index, zone.panels.length - 1)) }
            : zone,
        ),
      })),

    setShell: (shell) => commit((layout) => ({ ...layout, shell })),
  };
});

