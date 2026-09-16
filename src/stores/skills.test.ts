import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../lib/ipc", () => ({
  ipc: {
    listSkills: vi.fn(async () => [
      {
        id: "review",
        name: "Review",
        description: "Check a diff",
        prompt: "Look for bugs.",
        path: "C:\\skills\\review.md",
      },
    ]),
  },
}));

import { useSkills } from "./skills";

describe("skills store", () => {
  beforeEach(() => {
    useSkills.setState({ skills: [], loaded: false });
  });

  it("loads the list and marks itself loaded", async () => {
    await useSkills.getState().load();
    const state = useSkills.getState();
    expect(state.loaded).toBe(true);
    expect(state.skills).toHaveLength(1);
    expect(state.skills[0].id).toBe("review");
  });
});
