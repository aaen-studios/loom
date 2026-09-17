/**
 * Liquid glass: the clamps, the presets, and the app-wide CSS variables.
 *
 * Pure, like `lib/background.ts` — no store, no IPC. The settings screen and
 * the component that renders the glass both read from here, so the two cannot
 * disagree about a range.
 *
 * Why the clamps are not defensive dressing: Loom stores its config in Rust as
 * a typed struct with a container-level `#[serde(default)]` and no `extra`
 * catch-all, so a value that fails to deserialize does not degrade one field —
 * `load()` fails and the whole config falls back to defaults, taking every
 * provider and chat setting with it. `refraction` and friends are `u8` there,
 * so a hand-edited `999` is a whole-file risk. Everything that reaches the
 * config is clamped here first.
 */
import { useEffect } from "react";
import type { GlassConfig, GlassMode, LiquidGlassConfig, LiquidParams } from "../types";

/* ---------------------------------------------------------------------------
   Presets
--------------------------------------------------------------------------- */

export type GlassPresetId = "subtle" | "standard" | "prominent";

/**
 * Starting points, not themes.
 *
 * `standard` is `DEFAULT_LIQUID` exactly, so the preset buttons describe the
 * sliders rather than being a second source of truth that drifts from them, and
 * `matchingPreset` can name what the sliders currently spell.
 */
export const GLASS_PRESETS: Record<GlassPresetId, LiquidParams> = {
  subtle: { refraction: 16, frost: 4, saturation: 130, chromatics: 1, elasticity: 0, mode: "standard" },
  standard: { refraction: 32, frost: 6, saturation: 140, chromatics: 2, elasticity: 0, mode: "standard" },
  prominent: { refraction: 64, frost: 10, saturation: 165, chromatics: 3, elasticity: 0, mode: "prominent" },
};

export const PRESET_LABELS: Record<GlassPresetId, string> = {
  subtle: "Subtle",
  standard: "Standard",
  prominent: "Prominent",
};

/* ---------------------------------------------------------------------------
   Defaults and ranges
--------------------------------------------------------------------------- */

export const DEFAULT_LIQUID: LiquidParams = GLASS_PRESETS.standard;

/**
 * Ranges, shared by the clamps and the sliders.
 *
 * `frost` tops out at 40px rather than the library's effective ceiling of about
 * 1600. Past roughly 40 the backdrop is smeared flat enough that the refraction
 * stops reading — measured, not assumed (see `scripts/probe-glass.mjs`) — so a
 * larger number would be a slider that makes the effect *worse* while looking
 * like it makes the glass *stronger*.
 */
export const LIQUID_RANGE = {
  refraction: [0, 120],
  frost: [4, 40],
  saturation: [60, 220],
  chromatics: [0, 5],
  elasticity: [0, 0.5],
} as const;

/** Lower is more see-through, and more glassy. Floor at 50. */
export const TINT_RANGE = [50, 100] as const;
/** Higher is frostier. */
export const BLUR_RANGE = [0, 200] as const;

export const DEFAULT_GLASS: GlassConfig = {
  tint: 100,
  blur: 100,
  liquid: {
    // On by default once the surfaces are wired, so the feature is not a
    // setting you have to find before you can see it. `enabled` is the escape.
    enabled: true,
    pills: true,
    composer: true,
    panels: true,
    ...DEFAULT_LIQUID,
  },
};

/** The library always adds this much blur before its own scaling. */
const LIBRARY_BASE_BLUR_PX = 4;
/** And multiplies `blurAmount` by this. */
const LIBRARY_BLUR_SCALE = 32;

/**
 * Converts our px to the library's `blurAmount` units.
 *
 * The two scales are wildly different and the first version of this file got it
 * wrong in a way that silently killed the effect: `blurAmount` is multiplied by
 * 32 inside the library, so passing a plausible-looking `5` produced **164px**
 * of backdrop blur — a flat wash with no detail left for the displacement map
 * to bend. See the liquid glass region in `styles.css`.
 */
export function toBlurAmount(frostPx: number): number {
  return Math.max(0, (frostPx - LIBRARY_BASE_BLUR_PX) / LIBRARY_BLUR_SCALE);
}

/** The inverse, for labelling a slider with the number it will really produce. */
export function fromBlurAmount(blurAmount: number): number {
  return Math.round(LIBRARY_BASE_BLUR_PX + blurAmount * LIBRARY_BLUR_SCALE);
}

/* ---------------------------------------------------------------------------
   Clamps
--------------------------------------------------------------------------- */

function clamp(value: number, min: number, max: number, fallback: number): number {
  if (!Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(min, value));
}

export const LIQUID_MODES: GlassMode[] = ["standard", "polar", "prominent"];

const MODE_LABELS: Record<GlassMode, string> = {
  standard: "Standard",
  polar: "Polar",
  prominent: "Prominent",
};

export function modeLabel(mode: GlassMode): string {
  return MODE_LABELS[mode];
}

/** Forces the six numbers into range, field by field. */
export function clampParams(input: Partial<LiquidParams> | undefined): LiquidParams {
  const source = input ?? {};
  return {
    refraction: clamp(source.refraction ?? DEFAULT_LIQUID.refraction, ...LIQUID_RANGE.refraction, DEFAULT_LIQUID.refraction),
    frost: clamp(source.frost ?? DEFAULT_LIQUID.frost, ...LIQUID_RANGE.frost, DEFAULT_LIQUID.frost),
    saturation: clamp(source.saturation ?? DEFAULT_LIQUID.saturation, ...LIQUID_RANGE.saturation, DEFAULT_LIQUID.saturation),
    chromatics: clamp(source.chromatics ?? DEFAULT_LIQUID.chromatics, ...LIQUID_RANGE.chromatics, DEFAULT_LIQUID.chromatics),
    elasticity: clamp(source.elasticity ?? DEFAULT_LIQUID.elasticity, ...LIQUID_RANGE.elasticity, DEFAULT_LIQUID.elasticity),
    // An unknown mode is not a clamp failure — it is a preset that was retired,
    // and falling back quietly is the same migration story `presetById` has.
    mode: LIQUID_MODES.includes(source.mode as GlassMode)
      ? (source.mode as GlassMode)
      : DEFAULT_LIQUID.mode,
  };
}

/** Forces a whole liquid config — the six numbers and the four switches. */
export function clampLiquid(input: Partial<LiquidGlassConfig> | undefined): LiquidGlassConfig {
  const source = input ?? {};
  const params = clampParams(source);
  return {
    enabled: source.enabled ?? true,
    pills: source.pills ?? true,
    composer: source.composer ?? true,
    panels: source.panels ?? true,
    ...params,
  };
}

export function clampTint(value: number | undefined): number {
  return Math.round(clamp(value ?? DEFAULT_GLASS.tint, ...TINT_RANGE, DEFAULT_GLASS.tint));
}

export function clampBlur(value: number | undefined): number {
  return Math.round(clamp(value ?? DEFAULT_GLASS.blur, ...BLUR_RANGE, DEFAULT_GLASS.blur));
}

/**
 * Forces a whole glass config into range.
 *
 * Every consumer goes through this — the settings sliders *and* the component
 * that renders the glass — so a config edited by hand cannot reach the library
 * with a value the sliders could not have produced. One clamp, one answer.
 */
export function clampGlass(input: Partial<GlassConfig> | undefined): GlassConfig {
  const source = input ?? {};
  return {
    tint: clampTint(source.tint),
    blur: clampBlur(source.blur),
    liquid: clampLiquid(source.liquid),
  };
}

/** Which preset these params currently spell, if any (for the picker). */
export function matchingPreset(params: LiquidParams): GlassPresetId | null {
  const ids: GlassPresetId[] = ["subtle", "standard", "prominent"];
  for (const id of ids) {
    const preset = GLASS_PRESETS[id];
    if (
      preset.refraction === params.refraction &&
      preset.frost === params.frost &&
      preset.saturation === params.saturation &&
      preset.chromatics === params.chromatics &&
      preset.elasticity === params.elasticity &&
      preset.mode === params.mode
    ) {
      return id;
    }
  }
  return null;
}

/* ---------------------------------------------------------------------------
   The app-wide variables

   Two multipliers over every `pill`, `panel`, `panel-strong` and `blob` in the
   app, read by the unlayered overrides in `styles.css`. Numbers written as bare
   values rather than lengths, so the stylesheet can do arithmetic on them with
   `calc()` and `color-mix()`.
--------------------------------------------------------------------------- */

/** The custom properties, as a plain record. Pure, so it can be unit-tested. */
export function glassVars(glass: GlassConfig): Record<string, string> {
  return {
    "--glass-tint": String(clampTint(glass.tint)),
    "--glass-blur": String(clampBlur(glass.blur)),
  };
}

/**
 * Writes the two multipliers onto `<html>`.
 *
 * Depend on the **clamped primitives**, not on the config object and not on the
 * record `glassVars` returns: either would be a fresh reference on every render
 * and re-fire the effect forever. `audit-selectors.mjs` cannot catch that — it
 * only inspects hooks whose first argument is a store selector, and `useEffect`
 * is in its own skip list — so it is on the reader to keep this dependency list
 * primitive.
 */
export function useGlassVars(glass: GlassConfig): void {
  const tint = clampTint(glass.tint);
  const blur = clampBlur(glass.blur);

  useEffect(() => {
    const root = document.documentElement.style;
    root.setProperty("--glass-tint", String(tint));
    root.setProperty("--glass-blur", String(blur));
  }, [tint, blur]);
}
