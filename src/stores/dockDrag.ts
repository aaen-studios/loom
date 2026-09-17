import { create } from "zustand";
import type { DockEdge } from "../types";

/**
 * A panel drag in flight.
 *
 * Deliberately its own store, separate from `useDock`, because the two have
 * completely different update rates. The layout changes when the user lets go;
 * a drag wants to report the pointer sixty times a second. Mixing them would
 * put every panel in the dock on the re-render path of every pointermove, which
 * is exactly why dragging felt slow.
 *
 * Even here, only the *target* is state. The ghost is moved by writing a
 * transform straight onto the element, so a drag across the window renders React
 * a handful of times — once per change of drop target — rather than once per
 * frame.
 */

/** Where a dragged panel would land if released now. */
export type DropTarget =
  | { kind: "zone"; zoneId: string; index: number }
  | { kind: "edge"; edge: DockEdge }
  | { kind: "tearoff" };

/** The drop target's identity, as a string, so a selector can compare cheaply. */
export function targetKey(target: DropTarget | null): string {
  if (!target) return "";
  if (target.kind === "zone") return `zone:${target.zoneId}:${target.index}`;
  if (target.kind === "edge") return `edge:${target.edge}`;
  return "tearoff";
}

/** What the ghost says the release will do. */
export function targetLabel(target: DropTarget | null): string | null {
  if (!target) return null;
  switch (target.kind) {
    case "zone":
      return "Move here";
    case "edge":
      return `Dock to the ${target.edge}`;
    case "tearoff":
      return "Open in its own window";
  }
}

interface DragState {
  /** The panel on the pointer. */
  panel: string | null;
  /** The zone it started in, so a drop back where it began is a no-op. */
  fromZone: string | null;
  /** The current target, or null when the pointer is over nothing useful. */
  target: DropTarget | null;
  /** `targetKey(target)`, so a selector does not allocate on every move. */
  key: string;

  start: (panel: string, fromZone: string) => void;
  /** Returns true when the target actually changed, so callers can skip work. */
  retarget: (target: DropTarget | null) => boolean;
  end: () => void;
}

export const useDockDrag = create<DragState>((set, get) => ({
  panel: null,
  fromZone: null,
  target: null,
  key: "",

  start: (panel, fromZone) => set({ panel, fromZone, target: null, key: "" }),

  retarget: (target) => {
    const key = targetKey(target);
    if (key === get().key) return false;
    set({ target, key });
    return true;
  },

  end: () => set({ panel: null, fromZone: null, target: null, key: "" }),
}));

