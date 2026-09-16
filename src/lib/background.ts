import type { CSSProperties } from "react";
import type { BackgroundConfig } from "../types";

export interface BackgroundPreset {
  id: string;
  name: string;
  /** Base fill underneath the layers. */
  base: string;
  /** Layers painted across the background area, topmost first. */
  layers: string;
  /** Swatch preview for the settings grid. */
  swatch: string;
}

/**
 * The house texture: two hairline threads crossing at right angles, the way a
 * warp crosses a weft. It is deliberately almost invisible, and it earns its
 * place for two reasons.
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
 * Alpha is kept low because the thread only has to be perceptible at the edge
 * of noticing. In dark mode the dim veil halves it again, so dark presets pass
 * roughly double the alpha of light ones.
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
    // Exactly the colour the window paints before the app mounts (App.tsx),
    // so the very first frame already matches the background.
    base: "#eef1f7",
    layers: [
      weave("rgb(255 255 255 / 0.5)", "rgb(30 41 59 / 0.022)", 11, 118),
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
    base: "#f1ece2",
    layers: [
      weave("rgb(255 255 255 / 0.55)", "rgb(60 45 30 / 0.025)", 12, 124),
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
    base: "#0d1016",
    layers: [
      weave("rgb(255 255 255 / 0.06)", "rgb(0 0 0 / 0.16)", 12, 116),
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
    base: "#0a0f26",
    layers: [
      weave("rgb(255 255 255 / 0.05)", "rgb(0 0 0 / 0.16)", 13, 122),
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
    base: "#0a1720",
    layers: [
      weave("rgb(255 255 255 / 0.05)", "rgb(0 0 0 / 0.16)", 12, 114),
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
    base: "#25102c",
    layers: [
      weave("rgb(255 255 255 / 0.055)", "rgb(0 0 0 / 0.16)", 12, 120),
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
    base: "#20161f",
    layers: [
      weave("rgb(255 255 255 / 0.055)", "rgb(0 0 0 / 0.16)", 13, 118),
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

export function backgroundStyle(config: BackgroundConfig): CSSProperties {
  if (config.kind !== "builtin" && config.path) {
    return { background: "transparent" };
  }
  const preset = presetById(config.preset);
  return { background: preset.layers, backgroundColor: preset.base };
}
