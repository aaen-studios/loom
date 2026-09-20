import { describe, expect, it } from "vitest";
import type { Session, Workspace } from "../types";
import { folderName, groupSessions, sortSessions, workspaceLabel } from "./workspaces";

function session(id: string, workdir: string | null, updatedAt: number): Session {
  return {
    id,
    title: id,
    providerId: null,
    modelId: null,
    variant: null,
    personaId: null,
    systemPrompt: null,
    workdir,
    permissionMode: null,
    agentMode: null,
    computerAccess: false,
    browserAccess: false,
    position: null,
    createdAt: 0,
    updatedAt,
  };
}

const WORKSPACES: Workspace[] = [
  { path: "C:/work/loom", name: "Loom", addedAt: 1 },
  { path: "C:/work/site", name: "Marketing site", addedAt: 2 },
];

describe("workspace history grouping", () => {
  it("names groups from the saved list, newest activity first", () => {
    const groups = groupSessions(
      [
        session("a", "C:/work/site", 30),
        session("b", "C:/work/loom", 20),
        session("c", "C:/work/site", 10),
      ],
      WORKSPACES,
    );

    expect(groups.map((group) => group.name)).toEqual(["Marketing site", "Loom"]);
    expect(groups[0].sessions.map((entry) => entry.id)).toEqual(["a", "c"]);
    expect(groups[0].workdir).toBe("C:/work/site");
  });

  it("falls back to the folder name for unsaved workspaces", () => {
    const groups = groupSessions([session("a", "D:/scratch/notes", 5)], []);
    expect(groups).toHaveLength(1);
    expect(groups[0].name).toBe("notes");
  });

  it("collects chats without a folder into one trailing group", () => {
    const groups = groupSessions(
      [session("a", "C:/work/loom", 20), session("b", null, 10), session("c", null, 9)],
      WORKSPACES,
    );

    expect(groups.map((group) => group.name)).toEqual(["Loom", "No workspace"]);
    expect(groups[1].workdir).toBeNull();
    expect(groups[1].sessions).toHaveLength(2);
  });

  it("keeps a removed workspace's chats together under the folder name", () => {
    const groups = groupSessions([session("a", "C:/work/loom", 20)], []);
    expect(groups[0].name).toBe("loom");
  });

  it("returns nothing for no sessions", () => {
    expect(groupSessions([], WORKSPACES)).toEqual([]);
  });

  it("reads the last segment of a path in either slash style", () => {
    expect(folderName("C:/work/loom")).toBe("loom");
    expect(folderName("C:\\work\\loom\\")).toBe("loom");
  });
});

describe("workspace labels", () => {
  it("prefers the saved name and falls back to the folder name", () => {
    expect(workspaceLabel("C:/work/loom", WORKSPACES)).toBe("Loom");
    expect(workspaceLabel("D:/scratch/notes", WORKSPACES)).toBe("notes");
    expect(workspaceLabel(null, WORKSPACES)).toBeNull();
  });
});

describe("sidebar sorting", () => {
  it("keeps the newest-first order for recent", () => {
    const sessions = [session("a", null, 3), session("b", null, 2)];
    expect(sortSessions(sessions, "recent").map((entry) => entry.id)).toEqual([
      "a",
      "b",
    ]);
  });

  it("orders oldest first without touching the input", () => {
    const sessions = [session("a", null, 3), session("b", null, 2)];
    expect(sortSessions(sessions, "oldest").map((entry) => entry.id)).toEqual([
      "b",
      "a",
    ]);
    expect(sessions.map((entry) => entry.id)).toEqual(["a", "b"]);
  });

  it("orders by title, ignoring case, treating untitled chats as New chat", () => {
    const sessions = [
      { ...session("z", null, 3), title: "Zebra" },
      { ...session("a", null, 2), title: "apple" },
      { ...session("n", null, 1), title: "" },
    ];
    expect(sortSessions(sessions, "title").map((entry) => entry.id)).toEqual([
      "a",
      "n",
      "z",
    ]);
  });
});
