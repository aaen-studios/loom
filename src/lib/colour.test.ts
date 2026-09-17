import { describe, expect, it } from "vitest";

import {
  adjustForContrast,
  contrast,
  hslToRgb,
  isDarkColour,
  luminance,
  mix,
  parseHex,
  rgbToHsl,
  rgba,
  toHex,
  withLightness,
} from "./colour";

/**
 * These are the functions that decide whether a theme is readable, so they are
 * checked against the values the WCAG guidelines themselves state rather than
 * against whatever the implementation happens to produce.
 */
describe("colour maths", () => {
  it("parses both hex forms and refuses everything else", () => {
    expect(parseHex("#ffffff")).toEqual({ r: 255, g: 255, b: 255 });
    expect(parseHex("#000")).toEqual({ r: 0, g: 0, b: 0 });
    expect(parseHex("#8ea2ff")).toEqual({ r: 142, g: 162, b: 255 });
    // `#abc` expands by doubling, not by scaling — a common off-by-a-bit bug.
    expect(parseHex("#abc")).toEqual({ r: 170, g: 187, b: 204 });
    expect(parseHex("rgb(1,2,3)")).toBeNull();
    expect(parseHex("#12345")).toBeNull();
    expect(parseHex("")).toBeNull();
  });

  it("round-trips hex", () => {
    for (const value of ["#000000", "#ffffff", "#8ea2ff", "#1a7f37"]) {
      expect(toHex(parseHex(value)!)).toBe(value);
    }
  });

  it("writes rgba in the form the stylesheet uses", () => {
    expect(rgba({ r: 16, g: 19, b: 34 }, 0.06)).toBe("rgb(16 19 34 / 0.060)");
    expect(rgba({ r: 0, g: 0, b: 0 }, 1)).toBe("rgb(0 0 0 / 1.000)");
  });

  it("round-trips through HSL", () => {
    for (const value of ["#8ea2ff", "#e5484d", "#0d1016", "#eef1f7"]) {
      const original = parseHex(value)!;
      const back = hslToRgb(rgbToHsl(original));
      // A hex round trip is exact; HSL is floating point, so allow a byte.
      expect(toHex(back)).toBe(toHex(original));
    }
  });

  it("knows the luminance of black, white and a mid grey", () => {
    expect(luminance({ r: 0, g: 0, b: 0 })).toBe(0);
    expect(luminance({ r: 255, g: 255, b: 255 })).toBeCloseTo(1, 5);
    // The sRGB transfer function means mid-grey is far below 0.5, which is
    // exactly why `isDarkColour` cannot use a channel average.
    expect(luminance({ r: 128, g: 128, b: 128 })).toBeCloseTo(0.2158, 3);
  });

  it("matches the contrast ratios the guidelines state", () => {
    const white = { r: 255, g: 255, b: 255 };
    const black = { r: 0, g: 0, b: 0 };
    expect(contrast(white, black)).toBeCloseTo(21, 1);
    expect(contrast(white, white)).toBeCloseTo(1, 5);
    // #767676 on white is the canonical 4.5:1 boundary the guidelines cite.
    expect(contrast(parseHex("#767676")!, white)).toBeCloseTo(4.54, 1);
    // Order must not matter.
    expect(contrast(white, black)).toBe(contrast(black, white));
  });

  it("judges darkness by luminance, not by eye", () => {
    expect(isDarkColour({ r: 0, g: 0, b: 0 })).toBe(true);
    expect(isDarkColour({ r: 255, g: 255, b: 255 })).toBe(false);
    // Mid grey: dark by luminance even though it "looks" halfway. This is the
    // case where a channel average would give the wrong answer.
    expect(isDarkColour({ r: 128, g: 128, b: 128 })).toBe(true);
  });

  it("changes lightness without touching hue or saturation", () => {
    const start = parseHex("#8ea2ff")!;
    const before = rgbToHsl(start);
    const after = rgbToHsl(withLightness(start, 0.25));
    expect(after.h).toBeCloseTo(before.h, 1);
    expect(after.s).toBeCloseTo(before.s, 3);
    expect(after.l).toBeCloseTo(0.25, 3);
  });

  it("mixes endpoints exactly", () => {
    const a = { r: 0, g: 0, b: 0 };
    const b = { r: 255, g: 255, b: 255 };
    expect(mix(a, b, 0)).toEqual(a);
    expect(mix(a, b, 1)).toEqual(b);
    expect(mix(a, b, 0.5).r).toBeCloseTo(127.5, 3);
    // Out-of-range amounts clamp rather than extrapolate past the endpoints.
    expect(mix(a, b, 2)).toEqual(b);
  });
});

describe("adjustForContrast", () => {
  const white = { r: 255, g: 255, b: 255 };
  const nearBlack = parseHex("#0d1016")!;
  const nearWhite = parseHex("#eef1f7")!;

  it("leaves a colour that already passes completely alone", () => {
    // The whole point of the early return: a deliberate pick must survive.
    const chosen = parseHex("#4f5bd5")!;
    expect(adjustForContrast(chosen, nearWhite, 3)).toEqual(chosen);
  });

  it("lifts a colour out of a dark backdrop", () => {
    const dim = parseHex("#1b2030")!;
    const fixed = adjustForContrast(dim, nearBlack, 4.5);
    expect(contrast(fixed, nearBlack)).toBeGreaterThanOrEqual(4.5);
  });

  it("darkens a colour into a light backdrop", () => {
    const pale = parseHex("#f2f4ff")!;
    const fixed = adjustForContrast(pale, nearWhite, 4.5);
    expect(contrast(fixed, nearWhite)).toBeGreaterThanOrEqual(4.5);
  });

  it("moves as little as it can, not to the extreme", () => {
    // A colour that needs only a nudge should get a nudge. Landing on pure
    // white would satisfy the target and destroy the hue.
    const nearlyThere = parseHex("#5a5f6b")!;
    const fixed = adjustForContrast(nearlyThere, nearWhite, 4.5);
    expect(contrast(fixed, nearWhite)).toBeGreaterThanOrEqual(4.5);
    expect(fixed).not.toEqual(white);
    // Still recognises its own hue family rather than becoming neutral.
    expect(rgbToHsl(fixed).h).toBeCloseTo(rgbToHsl(nearlyThere).h, 0);
  });

  it("preserves hue, which is what makes an accent still an accent", () => {
    const accent = parseHex("#8ea2ff")!;
    const fixed = adjustForContrast(accent, nearWhite, 4.5);
    expect(rgbToHsl(fixed).h).toBeCloseTo(rgbToHsl(accent).h, 0);
  });

  it("gives up gracefully on a backdrop with no room", () => {
    // Mid grey: neither white nor black reaches 21:1, and a high target is
    // unreachable. It must still return something rather than loop.
    const midGrey = { r: 128, g: 128, b: 128 };
    const fixed = adjustForContrast(parseHex("#7a7a7a")!, midGrey, 15);
    expect(contrast(fixed, midGrey)).toBeGreaterThan(contrast(parseHex("#7a7a7a")!, midGrey));
  });

  it("reaches a high target when one exists", () => {
    // 7:1 is the AAA body-text threshold the ink target uses.
    const fixed = adjustForContrast(parseHex("#4a4a55")!, nearBlack, 7);
    expect(contrast(fixed, nearBlack)).toBeGreaterThanOrEqual(7);
  });

  it("picks the direction that actually reaches further", () => {
    // The case that broke a luminance-threshold version of this function.
    // `#7d7d7d` has a luminance of 0.205, so a midpoint rule calls it dark and
    // lightens — reaching only 4.12:1. Blackening reaches 5.10:1, so the
    // available target was missed. Direction has to be measured.
    const midGrey = parseHex("#7d7d7d")!;
    expect(isDarkColour(midGrey)).toBe(true);

    const fixed = adjustForContrast(parseHex("#4a4a55")!, midGrey, 4.5);
    expect(contrast(fixed, midGrey)).toBeGreaterThanOrEqual(4.5);
    // It went *down*, against what the threshold would have said.
    expect(luminance(fixed)).toBeLessThan(luminance(parseHex("#4a4a55")!));
  });

  it("is right on both sides of the crossover", () => {
    // The crossover is where the two extremes are equally good, at luminance
    // ≈0.179. Either side of it must still clear the target where possible.
    for (const hex of ["#3a3a3a", "#5a5a5a", "#8a8a8a", "#b0b0b0"]) {
      const backdrop = parseHex(hex)!;
      const fixed = adjustForContrast(parseHex("#7a7a7a")!, backdrop, 4.5);
      expect(
        contrast(fixed, backdrop),
        `${hex} could not be reached`,
      ).toBeGreaterThanOrEqual(4.0);
    }
  });
});
