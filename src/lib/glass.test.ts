import { describe, expect, it } from "vitest";
import {
  BLUR_RANGE,
  DEFAULT_GLASS,
  DEFAULT_LIQUID,
  GLASS_PRESETS,
  LIQUID_RANGE,
  SURFACE_FROST,
  TINT_RANGE,
  clampBlur,
  clampGlass,
  clampLiquid,
  clampParams,
  clampTint,
  fromBlurAmount,
  glassVars,
  matchingPreset,
  toBlurAmount,
} from "./glass";

/**
 * These tests exist because a bad number here is not a cosmetic problem. The
 * Rust side stores `refraction`, `frost`, `saturation` and `chromatics` as
 * `u8`, and `config::load()` falls back to a *whole* default config when a
 * field fails to deserialize — so one value out of range costs the user every
 * provider, every chat setting, and every workspace. `clampGlass` is the only
 * thing standing between a hand-edited `config.json` and that.
 */
describe("the clamps", () => {
  it("keeps every field inside the range the sliders offer", () => {
    const wild = clampParams({
      refraction: 999,
      frost: -12,
      saturation: 100_000,
      chromatics: -1,
      elasticity: 47,
    });

    const [rMin, rMax] = LIQUID_RANGE.refraction;
    const [fMin, fMax] = LIQUID_RANGE.frost;
    const [sMin, sMax] = LIQUID_RANGE.saturation;
    const [cMin, cMax] = LIQUID_RANGE.chromatics;
    const [eMin, eMax] = LIQUID_RANGE.elasticity;

    expect(wild.refraction).toBeGreaterThanOrEqual(rMin);
    expect(wild.refraction).toBeLessThanOrEqual(rMax);
    expect(wild.frost).toBeGreaterThanOrEqual(fMin);
    expect(wild.frost).toBeLessThanOrEqual(fMax);
    expect(wild.saturation).toBeGreaterThanOrEqual(sMin);
    expect(wild.saturation).toBeLessThanOrEqual(sMax);
    expect(wild.chromatics).toBeGreaterThanOrEqual(cMin);
    expect(wild.chromatics).toBeLessThanOrEqual(cMax);
    expect(wild.elasticity).toBeGreaterThanOrEqual(eMin);
    expect(wild.elasticity).toBeLessThanOrEqual(eMax);
  });

  it("lifts a stored frost below the floor to the default, not to the floor", () => {
    // `frost` shipped at 6px, which is below the 12px floor the slider now
    // offers. Clamping would leave it at the floor — legal, and still far too
    // clear for a panel to read as glass, since the utilities this replaced were
    // painting blur(24px) to blur(38px). A value the slider cannot produce
    // cannot have been chosen, so it becomes the default.
    expect(clampParams({ frost: 6 }).frost).toBe(DEFAULT_LIQUID.frost);
    expect(clampParams({ frost: 4 }).frost).toBe(DEFAULT_LIQUID.frost);

    // But a value *inside* the range is somebody's choice, floor included.
    expect(clampParams({ frost: LIQUID_RANGE.frost[0] }).frost).toBe(LIQUID_RANGE.frost[0]);
    expect(clampParams({ frost: 44 }).frost).toBe(44);
  });

  it("gives every surface group at least the frost the utility it replaced used", () => {
    // The rule `SURFACE_FROST` encodes, asserted rather than described: a
    // converted surface must never be *thinner* than the plain `pill`, `panel`
    // or `panel-strong` it was converted from. The first version of the table
    // broke this — it put the pills at 15px against the `pill` utility's 24px —
    // and the app's chrome ended up less frosted than it had ever been.
    const ORIGINAL = { pill: 24, panel: 34, "panel-strong": 38 } as const;
    const REPLACES = {
      pills: "pill",
      composer: "panel-strong",
      panels: "panel",
      popovers: "panel-strong",
      cards: "panel-strong",
      overlays: "panel-strong",
    } as const;

    for (const [group, utility] of Object.entries(REPLACES)) {
      // The worst case is the slider's own floor, since the multipliers only
      // ever raise it from there.
      const atFloor = LIQUID_RANGE.frost[0] * SURFACE_FROST[group as keyof typeof SURFACE_FROST];
      const atDefault = DEFAULT_LIQUID.frost * SURFACE_FROST[group as keyof typeof SURFACE_FROST];
      expect(
        atDefault,
        `${group} is less frosted at the default than the ${utility} it replaced`,
      ).toBeGreaterThanOrEqual(ORIGINAL[utility]);
      expect(
        atFloor,
        `${group} drops below the ${utility} it replaced at the slider's floor`,
      ).toBeGreaterThanOrEqual(ORIGINAL[utility] * 0.75);
    }
  });

  it("treats a non-finite number as absent rather than clamping it", () => {
    // `Math.min(120, NaN)` is NaN, and NaN would then be written into a `u8`
    // field. The fallback path matters more than the clamp for these.
    const broken = clampParams({
      refraction: Number.NaN,
      frost: Number.POSITIVE_INFINITY,
      elasticity: Number.NaN,
    });
    expect(broken.refraction).toBe(DEFAULT_LIQUID.refraction);
    expect(broken.frost).toBe(DEFAULT_LIQUID.frost);
    expect(broken.elasticity).toBe(DEFAULT_LIQUID.elasticity);
  });

  it("fills a missing field from the default instead of leaving a hole", () => {
    expect(clampParams({})).toEqual(DEFAULT_LIQUID);
    expect(clampParams(undefined)).toEqual(DEFAULT_LIQUID);
  });

  it("falls back on an unknown mode rather than refusing the config", () => {
    // A retired mode is a migration story, not a load failure — the same call
    // `presetById` makes for a retired background preset.
    const stale = clampParams({ mode: "shader" as never });
    expect(stale.mode).toBe(DEFAULT_LIQUID.mode);
  });

  it("never yields a value the Rust u8 fields cannot hold", () => {
    for (const value of [-1, 0, 51, 255, 256, 1e9]) {
      const glass = clampGlass({
        tint: value,
        blur: value,
        liquid: {
          ...DEFAULT_GLASS.liquid,
          refraction: value,
          frost: value,
          saturation: value,
          chromatics: value,
        },
      });
      for (const field of [
        glass.tint,
        glass.blur,
        glass.liquid.refraction,
        glass.liquid.frost,
        glass.liquid.saturation,
        glass.liquid.chromatics,
      ]) {
        expect(Number.isInteger(field)).toBe(true);
        expect(field).toBeGreaterThanOrEqual(0);
        expect(field).toBeLessThanOrEqual(255);
      }
    }
  });
});

describe("the presets", () => {
  it("round-trips: every preset is named by matchingPreset", () => {
    for (const [id, params] of Object.entries(GLASS_PRESETS)) {
      expect(matchingPreset(params)).toBe(id);
    }
  });

  it("names nothing for a set of values that is not a preset", () => {
    expect(matchingPreset({ ...DEFAULT_LIQUID, refraction: 33 })).toBeNull();
  });

  it("has `standard` as the default, so the buttons describe the sliders", () => {
    // The preset picker and the sliders are two views of one value. If these
    // ever differ, pressing "Standard" would move the sliders, which reads as
    // a bug even though nothing is broken.
    expect(DEFAULT_LIQUID).toEqual(GLASS_PRESETS.standard);
  });

  it("keeps frost inside the range the slider offers", () => {
    // A preset outside the range is a preset the UI cannot represent: the slider
    // would clamp it, `matchingPreset` would stop matching, and the "Standard"
    // chip would silently stop naming the values it had just set.
    const [min, max] = LIQUID_RANGE.frost;
    for (const [id, preset] of Object.entries(GLASS_PRESETS)) {
      expect(preset.frost, `${id} frost is below the slider floor`).toBeGreaterThanOrEqual(min);
      expect(preset.frost, `${id} frost is above the slider ceiling`).toBeLessThanOrEqual(max);
    }
  });

  it("trades frost against refraction rather than raising both", () => {
    // The two work against each other: more blur means less detail for the
    // displacement to bend. A "prominent" preset with *more* frost than
    // "subtle" would be claiming to be stronger while blurring away the thing
    // that makes it strong.
    const { subtle, prominent } = GLASS_PRESETS;
    expect(prominent.refraction).toBeGreaterThan(subtle.refraction);
    expect(prominent.frost).toBeLessThan(subtle.frost);
  });
});
describe("blur units", () => {
  it("converts px to the library's own scale", () => {
    // The library computes `blur((4 + blurAmount * 32)px)`. The default 6px is
    // its own 0.0625, and getting this wrong is what silently killed the whole
    // effect: `blurAmount: 5` is 164px of backdrop blur, which smears the
    // backdrop so flat the displacement map has nothing left to bend.
    expect(toBlurAmount(6)).toBeCloseTo(0.0625, 4);
    expect(toBlurAmount(36)).toBeCloseTo(1, 4);
    expect(fromBlurAmount(0.0625)).toBe(6);
  });

  it("never goes negative, because the library's base is a floor", () => {
    // The library always adds 4px, so asking for less than that is not
    // expressible; 0 is the closest it can get.
    expect(toBlurAmount(0)).toBe(0);
    expect(toBlurAmount(2)).toBe(0);
  });

  it("round-trips the whole slider range", () => {
    const [min, max] = LIQUID_RANGE.frost;
    for (let px = min; px <= max; px += 1) {
      expect(fromBlurAmount(toBlurAmount(px))).toBe(px);
    }
  });
});

describe("the app-wide variables", () => {
  it("is a no-op at the defaults", () => {
    // The single most important property of this feature: a fresh install and
    // every existing user look exactly as they did before until a slider moves.
    expect(glassVars(DEFAULT_GLASS)).toEqual({
      "--glass-tint": "100",
      "--glass-blur": "100",
    });
  });

  it("emits bare numbers, not lengths", () => {
    // The stylesheet does `calc(var(--glass-blur, 100) / 100)` and
    // `color-mix(... var(--glass-tint) * 1%)`. A stray `%` or `px` here would
    // make those declarations invalid at computed-value time, which fails
    // silently — the surface simply paints nothing.
    const vars = glassVars({ ...DEFAULT_GLASS, tint: 72, blur: 140 });
    expect(vars["--glass-tint"]).toBe("72");
    expect(vars["--glass-blur"]).toBe("140");
    for (const value of Object.values(vars)) {
      expect(value).toMatch(/^\d+$/);
    }
  });

  it("clamps what it writes, so the stylesheet cannot get a bad number", () => {
    const vars = glassVars({
      ...DEFAULT_GLASS,
      tint: 500,
      blur: -20,
    });
    expect(vars["--glass-tint"]).toBe(String(TINT_RANGE[1]));
    expect(vars["--glass-blur"]).toBe(String(BLUR_RANGE[0]));
  });

  it("clamps tint and blur independently", () => {
    expect(clampTint(10)).toBe(TINT_RANGE[0]);
    expect(clampBlur(10_000)).toBe(BLUR_RANGE[1]);
  });
});

describe("clampLiquid", () => {
  it("keeps the switches out of the clamp's way", () => {
    // The four booleans are not numbers; a clamp that dropped them would turn
    // every surface on, which is the opposite of what "off" means.
    const off = clampLiquid({
      ...DEFAULT_GLASS.liquid,
      enabled: false,
      pills: false,
      composer: true,
      panels: false,
    });
    expect(off.enabled).toBe(false);
    expect(off.pills).toBe(false);
    expect(off.composer).toBe(true);
    expect(off.panels).toBe(false);
  });

  it("defaults the switches to on when the whole block is absent", () => {
    const filled = clampLiquid(undefined);
    expect(filled.enabled).toBe(true);
    expect(filled).toEqual(DEFAULT_GLASS.liquid);
  });
});
