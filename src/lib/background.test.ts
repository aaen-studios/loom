// Deliberately the node environment, not jsdom: this file reads `index.html`
// from disk and tests pure functions. The jsdom pragma broke it, because
// `import.meta.url` becomes an `http://` URL there and `fileURLToPath` throws.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { BACKGROUND_PRESETS, THEME_BACKDROP, backdropFor, presetById } from "./background";

/**
 * The weave is the stitched loom pattern every preset is layered on top of, and
 * for a long time it was invisible while looking perfectly deliberate in the
 * source. Nothing in the suite said a word about it, because the tests only
 * asserted that the presets existed — not that the thread they paint could
 * actually be seen.
 *
 * These two tests pin the properties that were silently wrong.
 */

/**
 * The faintest thread allowed, as an authored alpha.
 *
 * Not a preference, and not a theoretical limit — it is set from what actually
 * failed. The light presets shipped a shadow thread at 0.078 (porcelain) and
 * 0.09 (linen), which are the only threads that can read at all on a near-white
 * base, and both were invisible in practice: the weave looked like a plain
 * gradient. The first version of this guard had a floor of 0.05, so both values
 * cleared it and the test stayed green while the feature was missing.
 *
 * The floor now sits above the known-bad values and below every value that has
 * been seen to read (0.16-0.17 light, 0.2 dark), so a preset that regresses to
 * the invisible range fails here instead of shipping.
 */
const MIN_THREAD_ALPHA = 0.12;

/** Authored alphas of the two thread colours, in paint order. */
function threadAlphas(layers: string): number[] {
  return [...layers.matchAll(/rgb\([^)]*\/\s*([\d.]+)\)/g)].map((m) =>
    Number(m[1]),
  );
}

/** Angles of the crossed thread runs, in paint order. */
function threadAngles(layers: string): number[] {
  return [...layers.matchAll(/repeating-linear-gradient\((\d+)deg/g)].map((m) =>
    Number(m[1]),
  );
}

describe("the window backdrop", () => {
  const ROOT = fileURLToPath(new URL("../../", import.meta.url));
  const HTML = readFileSync(`${ROOT}index.html`, "utf8");

  it("is the same colour in every place that paints before the app mounts", () => {
    // Three things paint this colour at different times: the `<html>` tag
    // before the bundle loads, `main.tsx` before React mounts, and the
    // Porcelain preset underneath everything else. They were four independent
    // literals; the light one is now referenced from one constant.
    expect(HTML).toContain(THEME_BACKDROP.light);
    expect(presetById("porcelain").base).toBe(THEME_BACKDROP.light);
  });

  it("picks by theme", () => {
    expect(backdropFor(false)).toBe(THEME_BACKDROP.light);
    expect(backdropFor(true)).toBe(THEME_BACKDROP.dark);
    expect(THEME_BACKDROP.light).not.toBe(THEME_BACKDROP.dark);
  });

  it("is a hex colour, because it is interpolated into CSS and HTML", () => {
    for (const value of Object.values(THEME_BACKDROP)) {
      expect(value).toMatch(/^#[0-9a-f]{6}$/i);
    }
  });
});

describe("weave", () => {
  it("paints every preset's two threads strongly enough to be seen", () => {
    for (const preset of BACKGROUND_PRESETS) {
      const alphas = threadAlphas(preset.layers);

      // One highlight, one shadow. A miss here means the weave was dropped or
      // rewritten into something this guard can no longer see.
      expect(alphas, `${preset.id} should paint two threads`).toHaveLength(2);

      for (const alpha of alphas) {
        expect(
          alpha,
          `${preset.id} thread alpha ${alpha} is too faint to survive compositing`,
        ).toBeGreaterThanOrEqual(MIN_THREAD_ALPHA);
      }
    }
  });

  it("crosses the warp and the weft at right angles", () => {
    for (const preset of BACKGROUND_PRESETS) {
      const angles = threadAngles(preset.layers);
      expect(angles, `${preset.id} should paint two threads`).toHaveLength(2);
      expect(
        angles[1]! - angles[0]!,
        `${preset.id} threads should be perpendicular`,
      ).toBe(90);
    }
  });
});
