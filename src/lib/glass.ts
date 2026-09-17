/**
 * Liquid glass parameters, and the clamps that keep them safe.
 *
 * Pure, like `lib/background.ts`: no React, no store, no IPC. The component
 * reads parameters from here and the settings screen will write them here, so
 * the two cannot disagree about a range.
 *
 * The clamps are not defensive dressing. Loom stores its config in Rust as a
 * typed struct with a container-level `#[serde(default)]` and no `extra` map,
 * so a value that fails to deserialize does not degrade one field — `load()`
 * fails and the whole config falls back to defaults. A hand-edited number is
 * therefore a whole-file risk, and every value that reaches the config is
 * clamped here first.
 */

/**
 * Refraction mode.
 *
 * `shader` is deliberately absent, not merely hidden: it rasterises the
 * displacement map pixel by pixel in a nested loop on mount *and* on every
 * resize, which is the wrong trade for a 40px pill and worse for the composer.
 * The three bitmap modes cost nothing to switch between.
 */
export type GlassMode = "standard" | "polar" | "prominent";

/**
 * What actually reaches `liquid-glass-react`.
 *
 * Named for what they do rather than after the library's props, because two of
 * the mappings are not one-to-one: `frost` becomes `4 + frost * 32` px of
 * backdrop blur inside the library, and `refraction` is a displacement scale in
 * source pixels that reads very differently on a 40px pill than on the
 * composer's ~90px sheet.
 */
export interface LiquidParams {
  /** `displacementScale`: how far the edge samples are pulled. 0–120. */
  refraction: number;
  /**
   * Backdrop blur, in **pixels** — not the library's own `blurAmount` units.
   *
   * The two are wildly different scales, and the first draft of this got it
   * wrong in a way that silently killed the effect. The library computes
   * `blur((4 + blurAmount * 32)px)`, so its default of `0.0625` is 6px, and a
   * plausible-looking integer like `5` becomes **164px**: the backdrop is
   * smeared into a flat wash with no detail left for the displacement map to
   * bend, and the surface reads as ordinary frosted glass. Measured, not
   * assumed — see the liquid glass region in `styles.css`.
   *
   * So this is px, and `toBlurAmount` below converts. The 4px floor is the
   * library's own, since it always adds 4 when `overLight` is off.
   */
  frost: number;
  /** `saturation`: percent. 100 is neutral. */
  saturation: number;
  /** `aberrationIntensity`: chromatic fringing on the rim. 0–5. */
  chromatics: number;
  /** `elasticity`: how far the surface follows the pointer. 0–0.5. */
  elasticity: number;
  mode: GlassMode;
}

/**
 * Elasticity is 0 by default, and that is a deliberate choice rather than a
 * timid one. The library moves the surface with the pointer from up to 200px
 * away, which on a title bar means the pills shift as you cross the window to
 * reach a menu. It is a lovely effect on a demo card and an irritation on
 * chrome you are aiming at; the slider is there for anyone who disagrees.
 */
export const DEFAULT_LIQUID: LiquidParams = {
  refraction: 32,
  // 6px: the library's own default, and about as much as a 40px pill can take
  // before the backdrop has nothing left to refract.
  frost: 6,
  saturation: 140,
  chromatics: 2,
  elasticity: 0,
  mode: "standard",
};

/**
 * Ranges, shared by the clamps and the sliders.
 *
 * `frost` tops out at 40px rather than the library's effective 1600: past about
 * 40 the backdrop is flat enough that the refraction stops reading, so a larger
 * number would be a slider that makes the effect *worse* while looking like it
 * makes the glass *stronger*.
 */
export const LIQUID_RANGE = {
  refraction: [0, 120],
  frost: [4, 40],
  saturation: [60, 220],
  chromatics: [0, 5],
  elasticity: [0, 0.5],
} as const;

/** The library always adds this much; it is why `frost` has a floor of 4px. */
const LIBRARY_BASE_BLUR_PX = 4;
/** And multiplies `blurAmount` by this. */
const LIBRARY_BLUR_SCALE = 32;

/** Converts our px to the library's `blurAmount` units. */
export function toBlurAmount(frostPx: number): number {
  return Math.max(0, (frostPx - LIBRARY_BASE_BLUR_PX) / LIBRARY_BLUR_SCALE);
}

/** The inverse, for labelling a slider with the number it will actually produce. */
export function fromBlurAmount(blurAmount: number): number {
  return Math.round(LIBRARY_BASE_BLUR_PX + blurAmount * LIBRARY_BLUR_SCALE);
}

export type GlassPresetId = "subtle" | "standard" | "prominent";

/**
 * Starting points, not themes.
 *
 * `standard` is `DEFAULT_LIQUID` exactly, so the preset buttons describe the
 * sliders rather than being a second source of truth that drifts from them.
 */
export const GLASS_PRESETS: Record<GlassPresetId, LiquidParams> = {
  subtle: { ...DEFAULT_LIQUID, refraction: 16, frost: 3, chromatics: 1 },
  standard: { ...DEFAULT_LIQUID },
  prominent: { ...DEFAULT_LIQUID, refraction: 64, frost: 9, chromatics: 3 },
};

function clamp(value: number, min: number, max: number, fallback: number): number {
  if (!Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(min, value));
}

/**
 * Forces a params object into range, field by field.
 *
 * Every consumer goes through this — the settings sliders *and* the component
 * that renders the glass — so a config edited by hand cannot reach the library
 * with a value the sliders could not have produced. One clamp, one answer.
 */
export function clampLiquid(input: Partial<LiquidParams> | undefined): LiquidParams {
  const source = input ?? {};
  const [refractionMin, refractionMax] = LIQUID_RANGE.refraction;
  const [frostMin, frostMax] = LIQUID_RANGE.frost;
  const [saturationMin, saturationMax] = LIQUID_RANGE.saturation;
  const [chromaticsMin, chromaticsMax] = LIQUID_RANGE.chromatics;
  const [elasticityMin, elasticityMax] = LIQUID_RANGE.elasticity;

  return {
    refraction: clamp(source.refraction ?? DEFAULT_LIQUID.refraction, refractionMin, refractionMax, DEFAULT_LIQUID.refraction),
    frost: clamp(source.frost ?? DEFAULT_LIQUID.frost, frostMin, frostMax, DEFAULT_LIQUID.frost),
    saturation: clamp(source.saturation ?? DEFAULT_LIQUID.saturation, saturationMin, saturationMax, DEFAULT_LIQUID.saturation),
    chromatics: clamp(source.chromatics ?? DEFAULT_LIQUID.chromatics, chromaticsMin, chromaticsMax, DEFAULT_LIQUID.chromatics),
    elasticity: clamp(source.elasticity ?? DEFAULT_LIQUID.elasticity, elasticityMin, elasticityMax, DEFAULT_LIQUID.elasticity),
    // An unknown id is not a clamp failure — it is a preset that was retired,
    // and falling back quietly is the same migration story `presetById` has.
    mode: LIQUID_MODES.includes(source.mode as GlassMode)
      ? (source.mode as GlassMode)
      : DEFAULT_LIQUID.mode,
  };
}

export const LIQUID_MODES: GlassMode[] = ["standard", "polar", "prominent"];

/** Which preset a params object currently equals, if any (for the picker). */
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
   App-wide glass tuning

   Two multipliers over every `pill`, `panel`, `panel-strong` and `blob` in the
   app. Both default to 100, so the app looks exactly as it does today until a
   slider moves — which matters, because light theme over bright artwork is
   already the weakest point in the design and it should not be traded away
   silently by shipping this feature.
--------------------------------------------------------------------------- */

/** Lower is more see-through, and more glassy. Floor at 50. */
export const TINT_RANGE = [50, 100] as const;
/** Higher is frostier. */
export const BLUR_RANGE = [0, 200] as const;

export const DEFAULT_TINT = 100;
export const DEFAULT_BLUR = 100;

export function clampTint(value: number | undefined): number {
  return Math.round(clamp(value ?? DEFAULT_TINT, TINT_RANGE[0], TINT_RANGE[1], DEFAULT_TINT));
}

export function clampBlur(value: number | undefined): number {
  return Math.round(clamp(value ?? DEFAULT_BLUR, BLUR_RANGE[0], BLUR_RANGE[1], DEFAULT_BLUR));
}
