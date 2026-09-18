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
 * Each differs in **frost** as well as refraction, and that is deliberate: the
 * two trade against each other, so "more prominent" means a little less frost as
 * well as a longer throw, and "subtle" means the reverse.
 *
 * `standard` is `DEFAULT_LIQUID` exactly, which is what lets `matchingPreset`
 * name the values the sliders currently spell.
 */
export const GLASS_PRESETS: Record<GlassPresetId, LiquidParams> = {
  subtle: { refraction: 16, frost: 48, saturation: 130, chromatics: 0, elasticity: 0, mode: "standard" },
  standard: { refraction: 32, frost: 38, saturation: 140, chromatics: 1, elasticity: 0, mode: "standard" },
  prominent: { refraction: 64, frost: 28, saturation: 165, chromatics: 2, elasticity: 0, mode: "prominent" },
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
 * `frost` starts at **8**, not at 0. It shipped at 4 with a default of 6, on the
 * reasoning that less blur leaves more detail for the displacement to bend —
 * which is true, and was the wrong thing to optimise. The surfaces this wrapper
 * replaced were painting `blur(24px)` (pill), `blur(34px)` (panel) and
 * `blur(38px)` (panel-strong), so 6px left the artwork plainly legible through
 * every panel and the app read as a film over the wallpaper rather than as glass.
 *
 * 8 is where the *default* stops being reachable downward while still leaving
 * room to experiment: the default is 38, and 8 is the point at which a user has
 * clearly asked for thinner glass than Loom ships. That is a choice worth
 * honouring, which is why the floor is not the default.
 *
 * The ceiling is 60 rather than the library's effective 1600: past roughly 60 the
 * backdrop is smeared flat enough that the refraction stops reading — measured,
 * not assumed (see `scripts/probe-glass.mjs`) — so a larger number would be a
 * slider that makes the effect *worse* while looking like it makes the glass
 * *stronger*.
 */
export const LIQUID_RANGE = {
  refraction: [0, 120],
  // 12, not 8: the floor times the smallest group multiplier must still land at
  // or above the `pill` utility's 24px, so that no setting of this control can
  // make the app's chrome thinner than it was before the feature existed.
  frost: [12, 72],
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
    // On everywhere by default, so the feature is not a setting you have to
    // find before you can see it. `enabled` is the escape hatch, and each group
    // has its own switch for anyone who wants the effect somewhere and not
    // somewhere else.
    enabled: true,
    pills: true,
    composer: true,
    panels: true,
    popovers: true,
    cards: true,
    overlays: true,
    // `DEFAULT_LIQUID` last, so the six numbers cannot be shadowed by the
    // switches above.
    ...DEFAULT_LIQUID,
  },
};

/**
 * How much of a surface's own token survives, per surface group.
 *
 * **Every entry is 100, and that is the whole design.**
 *
 * A converted surface paints its token at exactly the alpha the `panel-strong`
 * (or `pill`) it replaced did. Nothing is traded for the effect: the surface you
 * get is the surface you always had, plus a bent rim.
 *
 * This table exists to hold that at 100 rather than to tune anything, and it is
 * worth being blunt about why, because the first two attempts at this file did
 * real damage by treating opacity as a knob to spend on the effect:
 *
 *   * At a blanket `50` the settings drawer resolved to
 *     `color(srgb 1 1 1 / 0.48)` where `panel-strong` is `0.95` — a
 *     half-transparent sheet carrying the app's prose over the user's wallpaper.
 *   * At a per-group table (78–100) it was merely *worse* than before: the
 *     chrome looked washed rather than glassy, and bought nothing for it.
 *
 * Both were spending legibility on an effect that, measured, is not there: over
 * Loom's own presets the refraction moves **0% of pixels** (max channel delta 2),
 * and no combination of tint, frost and refraction rescues that, because a
 * smooth radial wash has no edges for a displacement map to bend. It is visible
 * over the user's own detailed artwork — 88% of pixels over a hard-edged
 * pattern — and that is a real feature, but it is not one worth one percent of
 * any panel's readability.
 *
 * So the sequencing is: **legible first, and the effect is what is left over.**
 * Anyone who wants more of it has the **Tint** slider, which is one control in
 * one place and whose floor is the honest expression of how far that trade can
 * go before text stops being readable.
 *
 * A group belongs in this table the day it has a *measured* reason to differ.
 * None does today, and the type is here so that the day it does, the change is
 * one number with a name rather than a magic `tintStrength` at a call site.
 */
export const SURFACE_STRENGTH: Record<
  "pills" | "composer" | "panels" | "popovers" | "cards" | "overlays",
  number
> = {
  pills: 100,
  composer: 100,
  panels: 100,
  popovers: 100,
  cards: 100,
  overlays: 100,
};

/**
 * Frost per surface group, as a multiple of the configured `frost`.
 *
 * A multiplier rather than an absolute, so the Frost slider still moves every
 * surface together while each keeps its proportion to the others.
 *
 * Each value makes the surface land on **the blur its own utility was already
 * painting**, at the default frost of 38px:
 *
 *     pills      38 x 0.64 = 24px   the `pill` utility's blur(24px), exactly
 *     composer   38 x 1.00 = 38px   `panel-strong`'s blur(38px), exactly
 *     panels     38 x 1.00 = 38px   (the `panel` utility is 34, so this is above)
 *     popovers   38 x 1.00 = 38px   `panel-strong`, exactly
 *     cards      38 x 1.00 = 38px   `panel-strong`, exactly
 *     overlays   38 x 1.00 = 38px   `panel-strong`, exactly
 *
 * Only the pills differ, and only because theirs was the one utility with a
 * smaller radius than the rest. Everything else is 1.0.
 *
 * **Two earlier versions of this table got it wrong, and both made the app worse
 * than it had been before the feature existed.** The first put every group at the
 * library's own 6px, which left the artwork plainly legible through every panel.
 * The second put the pills at 15px, *below* the `pill` utility's 24px, on the
 * reasoning that less frost means a more visible bend — which is true, and is
 * exactly why it is the wrong thing to spend. Over Loom's own presets the
 * refraction moves 0% of pixels whatever the frost is, so the trade bought
 * nothing and cost the chrome its glass.
 *
 * The rule both failures violated, and `glass.test.ts` now asserts: **at the
 * default, a converted surface is as frosted as what it replaced.** The slider's
 * floor is where a user can deliberately go thinner, and below there is the
 * honest expression of how far that trade goes.
 */
export const SURFACE_FROST: Record<
  "pills" | "composer" | "panels" | "popovers" | "cards" | "overlays",
  number
> = {
  pills: 0.64,
  composer: 1,
  panels: 1,
  popovers: 1,
  cards: 1,
  overlays: 1,
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

/**
 * Lifts a value below the range the UI offers up to the current default.
 *
 * Deliberately not the same as `clamp`, which would raise it only to the floor.
 * The distinction matters for exactly one field: `frost` shipped at **6px**,
 * below the 8px floor this version offers, so clamping would leave a stored
 * config at 8px — legal, and still far too clear for a panel to read as glass
 * (the utilities this replaced were painting `blur(24px)`–`blur(38px)`).
 *
 * A value the slider cannot produce cannot have been chosen deliberately, so the
 * default is the honest answer rather than the floor. A value *inside* the range
 * is somebody's choice and is returned untouched, including the floor itself.
 *
 * This mirrors `apply_preset_defaults` in Rust on purpose, and the duplication is
 * the point: the Rust migration repairs the *file*, and this repairs what is
 * *rendered*. Without it, a running build whose binary predates the migration
 * still shows 8px, and the surfaces look wrong for a reason nothing in the
 * frontend can fix.
 */
function liftIntoRange(value: number, min: number, fallback: number): number {
  if (!Number.isFinite(value)) return fallback;
  return value < min ? fallback : value;
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
    frost: clamp(
      liftIntoRange(
        source.frost ?? DEFAULT_LIQUID.frost,
        LIQUID_RANGE.frost[0],
        DEFAULT_LIQUID.frost,
      ),
      ...LIQUID_RANGE.frost,
      DEFAULT_LIQUID.frost,
    ),
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

/**
 * Forces a whole liquid config — the six numbers and the seven switches.
 *
 * The switches pass through rather than being clamped, because they are not
 * numbers; only their *default* matters, and it is on for all of them. A clamp
 * that dropped them would turn every surface on regardless of the config, which
 * is the opposite of what "off" means.
 */
export function clampLiquid(input: Partial<LiquidGlassConfig> | undefined): LiquidGlassConfig {
  const source = input ?? {};
  const params = clampParams(source);
  return {
    enabled: source.enabled ?? true,
    pills: source.pills ?? true,
    composer: source.composer ?? true,
    panels: source.panels ?? true,
    popovers: source.popovers ?? true,
    cards: source.cards ?? true,
    overlays: source.overlays ?? true,
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
