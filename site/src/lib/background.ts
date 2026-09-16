/**
 * The site's backdrop.
 *
 * The app paints its background entirely in CSS — a faint woven texture over a
 * few soft radial washes — and then frosts it with `backdrop-filter`. This does
 * the same, using the app's *default* preset rather than a lookalike, so a
 * visitor is looking at the surface the product actually ships.
 *
 * ---------------------------------------------------------------------------
 * Why the values are copied by hand, and what keeps them honest
 * ---------------------------------------------------------------------------
 *
 * The app's presets live in `src/lib/background.ts` as TypeScript, because each
 * one is also a data structure the settings grid renders as a swatch. There is
 * no way to hand that to a separate build without publishing a package or
 * parsing TypeScript at build time — both of which cost more than they return
 * for one preset.
 *
 * So the value is copied, and `background.test.ts` reads the app's source and
 * asserts the `base` colour still matches. If Porcelain is re-tinted in the app
 * this file fails to build rather than rendering a background the product never
 * had.
 *
 * The check covers the flat fill only. The layer stack is listed here so a
 * reviewer can diff it by eye; a change to the app's preset is a prompt to
 * update both, and the test's name says so.
 */

export interface SiteBackground {
  id: string;
  name: string;
  /** The fill underneath everything. Must match the app's `porcelain.base`. */
  base: string;
  /** The layer stack, topmost first. */
  layers: string;
}

/**
 * The house texture: two hairline threads crossing at right angles.
 *
 * Nearly invisible, and load-bearing. `backdrop-filter` can only frost what is
 * actually behind it, so a perfectly flat backdrop blurs to a perfectly flat
 * result and every glass surface above it loses its depth. The two directions
 * use periods two pixels apart on purpose — an exact grid reads as graph paper,
 * an incommensurate one reads as cloth.
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

export const PORCELAIN: SiteBackground = {
  id: "porcelain",
  name: "Porcelain",
  // Identical to the colour the app paints into the window before React mounts,
  // and to `--site-light` in `globals.css`. That is why there is no flash: the
  // first frame is already the right colour.
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
 * How hard to veil the backdrop in dark mode, 0–100.
 *
 * The app's dark palette is near-white ink, which needs a bright backdrop held
 * down or text stops being readable over it. Porcelain is a *light* preset, so
 * in dark mode it is dimmed to a cool grey rather than left glaring. This is the
 * app's own floor, not a site-specific decision — a visitor who switches to dark
 * should see what the product would actually do, including this.
 */
export const DARK_DIM_FLOOR = 48;

/**
 * Film grain, as an inline SVG turbulence filter.
 *
 * Large fields of a very subtle gradient band visibly on some displays; a
 * whisper of noise breaks the steps. The app uses the same technique at the same
 * 4% opacity, so the two surfaces have the same tooth.
 */
export const GRAIN =
  "url(\"data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='180' height='180'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='2' stitchTiles='stitch'/></filter><rect width='100%25' height='100%25' filter='url(%23n)' opacity='0.55'/></svg>\")";
