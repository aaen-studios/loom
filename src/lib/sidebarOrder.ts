import type { Session, Workspace } from "../types";
import { groupSessions, workspaceLabel, type WorkspaceGroup } from "./workspaces";

/**
 * The order of the chats popup, once it is the user's rather than a rule's.
 *
 * Grouped mode is purely manual. A workspace you have dragged keeps its place;
 * a chat you have dragged keeps its; and anything you have never dragged still
 * sorts newest-first — which is what makes a chat started a moment ago land on
 * top of a group you arranged last week without disturbing the arrangement
 * below it.
 *
 * The rules live here, as pure functions, because they are the part of the
 * sidebar that is hard to see and easy to get subtly wrong.
 */

/** Rows a workspace shows before "Show more". */
export const GROUP_PAGE_SIZE = 5;

/**
 * The collapsed set with one group toggled, as a new array.
 *
 * The sidebar keeps this as a list in `config.json` rather than as a map, so it
 * survives a restart and reads sensibly in the file. Order is preserved so the
 * saved list stays stable instead of churning on every click.
 */
export function toggleCollapsedGroup(keys: string[], key: string): string[] {
  return keys.includes(key) ? keys.filter((entry) => entry !== key) : [...keys, key];
}

/**
 * Whether a group renders collapsed.
 *
 * `revealed` is the ephemeral override for the group holding the chat you are
 * in: opening the list has to show you where you are, but that is a courtesy
 * for this visit, not a change to what you chose. So it is kept out of the
 * persisted list entirely — otherwise opening the sidebar would quietly undo a
 * collapse you meant, and the preference would never survive contact with the
 * thing it describes.
 */
export function isGroupCollapsed(
  collapsed: string[],
  revealed: string[],
  key: string,
): boolean {
  if (revealed.includes(key)) return false;
  return collapsed.includes(key);
}

/** The key of the pinned group: chats with no workspace. */
export const NO_WORKSPACE_KEY = "";

/** A chat's placement, which is all this module needs to know about one. */
interface Placeable {
  position: number | null;
  updatedAt: number;
}

/**
 * Orders the chats inside one workspace.
 *
 * Two buckets, in this order:
 *
 * 1. Chats with no `position` — never dragged — newest first. A chat you just
 *    sent a message in is the one you are looking for, so it goes on top, above
 *    whatever you arranged by hand.
 * 2. Chats with a `position`, ascending. Every chat in a group gets one the
 *    first time you drag anything in it, so from then on this is the order.
 *
 * `sort` is stable, so two chats with the same `updatedAt` keep the order the
 * database returned them in.
 */
export function orderChats<T extends Placeable>(chats: T[]): T[] {
  return [...chats].sort((left, right) => {
    const a = left.position;
    const b = right.position;
    if (a === null && b === null) return right.updatedAt - left.updatedAt;
    if (a === null) return -1;
    if (b === null) return 1;
    return a - b;
  });
}

/**
 * Where a group sits when nobody has dragged anything: as high as its newest
 * chat, or — for a folder with no chats in it yet — as high as the moment you
 * added it, so a folder you just picked appears straight away.
 */
function groupRank(group: WorkspaceGroup): number {
  return Math.max(group.newestAt, group.addedAt);
}

/**
 * Builds the popup's groups: every workspace with a chat in it, plus every
 * saved folder that is currently empty, in display order.
 *
 * The order is:
 *
 * 1. `No workspace`, always, and it is not draggable.
 * 2. Folders nobody has dragged, by recency — so a folder you just added lands
 *    at the top.
 * 3. Folders in `workspaceOrder`, in that order.
 *
 * Rule 2 before rule 3 is the whole trick. The first drag writes the complete
 * list, so every folder that exists at that moment is placed; only a folder
 * added afterwards is unlisted, and it is exactly that one that should appear
 * at the top.
 */
export function arrangeGroups(
  sessions: Session[],
  workspaces: Workspace[],
  workspaceOrder: string[],
): WorkspaceGroup[] {
  const groups = groupSessions(sessions, workspaces);

  // A saved folder with no chats in it is still a place you can start one, and
  // it is the only way "the folder you just added is at the top" is visible.
  const seen = new Set(groups.map((group) => group.key));
  for (const workspace of workspaces) {
    if (seen.has(workspace.path)) continue;
    groups.push({
      key: workspace.path,
      name: workspaceLabel(workspace.path, workspaces) ?? workspace.path,
      workdir: workspace.path,
      sessions: [],
      newestAt: 0,
      addedAt: workspace.addedAt,
    });
  }

  const rank = new Map(workspaceOrder.map((path, index) => [path, index]));
  const pinned = groups.filter((group) => group.key === NO_WORKSPACE_KEY);
  const rest = groups
    .filter((group) => group.key !== NO_WORKSPACE_KEY)
    .sort((left, right) => {
      const a = rank.get(left.key);
      const b = rank.get(right.key);
      if (a === undefined && b === undefined) return groupRank(right) - groupRank(left);
      if (a === undefined) return -1;
      if (b === undefined) return 1;
      return a - b;
    });

  return [...pinned, ...rest];
}

/**
 * Moves `from` into `to`'s slot, keeping everything else in order.
 *
 * `to`'s index is read from the list as it was, and the item is inserted there
 * after removal — so dragging a row down onto the row below it swaps the two
 * rather than doing nothing. Used for both chats and workspace groups.
 */
export function moveInList(ids: string[], from: string, to: string): string[] {
  const fromIndex = ids.indexOf(from);
  const toIndex = ids.indexOf(to);
  if (fromIndex < 0 || toIndex < 0 || fromIndex === toIndex) return ids;
  const next = [...ids];
  const [moved] = next.splice(fromIndex, 1);
  next.splice(toIndex, 0, moved);
  return next;
}

/**
 * The rows of one group to render, and how many are left over.
 *
 * Every group shows [`GROUP_PAGE_SIZE`] rows, except that the chat you are
 * looking at is always among them even when it sits deeper than that: the popup
 * has to show where you are.
 */
export function visibleRows<T extends { id: string }>(
  rows: T[],
  activeId: string | null,
  expanded: boolean,
): { shown: T[]; hidden: number } {
  if (expanded) return { shown: rows, hidden: 0 };
  const activeIndex = rows.findIndex((row) => row.id === activeId);
  const cutoff = Math.max(GROUP_PAGE_SIZE, activeIndex + 1);
  const shown = rows.slice(0, cutoff);
  return { shown, hidden: rows.length - shown.length };
}
