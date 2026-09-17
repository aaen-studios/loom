import type { CSSProperties } from "react";
import type { BackgroundConfig } from "../types";

/**
 * The window backdrop: the colour behind everything, including before the first
 * painted layer exists.
 *
 * Four places need the same value — `index.html` before the bundle loads,
 * `main.tsx` before React mounts, `App` on every theme change, and the Porcelain
 * preset's own base — so it is defined once here and asserted by
 * `background.test.ts` rather than repeated as a literal in each.
 */
export const THEME_BACKDROP: Record<"light" | "dark", string> = {
  light: "#eef1f7",
  dark: "#070a12",
};

/** The backdrop for a theme, so no caller repeats the ternary. */
export function backdropFor(isDark: boolean): string {
  return isDark ? THEME_BACKDROP.dark : THEME_BACKDROP.light;
}

export interface BackgroundPreset {
  id: string;
  name: string;
  /**
   * Which theme this preset is authored for.
   *
   * Not decoration: dark mode veils the background (`DARK_DIM_FLOOR`), and a
   * near-white preset under a 48% dark veil is mud — no colour of its own, and
   * nothing like the swatch in Settings. Tagging the tone is what lets the
   * picker group them and lets "Automatic" choose sensibly.
   */
  tone: "light" | "dark";
  /** Base fill underneath the layers. */
  base: string;
  /** Layers painted across the background area, topmost first. */
  layers: string;
  /** Swatch preview for the settings grid. */
  swatch: string;
}

/**
 * The id that means "pick by theme".
 *
 * The default, and the reason the built-in background used to look like it was
 * missing from the picker: the stored preset was `porcelain` — a light preset —
 * under a dark veil, so the window was grey and matched no swatch on offer. A
 * theme-aware default cannot have that problem.
 */
export const AUTO_PRESET = "auto";

/** Which preset `auto` resolves to for each theme. */
const AUTO_FOR_THEME: Record<"light" | "dark", string> = {
  light: "porcelain",
  // The one neutral dark preset. A default should be quiet, and every other
  // dark preset here carries a strong hue.
  dark: "graphite",
};

/**
 * The house texture: two hairline threads crossing at right angles, the way a
 * warp crosses a weft. It should be plainly visible, and it earns its place
 * for two reasons.
 *
 * 1. `backdrop-filter` can only frost what is actually behind it. A perfectly
 *    flat background blurs to a perfectly flat result, so the glass panels
 *    collapse into plain translucency and the whole window loses its depth. A
 *    whisper of texture gives the blur something to smear.
 * 2. Large fields of a very subtle gradient band visibly on some panels.
 *    Breaking the surface with a hairline hides the steps.
 *
 * The two directions use periods two pixels apart on purpose: an exact grid
 * reads as graph paper, an incommensurate one reads as cloth.
 *
 * The alphas carry all of this, and they cannot be chosen by eye in isolation.
 * Two things eat the thread before it reaches the screen: the drifting layer
 * scales it (see `loom-drift`, 1.08 → 1.14), which resamples every 1px hairline
 * and costs it roughly a third of its peak alpha, and dark presets lose more
 * again to the `DARK_DIM_FLOOR` veil. A thread authored at 0.02 arrives at
 * something under a two-level channel shift, which is why the texture read as
 * absent for so long — the CSS looked deliberate while painting nothing.
 *
 * The two directions are not symmetric and must not be tuned as if they were.
 * On a light preset the highlight is white and the base is very nearly white,
 * so the highlight contributes almost nothing and the *shadow* carries the
 * texture; on a dark preset the shadow is black on near-black, so the
 * *highlight* carries it. Both are kept on every preset because over the
 * coloured radial washes — where the base is neither near-white nor near-black
 * — the opposite thread is the one that shows.
 */
function weave(
  highlight: string,
  shadow: string,
  period: number,
  angle: number,
): string {
  return [
    `repeating-linear-gradient(${angle}deg, ${highlight} 0 1px, transparent 1px ${period}px)`,
    `repeating-linear-gradient(${angle + 90}deg, ${shadow} 0 1px, transparent 1px ${period + 2}px)`,
  ].join(", ");
}

/**
 * Built-in backgrounds, painted in CSS only — there is no bundled artwork, so
 * nothing ships that would need clearing for redistribution and the app stays
 * small.
 *
 * Every preset is built from the same parts, so they read as one family:
 *
 * - a faint woven texture on top,
 * - two or three soft radial washes that carry the colour,
 * - a diagonal base gradient underneath that keeps the corners from going
 *   flat,
 * - a `base` fill that matches what the window paints before React mounts, so
 *   there is no flash on launch.
 *
 * The first entry is the default and is also the fallback for any preset id
 * that no longer exists (see `presetById`), which is how a renamed or retired
 * preset migrates without touching anyone's config file.
 */
export const BACKGROUND_PRESETS: BackgroundPreset[] = [
  {
    id: "porcelain",
    name: "Porcelain",
    tone: "light",
    // Exactly the colour the window paints before the app mounts, so the very
    // first frame already matches the background. Referenced rather than
    // repeated: this used to be a literal that could drift from `index.html`
    // without anything noticing.
    base: THEME_BACKDROP.light,
    layers: [
      weave("rgb(255 255 255 / 0.85)", "rgb(30 41 59 / 0.16)", 9, 118),
      "radial-gradient(120% 95% at 16% 2%, #ffffff 0%, transparent 55%)",
      "radial-gradient(95% 80% at 94% 16%, #dbe4f5 0%, transparent 60%)",
      "radial-gradient(110% 90% at 74% 102%, #e2e3f7 0%, transparent 58%)",
      "radial-gradient(80% 65% at 0% 94%, #d3dce9 0%, transparent 54%)",
      "linear-gradient(158deg, #f9fbff 0%, #eef1f7 52%, #e7ebf5 100%)",
    ].join(", "),
    swatch: "linear-gradient(140deg, #ffffff, #dbe4f5 58%, #e2e3f7)",
  },
  {
    id: "linen",
    name: "Linen",
    tone: "light",
    base: "#f1ece2",
    layers: [
      weave("rgb(255 255 255 / 0.9)", "rgb(60 45 30 / 0.17)", 10, 124),
      "radial-gradient(115% 90% at 20% 0%, #ffffff 0%, transparent 58%)",
      "radial-gradient(100% 80% at 90% 20%, #e6dcc9 0%, transparent 60%)",
      "radial-gradient(105% 85% at 66% 104%, #ded6e6 0%, transparent 58%)",
      "radial-gradient(85% 70% at 2% 96%, #e9dfcd 0%, transparent 55%)",
      "linear-gradient(160deg, #f8f4ec 0%, #f1ece2 52%, #eae4de 100%)",
    ].join(", "),
    swatch: "linear-gradient(140deg, #ffffff, #e6dcc9 58%, #ded6e6)",
  },
  {
    // The only neutral in the set. Every other dark preset carries a strong
    // hue, so without this one there was nowhere to go for a quiet window.
    id: "graphite",
    name: "Graphite",
    tone: "dark",
    base: "#0d1016",
    layers: [
      weave("rgb(255 255 255 / 0.16)", "rgb(0 0 0 / 0.2)", 12, 116),
      "radial-gradient(120% 95% at 18% 0%, #2a3446 0%, transparent 56%)",
      "radial-gradient(95% 80% at 92% 14%, #1c2432 0%, transparent 60%)",
      "radial-gradient(110% 90% at 76% 102%, #171d29 0%, transparent 58%)",
      "linear-gradient(162deg, #0a0d13 0%, #12161f 55%, #0b0e15 100%)",
    ].join(", "),
    swatch: "linear-gradient(140deg, #2a3446, #141a24 60%, #0b0e15)",
  },
  {
    id: "aurora",
    name: "Aurora",
    tone: "dark",
    base: "#0a0f26",
    layers: [
      weave("rgb(255 255 255 / 0.15)", "rgb(0 0 0 / 0.2)", 13, 122),
      "radial-gradient(125% 95% at 10% 2%, #4368e8 0%, transparent 52%)",
      "radial-gradient(105% 85% at 92% 12%, #7b4bd9 0%, transparent 58%)",
      "radial-gradient(120% 95% at 78% 100%, #1f9bb5 0%, transparent 56%)",
      "radial-gradient(85% 70% at 26% 106%, #16204d 0%, transparent 58%)",
      "linear-gradient(162deg, #070b1c 0%, #0f1834 55%, #080d20 100%)",
    ].join(", "),
    swatch: "linear-gradient(140deg, #4368e8, #7b4bd9 55%, #1f9bb5)",
  },
  {
    id: "fjord",
    name: "Fjord",
    tone: "dark",
    base: "#0a1720",
    layers: [
      weave("rgb(255 255 255 / 0.15)", "rgb(0 0 0 / 0.2)", 12, 114),
      "radial-gradient(115% 88% at 14% 4%, #4fb3c9 0%, transparent 54%)",
      "radial-gradient(100% 80% at 88% 18%, #3f7fd9 0%, transparent 58%)",
      "radial-gradient(120% 90% at 62% 102%, #1d4a5c 0%, transparent 60%)",
      "radial-gradient(80% 65% at 4% 96%, #12303d 0%, transparent 56%)",
      "linear-gradient(162deg, #081318 0%, #0f2129 55%, #071016 100%)",
    ].join(", "),
    swatch: "linear-gradient(140deg, #4fb3c9, #3f7fd9 55%, #1d4a5c)",
  },
  {
    id: "rose",
    name: "Rose",
    tone: "dark",
    base: "#25102c",
    layers: [
      weave("rgb(255 255 255 / 0.16)", "rgb(0 0 0 / 0.2)", 12, 120),
      "radial-gradient(112% 82% at 84% 8%, #f2a8c8 0%, transparent 54%)",
      "radial-gradient(92% 72% at 12% 20%, #b56bd9 0%, transparent 60%)",
      "radial-gradient(120% 92% at 50% 102%, #6d3fb5 0%, transparent 58%)",
      "linear-gradient(158deg, #24102b 0%, #351449 55%, #1d0d22 100%)",
    ].join(", "),
    swatch: "linear-gradient(140deg, #f2a8c8, #b56bd9 55%, #6d3fb5)",
  },
  {
    id: "dusk",
    name: "Dusk",
    tone: "dark",
    base: "#20161f",
    layers: [
      weave("rgb(255 255 255 / 0.15)", "rgb(0 0 0 / 0.2)", 13, 118),
      "radial-gradient(112% 82% at 18% 10%, #ff9d6c 0%, transparent 52%)",
      "radial-gradient(102% 82% at 86% 20%, #d96c8f 0%, transparent 56%)",
      "radial-gradient(120% 92% at 55% 102%, #4a3d8f 0%, transparent 58%)",
      "linear-gradient(162deg, #171019 0%, #2c1c30 55%, #150f1b 100%)",
    ].join(", "),
    swatch: "linear-gradient(140deg, #ff9d6c, #d96c8f 55%, #4a3d8f)",
  },
];

/**
 * Resolves a preset id, falling back to the default (the first entry) when the
 * id is unknown. That fallback is the whole migration story for a retired
 * preset: the config keeps its old id, the UI quietly paints the default, and
 * nothing has to rewrite anyone's file.
 */
export function presetById(id: string): BackgroundPreset {
  return BACKGROUND_PRESETS.find((p) => p.id === id) ?? BACKGROUND_PRESETS[0];
}

/**
 * The preset that will actually paint, given the theme.
 *
 * `auto` is resolved here rather than at the point of painting so that the
 * picker, the background layer and the tests all agree on one answer. Anything
 * that needs to know "which swatch is in use" asks this.
 */
export function resolvePreset(id: string, isDark: boolean): BackgroundPreset {
  if (id === AUTO_PRESET) {
    return presetById(AUTO_FOR_THEME[isDark ? "dark" : "light"]);
  }
  return presetById(id);
}

/** Whether the stored preset is the theme-following one. */
export function isAuto(id: string): boolean {
  return id === AUTO_PRESET;
}

export function backgroundStyle(config: BackgroundConfig, isDark = false): CSSProperties {
  if (config.kind !== "builtin" && config.path) {
    return { background: "transparent" };
  }
  const preset = resolvePreset(config.preset, isDark);
  return { background: preset.layers, backgroundColor: preset.base };
}
