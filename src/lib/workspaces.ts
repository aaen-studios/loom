import type { Session, SidebarSort, Workspace } from "../types";

export interface WorkspaceGroup {
  /** Session workdir as the group key; `""` for chats with no workspace. */
  key: string;
  name: string;
  workdir: string | null;
  sessions: Session[];
  /** Newest `updatedAt` in the group; 0 when it has no chats yet. */
  newestAt: number;
  /** When the folder was added to the saved list; 0 for an unsaved folder. */
  addedAt: number;
}

/** Last path segment, used when a workspace has no saved name. */
export function folderName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

/** Display name for a chat's folder: the saved name wins, then the folder name. */
export function workspaceLabel(
  workdir: string | null,
  workspaces: Workspace[],
): string | null {
  if (!workdir) return null;
  return (
    workspaces.find((workspace) => workspace.path === workdir)?.name ??
    folderName(workdir)
  );
}

/** Reorders the (newest-first) session list per the sidebar preference. */
export function sortSessions(sessions: Session[], sort: SidebarSort): Session[] {
  if (sort === "recent") return sessions;
  const sorted = [...sessions];
  if (sort === "oldest") {
    sorted.sort((a, b) => a.updatedAt - b.updatedAt);
  } else {
    sorted.sort((a, b) =>
      (a.title || "New chat").localeCompare(b.title || "New chat", undefined, {
        sensitivity: "base",
      }),
    );
  }
  return sorted;
}

/**
 * Splits the (already sorted) session list into workspace groups. A saved
 * workspace name wins over the folder name; chats without a folder share one
 * "No workspace" group. Groups keep the order of their first session, so the
 * list's sort decides which workspace comes first.
 */
export function groupSessions(
  sessions: Session[],
  workspaces: Workspace[],
): WorkspaceGroup[] {
  const groups = new Map<string, WorkspaceGroup>();

  const groupFor = (workdir: string | null): WorkspaceGroup => {
    const key = workdir ?? "";
    let group = groups.get(key);
    if (!group) {
      group = {
        key,
        name: workspaceLabel(workdir, workspaces) ?? "No workspace",
        workdir,
        sessions: [],
        newestAt: 0,
        addedAt: addedAt(workdir, workspaces),
      };
      groups.set(key, group);
    }
    return group;
  };

  for (const session of sessions) {
    const group = groupFor(session.workdir);
    group.sessions.push(session);
    group.newestAt = Math.max(group.newestAt, session.updatedAt);
  }

  return [...groups.values()];
}

/** When the folder was saved, for the group ordering rule. */
function addedAt(workdir: string | null, workspaces: Workspace[]): number {
  if (!workdir) return 0;
  return workspaces.find((workspace) => workspace.path === workdir)?.addedAt ?? 0;
}
