// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";

import { backdropFor } from "../lib/background";
import { applyCachedAppearance } from "./settings";

/**
 * The appearance cache exists because the config arrives over async IPC, so the
 * first paint used to use the defaults and then flip: light→dark, sharp→blurred.
 * These tests cover the two things that can go wrong with a synchronous cache —
 * reading it wrong, and a bad entry stopping the boot.
 */

const KEY = "loomAppearance";

function cache(value: unknown): void {
  localStorage.setItem(KEY, typeof value === "string" ? value : JSON.stringify(value));
}

/**
 * What jsdom will report for a hex colour.
 *
 * Assigning `style.background = "#eef1f7"` and reading it back gives
 * `rgb(238, 241, 247)`: the CSSOM normalises to the computed form. Comparing
 * against the hex fails on the serialisation, not on the value, so the
 * expectation is converted rather than the assertion weakened to a substring
 * match that would pass for the wrong colour.
 */
function asCssRgb(hex: string): string {
  const value = hex.replace("#", "");
  const parts = [0, 2, 4].map((offset) =>
    parseInt(value.slice(offset, offset + 2), 16),
  );
  return `rgb(${parts.join(", ")})`;
}

describe("applyCachedAppearance", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.classList.remove("dark");
    document.documentElement.style.background = "";
  });

  it("applies a dark theme before anything renders", () => {
    cache({ theme: "dark" });

    applyCachedAppearance();

    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(document.documentElement.style.background).toBe(
      asCssRgb(backdropFor(true)),
    );
  });

  it("applies a light theme, and removes a stale dark class", () => {
    // The class can be left over from a previous theme, so this asserts the
    // removal as well as the absence — a `toggle` that only ever adds would
    // pass a weaker check and show the wrong theme.
    document.documentElement.classList.add("dark");
    cache({ theme: "light" });

    applyCachedAppearance();

    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(document.documentElement.style.background).toBe(
      asCssRgb(backdropFor(false)),
    );
  });

  it("falls back to the default rather than throwing on a bad entry", () => {
    // Each of these is a way a cache can be wrong on a real machine: truncated
    // by a crash, written by an older build, or hand-edited. None may stop the
    // boot, because the config arrives a moment later and corrects everything.
    for (const bad of [
      "not json at all",
      "null",
      "[]",
      '{"theme":"purple"}',
      '{"theme":42}',
      "42",
    ]) {
      localStorage.clear();
      cache(bad);
      expect(() => applyCachedAppearance(), bad).not.toThrow();
      expect(document.documentElement.classList.contains("dark"), bad).toBe(false);
      expect(
        document.documentElement.style.background,
        bad,
      ).toBe(asCssRgb(backdropFor(false)));
    }
  });

  it("does nothing when there is no cache at all", () => {
    // A first run. The defaults must still leave a usable window.
    expect(() => applyCachedAppearance()).not.toThrow();
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(document.documentElement.style.background).toBe(
      asCssRgb(backdropFor(false)),
    );
  });
});
