import { createContext, useContext, type ReactNode } from "react";
import type { ComponentType } from "react";
import {
  FilesIcon,
  GitBranchIcon,
  GlobeIcon,
  PanelLeftIcon,
  RunsIcon,
  TargetIcon,
  TerminalIcon,
  WrenchIcon,
} from "../components/icons";
import {
  BrowserPanel,
  EditorPanel,
  EditorTabs,
  FilesPanel,
  GitPanel,
  GoalDockPanel,
  RunsPanel,
  SessionsPanel,
} from "../components/dockPanels";
import { TerminalPanel, TerminalTabs } from "../components/TerminalPanel";
import type { DockEdge } from "../types";

/**
 * The panel registry: the one place that knows what a panel is.
 *
 * Rust owns *where* panels go (see `stores/dock.ts` for why) but deliberately
 * knows nothing about *what* one is, so this is the seam that makes adding a
 * panel a frontend-only change. A browser, a diff view, or anything else lands
 * as one entry here plus one component; the zones, the tab strips and the
 * tear-off path all pick it up without being touched.
 *
 * `id` is the wire format: it is what a layout stores and what a torn-off window
 * is launched with, so renaming one is a data migration.
 */
export interface PanelProps {
  /** The active chat's folder, or null. Panels that are not per-folder ignore it. */
  workdir: string | null;
}

/**
 * Whether a panel's own tab strip is currently standing in for the zone's.
 *
 * Needed because the terminal has inner tabs of its own — one per shell — and
 * when the terminal is alone in a zone those *become* the zone's tabs. The panel
 * has to know, or it would draw a second copy of the same row underneath.
 *
 * A context rather than a prop because the panel is rendered as `def.render`
 * deep inside the zone, and threading a flag through would mean `PanelProps`
 * carrying something only one panel cares about — and every panel would then
 * have to decide whether to pass it on.
 */
const StripRendered = createContext(false);

/** Marks the subtree where a panel's own strip has already been drawn. */
export function StripRenderedProvider({
  value,
  children,
}: {
  value: boolean;
  children: ReactNode;
}) {
  return <StripRendered.Provider value={value}>{children}</StripRendered.Provider>;
}

/** True when this panel's tab strip is already on screen above it. */
export function useStripRendered(): boolean {
  return useContext(StripRendered);
}

export interface PanelDef {
  id: string;
  title: string;
  /** Shown in the tab strip and the panels menu. */
  icon: ComponentType<{ size?: number; className?: string }>;
  /** Where a panel goes when nothing has decided yet. */
  edge: DockEdge;
  /** The label for a torn-off window, and its title bar. */
  windowTitle: string;
  render: ComponentType<PanelProps>;
  /**
   * An optional replacement for the zone's tab strip, used *when this panel is
   * the only one in its zone*.
   *
   * The terminal is the reason it exists. It has inner tabs of its own — one per
   * shell — and a panel tab row above a shell tab row is two rows saying almost
   * the same thing. When the terminal is alone in its zone, its shell tabs *are*
   * the zone's tabs, and the `+` that adds a shell is where the `+` for a panel
   * would be.
   *
   * Only when it is alone: with a second panel stacked in the same zone there is
   * no single panel whose tabs could stand in for the zone's, so the normal row
   * comes back and the shells move inside the panel.
   *
   * Outside a zone — a torn-off window — the panel draws its own strip instead,
   * which is what `standalone` below is for.
   */
  tabStrip?: ComponentType<PanelProps & { standalone?: boolean }>;
}

const DEFS: PanelDef[] = [
  {
    id: "terminal",
    title: "Terminal",
    icon: TerminalIcon,
    edge: "right",
    windowTitle: "Loom — Terminal",
    render: TerminalPanel,
    tabStrip: TerminalTabs,
  },
  {
    id: "runs",
    title: "Runs",
    icon: RunsIcon,
    edge: "bottom",
    windowTitle: "Loom — Runs",
    render: RunsPanel,
  },
  {
    id: "sessions",
    title: "Chats",
    icon: PanelLeftIcon,
    // The left edge, and the only panel that lives there by default: the list is
    // the thing you scan while the chat is the thing you read, so they belong
    // side by side rather than stacked.
    edge: "left",
    windowTitle: "Loom — Chats",
    render: SessionsPanel,
  },
  {
    id: "files",
    title: "Files",
    icon: FilesIcon,
    edge: "right",
    windowTitle: "Loom — Files",
    render: FilesPanel,
  },
  {
    // Registered but in no default layout: the goal stays in the composer, and
    // this is the option for people who would rather have it beside the chat.
    id: "goal",
    title: "Goal",
    icon: TargetIcon,
    edge: "right",
    windowTitle: "Loom — Goal",
    render: GoalDockPanel,
  },
  {
    // The seam for the built-in browser. Present so the registry, the menu and
    // the tear-off path are already exercised by six panels rather than five,
    // which is what makes the browser a component and nothing else.
    id: "browser",
    title: "Browser",
    icon: GlobeIcon,
    edge: "right",
    windowTitle: "Loom — Browser",
    render: BrowserPanel,
  },
  {
    // Left by default, and first in the left zone's tab stack — see
    // `DockLayout::default()` in `dock.rs`. The left edge is where a project's
    // *state* belongs: what has changed, and what you are working on.
    id: "git",
    title: "Git",
    icon: GitBranchIcon,
    edge: "left",
    windowTitle: "Loom — Git",
    render: GitPanel,
  },
  {
    // Right by default, beside the terminal, because both are things you read
    // while the chat runs. **Not** added to the right zone's default tab stack:
    // registering the edge is enough for `openPanel` to land it there, and
    // putting it in the stack would cost the terminal its solo tab strip —
    // where its shell tabs *become* the zone's tabs and the `+` that opens a
    // shell sits where a `+` for a panel would.
    id: "editor",
    title: "Editor",
    icon: WrenchIcon,
    edge: "right",
    windowTitle: "Loom — Editor",
    render: EditorPanel,
    // The same seam the terminal uses: when the editor is alone in its zone its
    // open files *are* the zone's tabs, so there is one row instead of two
    // saying almost the same thing.
    tabStrip: EditorTabs,
  },
];

export const PANELS: Record<string, PanelDef> = Object.fromEntries(
  DEFS.map((def) => [def.id, def]),
);

/** Every panel, in the order the menus list them. */
export const PANEL_LIST = DEFS;

/**
 * The definition for an id, or null.
 *
 * Null rather than a fallback, because an id this build does not know is a panel
 * a newer build added. Rendering a placeholder is honest; rendering something
 * else would be a quiet lie, and `dock.rs` deliberately preserves the id so
 * nothing is lost.
 */
export function panelDef(id: string): PanelDef | null {
  return PANELS[id] ?? null;
}

/**
 * The icon for a panel id.
 *
 * Falls back to the generic panel glyph, so a tab for an unknown panel is still
 * a tab rather than a gap.
 */
export function iconFor(id: string): ComponentType<{ size?: number; className?: string }> {
  return PANELS[id]?.icon ?? PanelLeftIcon;
}

/** The display title for a panel id. */
export function titleFor(id: string): string {
  return PANELS[id]?.title ?? id;
}
