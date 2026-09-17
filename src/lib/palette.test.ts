import { describe, expect, it } from "vitest";

import { contrast, isDarkColour, parseHex, rgbToHsl, toHex } from "./colour";
import { AUTO_PRESET, BACKGROUND_PRESETS, resolvePreset } from "./background";
import {
  MIN_ACCENT_CONTRAST,
  MIN_INK_CONTRAST,
  PALETTE_TOKENS,
  deriveAccent,
  deriveAdaptive,
  deriveInk,
  deriveSurface,
  enforceContrast,
  fromPaletteHex,
  isReadable,
  quantise,
  swatchSignature,
  swatchesFromGradient,
  toPaletteHex,
  tokensFor,
  wasAdjusted,
  type Palette,
  type Swatch,
} from "./palette";

/**
 * The palette layer's whole job is a guarantee: whatever colours come in —
 * sampled from a photograph, or typed by hand — the text is readable on the
 * surface. So these tests assert the *ratios*, not the hex values. A test that
 * pinned the output of a derivation would fail on every improvement and pass
 * on every regression that kept the same numbers.
 */

const swatch = (hex: string, weight: number): Swatch => ({
  colour: parseHex(hex)!,
  weight,
});

/** A dark landscape: lots of deep blue, a bright sky, a warm highlight. */
const DARK_SCENE: Swatch[] = [
  swatch("#151d2e", 0.55),
  swatch("#2c4a6e", 0.2),
  swatch("#8fb3d9", 0.15),
  swatch("#e8b06a", 0.1),
];

/** A bright, pale scene: mostly near-white with one saturated detail. */
const LIGHT_SCENE: Swatch[] = [
  swatch("#f4f1ea", 0.6),
  swatch("#dcd3c4", 0.25),
  swatch("#c2461f", 0.15),
];

/** A black-and-white photograph. No usable hue anywhere. */
const MONO_SCENE: Swatch[] = [
  swatch("#101010", 0.5),
  swatch("#8a8a8a", 0.3),
  swatch("#f0f0f0", 0.2),
];

const FALLBACK_ACCENT = parseHex("#8ea2ff")!;

/** Runs the sampler's real work over a flat set of pixels. */
function pixelsOf(swatches: Swatch[], size = 40): Uint8ClampedArray {
  const data = new Uint8ClampedArray(size * size * 4);
  let at = 0;
  for (const entry of swatches) {
    const count = Math.round(entry.weight * size * size);
    for (let i = 0; i < count && at < size * size; i += 1, at += 1) {
      data[at * 4] = entry.colour.r;
      data[at * 4 + 1] = entry.colour.g;
      data[at * 4 + 2] = entry.colour.b;
      data[at * 4 + 3] = 255;
    }
  }
  // Any pixels left over repeat the first swatch, so the buffer is full.
  for (; at < size * size; at += 1) {
    data[at * 4] = swatches[0].colour.r;
    data[at * 4 + 1] = swatches[0].colour.g;
    data[at * 4 + 2] = swatches[0].colour.b;
    data[at * 4 + 3] = 255;
  }
  return data;
}

describe("quantise", () => {
  it("recovers the colours a picture is made of", () => {
    const found = quantise(pixelsOf(DARK_SCENE));

    expect(found.length).toBeGreaterThan(1);
    // The dominant swatch is the colour the picture is mostly made of, and it
    // is a mean rather than a histogram centre, so it lands close to the truth.
    const heaviest = found[0].colour;
    expect(contrast(heaviest, parseHex("#151d2e")!)).toBeLessThan(1.6);
    // Weights are shares of the sampled pixels.
    const total = found.reduce((sum, entry) => sum + entry.weight, 0);
    expect(total).toBeCloseTo(1, 5);
  });

  it("finds a small vivid colour, not only the large flat one", () => {
    // The whole point of scoring by population *and* range: a 10% saturated
    // detail must survive next to a 55% field of dark blue.
    const found = quantise(pixelsOf(DARK_SCENE, 60));
    const hasWarm = found.some((entry) => {
      const { h, s } = rgbToHsl(entry.colour);
      return h > 20 && h < 60 && s > 0.3;
    });
    expect(hasWarm).toBe(true);
  });

  it("ignores fully transparent pixels", () => {
    const data = new Uint8ClampedArray(4 * 4);
    // Four pixels, all transparent, all pure red: none of it counts.
    for (let i = 0; i < 4; i += 1) {
      data[i * 4] = 255;
      data[i * 4 + 3] = 0;
    }
    expect(quantise(data)).toEqual([]);
  });

  it("returns nothing for an empty buffer rather than throwing", () => {
    expect(quantise(new Uint8ClampedArray(0))).toEqual([]);
  });

  it("is deterministic, so a theme does not change between reloads", () => {
    const first = quantise(pixelsOf(LIGHT_SCENE));
    const second = quantise(pixelsOf(LIGHT_SCENE));
    expect(first.map((e) => toHex(e.colour))).toEqual(second.map((e) => toHex(e.colour)));
  });
});

describe("deriving a theme from an image", () => {
  it("holds the text legible on a dark scene, in both bases", () => {
    for (const isDark of [true, false]) {
      const palette = deriveAdaptive(DARK_SCENE, isDark, FALLBACK_ACCENT);
      expect(
        contrast(palette.ink, palette.surface),
        `ink on surface, dark=${isDark}`,
      ).toBeGreaterThanOrEqual(MIN_INK_CONTRAST);
      expect(
        contrast(palette.accent, palette.surface),
        `accent on surface, dark=${isDark}`,
      ).toBeGreaterThanOrEqual(MIN_ACCENT_CONTRAST);
      expect(isReadable(palette)).toBe(true);
    }
  });

  it("holds the text legible on a pale scene, in both bases", () => {
    for (const isDark of [true, false]) {
      const palette = deriveAdaptive(LIGHT_SCENE, isDark, FALLBACK_ACCENT);
      expect(contrast(palette.ink, palette.surface)).toBeGreaterThanOrEqual(
        MIN_INK_CONTRAST,
      );
      expect(contrast(palette.accent, palette.surface)).toBeGreaterThanOrEqual(
        MIN_ACCENT_CONTRAST,
      );
    }
  });

  it("survives a black-and-white photograph", () => {
    // A monochrome picture has no hue to sample. It must still produce a usable
    // theme rather than a washed-out grey pretending to be a colour choice.
    const palette = deriveAdaptive(MONO_SCENE, true, FALLBACK_ACCENT);
    expect(isReadable(palette)).toBe(true);
    // The accent falls back to the base theme's, which is a real colour rather
    // than a grey standing in for one.
    expect(rgbToHsl(palette.accent).s).toBeGreaterThan(0.3);
  });

  it("survives an image with no pixels at all", () => {
    const palette = deriveAdaptive([], true, FALLBACK_ACCENT);
    expect(isReadable(palette)).toBe(true);
    expect(toHex(palette.accent)).toBe(toHex(FALLBACK_ACCENT));
  });

  it("keeps the surface quiet, so glass over a photo still reads", () => {
    // A saturated panel would fight the artwork it sits on. This is the same
    // choice the built-in presets make, and worth pinning.
    const vivid = [swatch("#ff0000", 1)];
    const surface = deriveSurface(vivid, true);
    expect(rgbToHsl(surface).s).toBeLessThanOrEqual(0.16);
    // Its hue, though, is the picture's — which is what makes it adaptive.
    expect(rgbToHsl(surface).h).toBeCloseTo(0, 0);
  });

  it("takes the surface's lightness from the base, not the picture", () => {
    // The same picture under light and dark must give recognisably different
    // surfaces: the base still decides how bright the app is.
    const dark = deriveSurface(DARK_SCENE, true);
    const light = deriveSurface(DARK_SCENE, false);
    expect(isDarkColour(dark)).toBe(true);
    expect(isDarkColour(light)).toBe(false);
  });

  it("lifts a muted accent into a usable band", () => {
    // A photograph's colours are mostly muted; a muted accent is
    // indistinguishable from a grey one, which is the failure this prevents.
    const muted = [swatch("#6b6f5e", 1)];
    const surface = deriveSurface(muted, true);
    const accent = deriveAccent(muted, surface, FALLBACK_ACCENT);
    expect(rgbToHsl(accent).s).toBeGreaterThanOrEqual(0.4);
    expect(contrast(accent, surface)).toBeGreaterThanOrEqual(MIN_ACCENT_CONTRAST);
  });

  it("keeps a hint of hue in the ink without making text coloured", () => {
    // Beyond a small tint the eye reads body text as coloured rather than as
    // text, so the saturation is capped deliberately low.
    const surface = parseHex("#0d1016")!;
    const ink = deriveInk(surface, 140, true);
    expect(rgbToHsl(ink).s).toBeLessThanOrEqual(0.16);
    expect(contrast(ink, surface)).toBeGreaterThan(4.5);
  });
});

describe("enforcing contrast on a hand-picked palette", () => {
  it("leaves a readable palette exactly as it is", () => {
    // A deliberate choice must not be second-guessed.
    const chosen: Palette = {
      surface: parseHex("#101418")!,
      ink: parseHex("#f2f5fa")!,
      accent: parseHex("#8ea2ff")!,
    };
    const after = enforceContrast(chosen);
    expect(after).toEqual(chosen);
    expect(wasAdjusted(chosen, after)).toBe(false);
  });

  it("rescues unreadable text, and says that it did", () => {
    // The case a user actually hits: dark text on a dark surface.
    const broken: Palette = {
      surface: parseHex("#12151f")!,
      ink: parseHex("#22252f")!,
      accent: parseHex("#1a1d28")!,
    };
    expect(isReadable(broken)).toBe(false);

    const fixed = enforceContrast(broken);

    expect(isReadable(fixed)).toBe(true);
    expect(wasAdjusted(broken, fixed)).toBe(true);
    expect(contrast(fixed.ink, fixed.surface)).toBeGreaterThanOrEqual(MIN_INK_CONTRAST);
  });

  it("never moves the surface, because everything is measured against it", () => {
    const broken: Palette = {
      surface: parseHex("#12151f")!,
      ink: parseHex("#20232c")!,
      accent: parseHex("#1a1d28")!,
    };
    expect(toHex(enforceContrast(broken).surface)).toBe("#12151f");
  });

  it("handles a light surface with light text", () => {
    const broken: Palette = {
      surface: parseHex("#f6f7fb")!,
      ink: parseHex("#ffffff")!,
      accent: parseHex("#eeeeee")!,
    };
    const fixed = enforceContrast(broken);
    expect(isReadable(fixed)).toBe(true);
    // Darkened, not lightened: the direction follows the surface.
    expect(isDarkColour(fixed.ink)).toBe(true);
  });
});

describe("tokens", () => {
  const palette: Palette = {
    surface: parseHex("#12151f")!,
    ink: parseHex("#f2f5fa")!,
    accent: parseHex("#8ea2ff")!,
  };

  it("covers every token the app themes, so nothing is left half-applied", () => {
    const tokens = tokensFor(palette);
    // Spot-check the ones a hand-written theme forgets: these are what make a
    // partial override look wrong in a way nobody can name.
    for (const key of [
      "--ink",
      "--ink-soft",
      "--ink-faint",
      "--hover-bg",
      "--glass-border",
      "--control-bg",
      "--control-ink",
      "--danger",
      "--panel-bg",
      "--thread",
    ]) {
      expect(tokens[key], `${key} should be set`).toBeTruthy();
    }
    expect(Object.keys(tokens).length).toBe(PALETTE_TOKENS.length);
  });

  it("derives the filled-button pair by inversion, so it cannot fail", () => {
    const tokens = tokensFor(palette);
    // Ink on light-ish surface: the button is ink, its label is the surface.
    expect(tokens["--control-bg"]).toContain("242 245 250");
    expect(tokens["--control-ink"]).toBe("#12151f");
  });

  it("follows the surface rather than the theme label for direction", () => {
    // The direction question this answers: a *light* surface whose colours have
    // to darken, regardless of which base theme is active. The washes follow the
    // panel because the panel is what they sit on.
    const lightSurface: Palette = {
      surface: parseHex("#f2f4f8")!,
      ink: parseHex("#0c0f16")!,
      accent: parseHex("#3b49c4")!,
    };
    const tokens = tokensFor(lightSurface);
    expect(tokens["--hover-bg"]).toContain("12 15 22");
    expect(tokens["--control-ink"]).toBe("#f2f4f8");
  });

  it("writes shadows and highlights in a direction that matches the surface", () => {
    const dark = tokensFor(palette);
    expect(dark["--glass-highlight"]).toContain("0.05");
    const light = tokensFor({
      surface: parseHex("#f6f7fb")!,
      ink: parseHex("#0b0e15")!,
      accent: parseHex("#3b49c4")!,
    });
    expect(light["--glass-highlight"]).toContain("0.9");
  });

  it("leaves the terminal's own palette alone", () => {
    // A terminal has its palette on purpose. It reads `--ink` for its
    // foreground, so it inherits the text colour without having its ANSI
    // colours scrambled by whatever photograph is on screen.
    const tokens = tokensFor(palette);
    expect(Object.keys(tokens).some((key) => key.startsWith("--ansi"))).toBe(false);
    expect(Object.keys(tokens).some((key) => key.startsWith("--terminal"))).toBe(false);
  });
});

describe("hex round trip", () => {
  it("survives serialisation", () => {
    const palette: Palette = {
      surface: parseHex("#12151f")!,
      ink: parseHex("#f2f5fa")!,
      accent: parseHex("#8ea2ff")!,
    };
    expect(fromPaletteHex(toPaletteHex(palette), palette)).toEqual(palette);
  });

  it("falls back per channel rather than failing wholesale", () => {
    // A config hand-edited to a broken value must lose that one colour, not
    // the whole palette.
    const fallback: Palette = {
      surface: parseHex("#12151f")!,
      ink: parseHex("#f2f5fa")!,
      accent: parseHex("#8ea2ff")!,
    };
    const merged = fromPaletteHex({ ink: "not a colour", accent: "#ff0000" }, fallback);
    expect(toHex(merged.ink)).toBe("#f2f5fa");
    expect(toHex(merged.accent)).toBe("#ff0000");
    expect(toHex(merged.surface)).toBe("#12151f");
  });
});

/** Named for the failure it prevents, so a reader knows why it matters. */
describe("no combination of inputs can produce unreadable text", () => {
  const scenes = [DARK_SCENE, LIGHT_SCENE, MONO_SCENE, [swatch("#ff00ff", 1)]];
  const surfaces = ["#000000", "#ffffff", "#7d7d7d", "#12151f", "#f6f7fb"];

  it("holds across every scene, base and hand-picked surface", () => {
    for (const scene of scenes) {
      for (const isDark of [true, false]) {
        const derived = deriveAdaptive(scene, isDark, FALLBACK_ACCENT);
        expect(isReadable(derived), `derived, dark=${isDark}`).toBe(true);

        for (const hex of surfaces) {
          const chosen: Palette = {
            surface: parseHex(hex)!,
            ink: derived.ink,
            accent: derived.accent,
          };
          const fixed = enforceContrast(chosen);
          expect(
            contrast(fixed.ink, fixed.surface),
            `ink ${hex} from scene dark=${isDark}`,
          ).toBeGreaterThanOrEqual(MIN_INK_CONTRAST);
        }
      }
    }
  });
});

/* ---------------------------------------------------------------------------
   Adaptive on a built-in preset

   Built-in backgrounds paint in CSS, so there is no file to decode — but every
   preset carries a `swatch` gradient of its literal colours. These are the
   tests for turning that gradient into the swatches an adaptive palette derives
   from, which is what makes Adaptive do anything at all on the presets. Before
   this, a preset produced no swatches and the mode silently fell back to the
   default accent on every background most people use.
--------------------------------------------------------------------------- */

describe("swatchesFromGradient", () => {
  const weightOf = (css: string, hex: string) => {
    const found = swatchesFromGradient(css).find((s) => toHex(s.colour) === hex);
    return found ? found.weight : 0;
  };

  it("reads each stop, weighted by the span it covers", () => {
    const swatches = swatchesFromGradient(
      "linear-gradient(140deg, #ffffff, #dbe4f5 58%, #e2e3f7)",
    );
    expect(swatches).toHaveLength(3);
    // The span of a stop runs from the midpoint of the gap on its left to the
    // midpoint of the gap on its right: 29%, 50%, 21%. The first stop is
    // therefore *not* the dominant one, which is the whole point of measuring
    // the span rather than counting stops.
    expect(weightOf("linear-gradient(#ffffff, #dbe4f5 58%, #e2e3f7)", "#ffffff")).toBeCloseTo(0.29, 4);
    expect(weightOf("linear-gradient(#ffffff, #dbe4f5 58%, #e2e3f7)", "#dbe4f5")).toBeCloseTo(0.5, 4);
    expect(weightOf("linear-gradient(#ffffff, #dbe4f5 58%, #e2e3f7)", "#e2e3f7")).toBeCloseTo(0.21, 4);
  });

  it("makes the middle stop dominant in a typical preset", () => {
    const swatches = swatchesFromGradient(
      "linear-gradient(140deg, #ffffff, #dbe4f5 58%, #e2e3f7)",
    );
    expect(toHex(swatches[0].colour)).toBe("#dbe4f5");
  });

  it("weights sum to one, so the dominant colour is a real proportion", () => {
    for (const preset of BACKGROUND_PRESETS) {
      const total = swatchesFromGradient(preset.swatch).reduce(
        (sum, swatch) => sum + swatch.weight,
        0,
      );
      expect(total, preset.id).toBeCloseTo(1, 6);
    }
  });

  it("splits the stops evenly when no positions are given", () => {
    const swatches = swatchesFromGradient(
      "linear-gradient(#000000, #111111, #222222)",
    );
    expect(swatches.map((s) => toHex(s.colour))).toEqual([
      "#111111",
      "#000000",
      "#222222",
    ]);
    // 25 / 50 / 25.
    expect(swatches.map((s) => Number(s.weight.toFixed(3)))).toEqual([0.5, 0.25, 0.25]);
  });

  it("treats one colour twice as one colour", () => {
    const swatches = swatchesFromGradient("linear-gradient(#ffffff, #ffffff)");
    expect(swatches).toHaveLength(1);
    expect(swatches[0].weight).toBeCloseTo(1, 6);
  });

  it("returns nothing rather than guessing at a non-hex colour", () => {
    expect(swatchesFromGradient("linear-gradient(rgb(0 0 0), rgb(255 255 255))")).toEqual([]);
    expect(swatchesFromGradient("color-mix(in oklab, red, blue)")).toEqual([]);
    expect(swatchesFromGradient("")).toEqual([]);
  });

  it("never reads a four-digit hex as if it were three", () => {
    // `#abcd` is a valid 4-digit hex (with alpha). Taking `#abc` out of it would
    // invent a colour the stylesheet never asked for, so it is skipped.
    const swatches = swatchesFromGradient("linear-gradient(#abcd, #123456)");
    expect(swatches.map((s) => toHex(s.colour))).toEqual(["#123456"]);
  });

  it("is deterministic, including between equal weights", () => {
    const css = "linear-gradient(#222222, #111111)";
    const first = swatchesFromGradient(css).map((s) => toHex(s.colour));
    expect(first).toEqual(["#111111", "#222222"]);
    expect(swatchesFromGradient(css).map((s) => toHex(s.colour))).toEqual(first);
  });

  it("gives every built-in preset enough to derive a palette from", () => {
    for (const preset of BACKGROUND_PRESETS) {
      const swatches = swatchesFromGradient(preset.swatch);
      expect(swatches.length, preset.id).toBeGreaterThanOrEqual(2);
      const hexes = swatches.map((s) => toHex(s.colour));
      expect(new Set(hexes).size, preset.id).toBe(hexes.length);
    }
  });

  it("gives the theme-following preset a samplable gradient in both themes", () => {
    for (const isDark of [false, true]) {
      const preset = resolvePreset(AUTO_PRESET, isDark);
      expect(
        swatchesFromGradient(preset.swatch).length,
        `${preset.id} dark=${isDark}`,
      ).toBeGreaterThanOrEqual(2);
    }
  });

  it("adapts rather than falling back, which is the bug this fixes", () => {
    const preset = BACKGROUND_PRESETS[0];
    const swatches = swatchesFromGradient(preset.swatch);
    // The fallback accent is what an empty swatch list produced, so an adapted
    // accent that equals it would be indistinguishable from the failure.
    const accent = deriveAdaptive(swatches, false, { r: 0, g: 0, b: 0 }).accent;
    expect(toHex(accent)).not.toBe("#000000");
  });
});

describe("swatchSignature", () => {
  const swatch = (hex: string, weight: number): Swatch => ({
    colour: parseHex(hex)!,
    weight,
  });

  it("names the empty case rather than returning an empty string", () => {
    expect(swatchSignature([])).toBe("none");
  });

  it("is stable for the same swatches", () => {
    const a = [swatch("#ff0000", 0.6), swatch("#00ff00", 0.4)];
    const b = [swatch("#ff0000", 0.6), swatch("#00ff00", 0.4)];
    expect(swatchSignature(a)).toBe(swatchSignature(b));
  });

  it("separates two pictures that a count and a dominant channel could not", () => {
    // This is the regression. The old memo key was `${length}:${dominant.r}`,
    // which is the same string for both of these — so picking the second
    // picture kept the first one's palette, and Adaptive looked like it simply
    // did not update.
    const first = [swatch("#ff0000", 0.6), swatch("#00ff00", 0.4)];
    const second = [swatch("#ff0000", 0.6), swatch("#0000ff", 0.4)];
    expect(`${first.length}:${first[0].colour.r}`).toBe(
      `${second.length}:${second[0].colour.r}`,
    );
    expect(swatchSignature(first)).not.toBe(swatchSignature(second));
  });

  it("separates the same colours in different proportions", () => {
    const a = [swatch("#ff0000", 0.6), swatch("#0000ff", 0.4)];
    const b = [swatch("#ff0000", 0.4), swatch("#0000ff", 0.6)];
    expect(swatchSignature(a)).not.toBe(swatchSignature(b));
  });

  it("ignores float noise that is not a real change", () => {
    const a = [swatch("#ff0000", 0.3333333), swatch("#0000ff", 0.6666667)];
    const b = [swatch("#ff0000", 0.3333334), swatch("#0000ff", 0.6666666)];
    expect(swatchSignature(a)).toBe(swatchSignature(b));
  });

  it("distinguishes two gradients whose stops share a colour", () => {
    const porcelain = swatchesFromGradient(
      "linear-gradient(140deg, #ffffff, #dbe4f5 58%, #e2e3f7)",
    );
    const linen = swatchesFromGradient(
      "linear-gradient(140deg, #ffffff, #e6dcc9 58%, #ded6e6)",
    );
    // Both start on white, and both have three stops.
    expect(toHex(porcelain[1].colour)).toBe("#ffffff");
    expect(toHex(linen[1].colour)).toBe("#ffffff");
    expect(swatchSignature(porcelain)).not.toBe(swatchSignature(linen));
  });
});
