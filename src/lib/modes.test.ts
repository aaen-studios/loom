import { describe, expect, it } from "vitest";
import type { PermissionMode } from "../types";
import {
  AGENT_MODES,
  GLOBAL_PERMISSION_MODES,
  PERMISSION_MODES,
  canAlwaysAllow,
} from "./modes";

describe("mode lists", () => {
  it("has four permission modes, Atelier last and accent-toned", () => {
    expect(PERMISSION_MODES).toHaveLength(4);
    for (const mode of PERMISSION_MODES) {
      expect(mode.label.length).toBeGreaterThan(0);
      expect(mode.help.length).toBeGreaterThan(0);
      expect(["neutral", "accent", "danger"]).toContain(mode.tone);
    }
    expect(PERMISSION_MODES.map((mode) => mode.id)).toEqual([
      "ask",
      "auto-read-only",
      "auto-all",
      "atelier",
    ]);
    expect(PERMISSION_MODES[3].tone).toBe("accent");
    expect(PERMISSION_MODES[3].help).toContain("harness");
    expect(PERMISSION_MODES[2].tone).toBe("danger");
  });

  it("never offers Atelier as a global default", () => {
    expect(GLOBAL_PERMISSION_MODES.map((mode) => mode.id)).toEqual([
      "ask",
      "auto-read-only",
      "auto-all",
    ]);
  });

  it("lists the agent modes, read-only ones after the defaults", () => {
    expect(AGENT_MODES.map((mode) => mode.id)).toEqual([
      "plan",
      "build",
      "review",
      "chat",
    ]);
  });

  it("hides Always allow in Atelier", () => {
    // The card would persist Auto all globally, dropping the harness tools and
    // the mode itself; every other mode may still remember its answer.
    expect(canAlwaysAllow("atelier")).toBe(false);
    const allowed: (PermissionMode | null | undefined)[] = [
      "ask",
      "auto-read-only",
      "auto-all",
      null,
      undefined,
    ];
    for (const mode of allowed) {
      expect(canAlwaysAllow(mode)).toBe(true);
    }
  });

  it("hides Always allow on a delete, whatever the mode", () => {
    // A delete card under Auto all means the loss would be real, so the card
    // must not offer to stop asking: that is the one question worth keeping.
    for (const mode of ["ask", "auto-read-only", "auto-all", null, undefined] as const) {
      expect(canAlwaysAllow(mode, "delete_path")).toBe(false);
      expect(canAlwaysAllow(mode, "write_file")).toBe(true);
      expect(canAlwaysAllow(mode)).toBe(true);
    }
    expect(canAlwaysAllow("atelier", "write_file")).toBe(false);
  });
});
