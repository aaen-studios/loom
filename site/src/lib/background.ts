/**
 * The site's background.
 *
 * The app paints its own background in CSS (a faint woven texture over soft
 * radial washes) and frosts it with `backdrop-filter`. The site does the same,
 * using the app's **default preset** rather than a re-styled imitation, so a
 * visitor sees the product's actual surface.
 *
 * ---------------------------------------------------------------------------
 * Why this is a hand-copy, and what guards it
 *
 * The app's presets live in `src/lib/background.ts` as TypeScript, because each
 * one is a data structure the settings grid also renders as a swatch. There is
 * no way to share that with a different build without either extracting the
 * step into a published package or parsing TypeScript at build time.
 *
 * So the values below are copied by hand, and `scripts/sync-site-tokens.mjs`
 * checks the one field that can be read unambiguously — the preset's `base`
 * colour — against the app's copy. If someone changes Porcelain in the app, the
 * parity check fails and this file has to follow.
 *
 * That check covers the flat fill but not the layer stack. The layers are
 * deliberately listed here verbatim from the app so a reviewer can diff them by
 * eye; treat a change to the app's preset as a prompt to update both.
 * ---------------------------------------------------------------------------
 */

export interface SiteBackgroundPreset {
  id: string;
  name: string;
  /** Base fill underneath the layers. Must match the app's `porcelain.base`. */
  base: string;
  /** Layers painted across the background area, topmost first. */
  layers: string;
}

/**
 * Two hairline threads crossing at right angles — the house texture. Nearly
 * invisible, and load-bearing: `backdrop-filter` can only frost what is
 * actually behind it, so a perfectly flat background blurs to a perfectly flat
 * result and the glass panels lose their depth entirely.
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
 * Porcelain: the app's default background, and the site's.
 *
 * Its `base` is exactly the colour `<html>` is painted with before anything
 * renders (see `THEME_BOOT` in `layout.tsx`), which is also the colour the app
 * uses for its pre-mount backdrop. That is why there is no flash on load: the
 * first frame already matches.
 */
export const PORCELAIN: SiteBackgroundPreset = {
  id: "porcelain",
  name: "Porcelain",
  base: "#eef1f7",
  layers: [
    weave("rgb(255 255 255 / 0.5)", "rgb(30 41 59 / 0.022)", 11, 118),
    "radial-gradient(120% 95% at 16% 2%, #ffffff 0%, transparent 55%)",
    "radial-gradient(95% 80% at 94% 16%, #dbe4f5 0%, transparent 60%)",
    "radial-gradient(110% 90% at 74% 102%, #e2e3f7 0%, transparent 58%)",
    "radial-gradient(80% 65% at 0% 94%, #d3dce9 0%, transparent 54%)",
    "linear-gradient(158deg, #f9fbff 0%, #eef1f7 52%, #e7ebf5 100%)",
  ].join(", "),
};

/**
 * Dark mode keeps a floor on the veil, exactly as the app does
 * (`DARK_DIM_FLOOR` in the app's `Background.tsx`).
 *
 * The app's dark palette uses near-white ink, which needs a bright background
 * held down. Porcelain is a light preset, so in dark mode it is veiled to a cool
 * grey rather than left glaring. This is deliberately the app's behaviour, not
 * a site-specific choice: a visitor toggling dark should see what the product
 * actually does.
 */
export const DARK_DIM_FLOOR = 48;
