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
  subtle: { refraction: 16, frost: 38, saturation: 130, chromatics: 0, elasticity: 0, mode: "standard" },
  standard: { refraction: 32, frost: 30, saturation: 140, chromatics: 1, elasticity: 0, mode: "standard" },
  prominent: { refraction: 64, frost: 20, saturation: 165, chromatics: 2, elasticity: 0, mode: "prominent" },
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
 * `frost` starts at **8**, not at 0, and that floor is a correction rather than a
 * preference. It shipped at 4, with a default of 6, on the reasoning that less
 * blur leaves more detail for the displacement to bend — which is true, and was
 * the wrong thing to optimise. The surfaces this wrapper replaced were painting
 * `blur(24px)` (pill), `blur(34px)` (panel) and `blur(38px)` (panel-strong), so
 * 6px left the artwork plainly legible through every panel and the app read as a
 * film over the wallpaper rather than as glass.
 *
 * The ceiling is 60 rather than the library's effective 1600: past roughly 60
 * the backdrop is smeared flat enough that the refraction stops reading —
 * measured, not assumed (see `scripts/probe-glass.mjs`) — so a larger number
 * would be a slider that makes the effect *worse* while looking like it makes
 * the glass *stronger*.
 */
export const LIQUID_RANGE = {
  refraction: [0, 120],
  frost: [8, 60],
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
 * **This is the table that matters most, and the first attempt got it badly
 * wrong.** It used one default of 50 everywhere, which halved every surface's
 * alpha. Measured afterwards:
 *
 *     the `panel-strong` utility    color(srgb 1 1 1 / 0.95), blur(38px)
 *     the settings drawer after     color(srgb 1 1 1 / 0.48), frost 6px
 *     the title-bar pills after     0.45, against `--pill-bg`'s 0.90
 *
 * A half-transparent sheet with a 6px blur, with the app's prose on it. Glass
 * became a barely-legible film over whatever the user's wallpaper happened to be.
 *
 * Two things made that worse than it looks. The 6px frost was chosen to *help*
 * the effect — less blur leaves more detail to bend — so it traded legibility
 * away for something that, measured over Loom's own presets, is imperceptible
 * anyway: 0% of pixels bend, max channel delta 2, and no combination of frost
 * and refraction rescues it because a smooth wash has no edges to bend.
 *
 * So the rule is inverted from where it started: a converted surface keeps the
 * alpha of the utility it replaced, and only a group with a **measured** reason
 * spends any of it. There is exactly one such group, the pills, and its entry
 * below explains the reason. Nothing spends opacity for an effect that cannot be
 * seen — which is what the first version did, on every surface at once.
 *
 *   pills      78   `--pill-bg` is 90% light, so this lands at 0.70.
 *                   The one group with a spend, and the number is reasoned
 *                   rather than taste: **tint and visibility are not the same
 *                   lever.** The displacement acts on the backdrop *behind* the
 *                   element whatever the tint is — a heavier tint only means you
 *                   see less of the result. At 0.70 you still see 30% of the
 *                   backdrop at the rim, shifted, which is what reads as a bend,
 *                   while icons keep the contrast they had. At the 0.52 this was
 *                   set to first, the chrome looked washed rather than glassy and
 *                   bought nothing for it.
 *
 *                   The pills are the right group for even this much: they hold
 *                   icons and never a sentence, and they sit over the user's own
 *                   artwork, which is the only backdrop in the app where the
 *                   effect is visible at all (measured: 88% of pixels over a
 *                   photograph, against 0% over the built-in presets).
 *   composer   92   The surface messages are typed into, so it stays close to
 *                   opaque. The refraction on it is effectively invisible, and
 *                   that is the right trade.
 *   panels     96   Docked panels hold tables, file lists and shells.
 *   popovers   88   Menus sit over the transcript, where the bend reads against
 *                   text and code, so this is the second group that spends a
 *                   little — and 88% of a 95% token is still 84%, which stays
 *                   comfortably legible behind a 36px frost.
 *   cards      90   Tool calls and cards, also over the transcript.
 *   overlays  100   The settings drawer, the shortcut sheet, voice mode. The most
 *                   text in the app on the largest surfaces, and legibility here
 *                   is not negotiable. Exactly what they had before.
 */
export const SURFACE_STRENGTH: Record<
  "pills" | "composer" | "panels" | "popovers" | "cards" | "overlays",
  number
> = {
  pills: 78,
  composer: 92,
  panels: 96,
  popovers: 88,
  cards: 90,
  overlays: 100,
};

/**
 * Frost per surface group, as a multiple of the configured `frost`.
 *
 * A multiplier rather than an absolute, so the Frost slider stays meaningful
 * everywhere while a 40px pill and a 720px drawer each get the frost they can
 * carry.
 *
 * **The two groups are doing opposite jobs here, and that is the point.**
 *
 * For everything that holds text, the value lands on or above the blur the
 * utility it replaced used — `panel-strong` was `blur(38px)`, `panel` was
 * `blur(34px)` — so a converted surface is exactly as frosted as it always was:
 *
 *     composer   30 x 1.10 = 33px
 *     panels     30 x 1.15 = 35px
 *     popovers   30 x 1.20 = 36px
 *     cards      30 x 1.10 = 33px
 *     overlays   30 x 1.25 = 38px   `panel-strong`'s 38px, exactly
 *
 * For the **pills** it is deliberately *below* the `pill` utility's 24px, and
 * that is the one place this table spends frost rather than protecting it. The
 * reason is a conflict the earlier versions of this file never resolved: the two
 * properties the library puts on one element run in an order that makes them
 * enemies.
 *
 *     backdrop-filter: blur(Npx)     blurs what is behind
 *     filter: url(#displacement)     then displaces that blur
 *
 * The frost destroys the detail *before* the displacement reaches it, so a bend
 * only shows when the blur is smaller than the scale of the structure behind. The
 * `pill` utility's 24px is over half of the 40px capsule it sits in, and over a
 * photograph it averages the detail into a wash — a 32px displacement of a wash
 * looks like nothing at all. Measured over Loom's own presets it is 0% of pixels,
 * and no combination of frost and refraction rescues that, because a smooth
 * radial wash has no edges to move.
 *
 * So the pills take **15px**: still plainly frosted, small enough that real
 * artwork keeps its structure for the displacement to act on. They are the one
 * group that can afford the trade — icons, never a sentence — and they sit over
 * the user's own wallpaper, which is the only backdrop in the app where this
 * effect is visible at all.
 */
export const SURFACE_FROST: Record<
  "pills" | "composer" | "panels" | "popovers" | "cards" | "overlays",
  number
> = {
  pills: 0.5,
  composer: 1.1,
  panels: 1.15,
  popovers: 1.2,
  cards: 1.1,
  overlays: 1.25,
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
