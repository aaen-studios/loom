import type { CSSProperties } from "react";
import type { BackgroundConfig } from "../types";

export interface BackgroundPreset {
  id: string;
  name: string;
  /** Base fill underneath the gradient layers. */
  base: string;
  /** Layered gradients painted across the background layer. */
  layers: string;
  /** Swatch preview for the settings grid. */
  swatch: string;
}

/**
 * Built-in backgrounds. Each is pure CSS so it scales to any window size and
 * costs nothing to load. User images/videos arrive in M2.
 */
export const BACKGROUND_PRESETS: BackgroundPreset[] = [
  {
    id: "aurora",
    name: "Aurora",
    base: "#0d1330",
    layers: [
      "radial-gradient(120% 90% at 12% 8%, #3d5bd9 0%, transparent 55%)",
      "radial-gradient(100% 80% at 88% 16%, #7b4bd9 0%, transparent 60%)",
      "radial-gradient(110% 90% at 78% 92%, #1f9bb5 0%, transparent 58%)",
      "radial-gradient(90% 70% at 18% 88%, #14235c 0%, transparent 65%)",
      "linear-gradient(160deg, #0a1026 0%, #101a3d 60%, #0b1226 100%)",
    ].join(", "),
    swatch: "linear-gradient(135deg, #3d5bd9, #7b4bd9 55%, #1f9bb5)",
  },
  {
    id: "rose",
    name: "Rose",
    base: "#2a1030",
    layers: [
      "radial-gradient(110% 80% at 82% 10%, #f2a8c8 0%, transparent 55%)",
      "radial-gradient(90% 70% at 14% 22%, #b56bd9 0%, transparent 62%)",
      "radial-gradient(120% 90% at 50% 100%, #6d3fb5 0%, transparent 60%)",
      "linear-gradient(155deg, #2b1233 0%, #3c1650 55%, #241029 100%)",
    ].join(", "),
    swatch: "linear-gradient(135deg, #f2a8c8, #b56bd9 55%, #6d3fb5)",
  },
  {
    id: "meadow",
    name: "Meadow",
    base: "#eaf3e4",
    layers: [
      "radial-gradient(90% 70% at 18% 12%, #fdf6c9 0%, transparent 58%)",
      "radial-gradient(110% 80% at 84% 18%, #9fd8b4 0%, transparent 60%)",
      "radial-gradient(120% 90% at 62% 96%, #7fc3e0 0%, transparent 62%)",
      "linear-gradient(160deg, #eef7ea 0%, #e2efe6 55%, #dcebf2 100%)",
    ].join(", "),
    swatch: "linear-gradient(135deg, #fdf6c9, #9fd8b4 55%, #7fc3e0)",
  },
  {
    id: "dusk",
    name: "Dusk",
    base: "#241a2e",
    layers: [
      "radial-gradient(110% 80% at 20% 14%, #ff9d6c 0%, transparent 52%)",
      "radial-gradient(100% 80% at 84% 22%, #d96c8f 0%, transparent 58%)",
      "radial-gradient(120% 90% at 55% 100%, #4a3d8f 0%, transparent 60%)",
      "linear-gradient(160deg, #1d1526 0%, #33203a 55%, #1b1424 100%)",
    ].join(", "),
    swatch: "linear-gradient(135deg, #ff9d6c, #d96c8f 55%, #4a3d8f)",
  },
  {
    id: "fjord",
    name: "Fjord",
    base: "#0e1a20",
    layers: [
      "radial-gradient(110% 80% at 16% 10%, #4fb3c9 0%, transparent 55%)",
      "radial-gradient(100% 80% at 86% 20%, #3f7fd9 0%, transparent 58%)",
      "radial-gradient(120% 90% at 60% 100%, #1d4a5c 0%, transparent 62%)",
      "linear-gradient(160deg, #0b1519 0%, #102530 55%, #0a1216 100%)",
    ].join(", "),
    swatch: "linear-gradient(135deg, #4fb3c9, #3f7fd9 55%, #1d4a5c)",
  },
  {
    id: "linen",
    name: "Linen",
    base: "#f3efe7",
    layers: [
      "radial-gradient(90% 70% at 22% 10%, #ffffff 0%, transparent 60%)",
      "radial-gradient(110% 80% at 82% 24%, #e8ded0 0%, transparent 62%)",
      "radial-gradient(120% 90% at 58% 100%, #dad3e4 0%, transparent 60%)",
      "linear-gradient(160deg, #f6f3ec 0%, #efe9df 55%, #e9e6f0 100%)",
    ].join(", "),
    swatch: "linear-gradient(135deg, #ffffff, #e8ded0 55%, #dad3e4)",
  },
];

export function presetById(id: string): BackgroundPreset {
  return BACKGROUND_PRESETS.find((p) => p.id === id) ?? BACKGROUND_PRESETS[0];
}

export function backgroundStyle(config: BackgroundConfig): CSSProperties {
  if (config.kind !== "builtin" && config.path) {
    return { background: "transparent" };
  }
  const preset = presetById(config.preset);
  return { background: `${preset.layers}`, backgroundColor: preset.base };
}
