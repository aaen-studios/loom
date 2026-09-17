import { describe, expect, it } from "vitest";
import type { Session, Workspace } from "../types";
import {
  arrangeGroups,
  GROUP_PAGE_SIZE,
  moveInList,
  orderChats,
  visibleRows,
} from "./sidebarOrder";

function session(
  id: string,
  workdir: string | null,
  updatedAt: number,
  position: number | null = null,
): Session {
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
    position,
    createdAt: 0,
    updatedAt,
  };
}

const WORKSPACES: Workspace[] = [
  { path: "C:/work/loom", name: "Loom", addedAt: 10 },
  { path: "C:/work/site", name: "Marketing site", addedAt: 20 },
];

describe("chat order inside a workspace", () => {
  it("keeps the newest-first order when nothing has been dragged", () => {
    const rows = [session("a", null, 30), session("b", null, 20), session("c", null, 10)];
    expect(orderChats(rows).map((row) => row.id)).toEqual(["a", "b", "c"]);
  });

  it("puts a chat that was never dragged above the hand-placed ones", () => {
    // The point of the rule: a chat you just started is the one you want, and
    // it must not be buried under an arrangement made last week.
    const rows = [
      session("placed-1", null, 1, 0),
      session("placed-2", null, 2, 1),
      session("fresh", null, 99, null),
    ];
    expect(orderChats(rows).map((row) => row.id)).toEqual([
      "fresh",
      "placed-1",
      "placed-2",
    ]);
  });

  it("orders placed chats by position, not by recency", () => {
    const rows = [
      session("old", null, 500, 1),
      session("recent", null, 900, 0),
    ];
    expect(orderChats(rows).map((row) => row.id)).toEqual(["recent", "old"]);
  });

  it("does not mutate the list it was given", () => {
    const rows = [session("a", null, 1, 1), session("b", null, 2, 0)];
    orderChats(rows);
    expect(rows.map((row) => row.id)).toEqual(["a", "b"]);
  });

  it("keeps the input order for chats with the same timestamp", () => {
    const rows = [session("first", null, 5), session("second", null, 5)];
    expect(orderChats(rows).map((row) => row.id)).toEqual(["first", "second"]);
  });
});

describe("workspace group order", () => {
  it("pins No workspace first and then goes by recency", () => {
    const groups = arrangeGroups(
      [
        session("a", "C:/work/site", 30),
        session("b", "C:/work/loom", 40),
        session("c", null, 10),
      ],
      WORKSPACES,
      [],
    );

    expect(groups.map((group) => group.name)).toEqual([
      "No workspace",
      "Loom",
      "Marketing site",
    ]);
  });

  it("lists a saved folder that has no chats, ranked by when it was added", () => {
    const groups = arrangeGroups([session("a", "C:/work/loom", 30)], WORKSPACES, []);
    expect(groups.map((group) => group.name)).toEqual(["Loom", "Marketing site"]);
    expect(groups[1].sessions).toEqual([]);
  });

  it("honours a hand-placed order over recency", () => {
    const groups = arrangeGroups(
      [session("a", "C:/work/loom", 40), session("b", "C:/work/site", 90)],
      WORKSPACES,
      ["C:/work/loom", "C:/work/site"],
    );
    expect(groups.map((group) => group.name)).toEqual(["Loom", "Marketing site"]);
  });

  it("floats a folder nobody has dragged above the placed ones", () => {
    // This is what makes a folder you just added appear at the top without
    // disturbing the arrangement underneath it.
    const fresh: Workspace[] = [
      ...WORKSPACES,
      { path: "C:/work/new", name: "Brand new", addedAt: 5_000 },
    ];
    const groups = arrangeGroups(
      [session("a", "C:/work/loom", 40), session("b", "C:/work/new", 60)],
      fresh,
      ["C:/work/site", "C:/work/loom"],
    );
    // "Brand new" is not in the saved order, so it floats; the two that are
    // keep the places they were given, in that order.
    expect(groups.map((group) => group.name)).toEqual([
      "Brand new",
      "Marketing site",
      "Loom",
    ]);
  });

  it("returns nothing when there is nothing to show", () => {
    expect(arrangeGroups([], [], [])).toEqual([]);
  });
});

describe("moveInList", () => {
  it("moves a row to the target's slot", () => {
    expect(moveInList(["a", "b", "c"], "c", "a")).toEqual(["c", "a", "b"]);
    expect(moveInList(["a", "b", "c"], "a", "c")).toEqual(["b", "c", "a"]);
  });

  it("swaps neighbours instead of doing nothing", () => {
    expect(moveInList(["a", "b"], "a", "b")).toEqual(["b", "a"]);
  });

  it("leaves the list alone for a no-op or an unknown id", () => {
    const ids = ["a", "b"];
    expect(moveInList(ids, "a", "a")).toBe(ids);
    expect(moveInList(ids, "a", "zz")).toBe(ids);
  });
});

describe("the five-row cap", () => {
  const rows = (count: number) =>
    Array.from({ length: count }, (_, index) => ({ id: `s${index}` }));

  it("shows five and counts the rest", () => {
    const { shown, hidden } = visibleRows(rows(9), null, false);
    expect(shown).toHaveLength(GROUP_PAGE_SIZE);
    expect(hidden).toBe(4);
  });

  it("shows everything once expanded", () => {
    const { shown, hidden } = visibleRows(rows(9), null, true);
    expect(shown).toHaveLength(9);
    expect(hidden).toBe(0);
  });

  it("keeps the open chat on screen when it sits past the cap", () => {
    const { shown } = visibleRows(rows(9), "s7", false);
    expect(shown.map((row) => row.id)).toContain("s7");
    expect(shown).toHaveLength(8);
  });

  it("does not pad a short group", () => {
    const { shown, hidden } = visibleRows(rows(2), "s0", false);
    expect(shown).toHaveLength(2);
    expect(hidden).toBe(0);
  });
});
