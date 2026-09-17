import {
  adjustForContrast,
  clamp,
  contrast,
  hslToRgb,
  isDarkColour,
  luminance,
  parseHex,
  rgbToHsl,
  rgba,
  toHex,
  withLightness,
  type Hsl,
  type Rgb,
} from "./colour";

/**
 * The three colours a palette is built from, and the tokens derived from them.
 *
 * Three inputs, not twenty. Every other colour the app uses is a derivative of
 * one of these, so a custom or adaptive theme cannot end up half-applied — the
 * `--ink-faint` that a hand-written theme would forget is computed here, at the
 * same time as `--ink`, from the same value.
 */
export interface Palette {
  accent: Rgb;
  ink: Rgb;
  surface: Rgb;
}

/** The same three, as hex, which is what config and `<input type="color">` use. */
export interface PaletteHex {
  accent: string;
  ink: string;
  surface: string;
}

/** How the palette is chosen. `default` means the stylesheet's own tokens. */
export type PaletteMode = "default" | "custom" | "adaptive";

/**
 * A palette as something other than this module hands it over: three hex
 * strings, and optionally the mode.
 *
 * `mode` is optional so a caller holding only the colours — a test, or a
 * component that has already branched on the mode — does not have to invent one
 * to reach `fromPaletteHex`.
 */
export interface PaletteLike extends Partial<PaletteHex> {
  mode?: PaletteMode;
}

/** A colour present in an image, and how much of it there is. */
export interface Swatch {
  colour: Rgb;
  /** Share of the sampled pixels, 0..1. Weights across a set sum to ~1. */
  weight: number;
}

/* ---------------------------------------------------------------------------
   Contrast targets

   Named because they are the *contract* of this module: whatever a theme
   derives, these are the ratios it must reach, and `palette.test.ts` asserts
   them for every mode rather than trusting the maths.
--------------------------------------------------------------------------- */

/** Body text against its surface. WCAG AA. */
export const MIN_INK_CONTRAST = 4.5;

/**
 * Accent against its surface. WCAG AA for non-text (icons, borders, focus
 * rings) and for large text, which is what an accent mostly draws.
 */
export const MIN_ACCENT_CONTRAST = 3;

/** Aimed higher than the minimum, because `--ink-faint` is this at 64% alpha
 *  and the alpha reduces the contrast again. */
const INK_TARGET = 7;

/** Error text has to be as readable as body text: it is the one message a user
 *  cannot afford to squint at. */
const DANGER_TARGET = 4.5;

/** The red errors use, as a hue and saturation. Only its lightness is derived. */
const DANGER: Hsl = { h: 358, s: 0.72, l: 0.5 };

/* ---------------------------------------------------------------------------
   Quantisation
--------------------------------------------------------------------------- */

/** Buckets per channel in the histogram. 5 bits keeps near-neutral surfaces
 *  distinguishable; the bucket *mean* is used rather than its centre, so the
 *  result is not quantised to this grid. */
const BITS = 5;
const LEVELS = 1 << BITS;
const SHIFT = 8 - BITS;

/**
 * Reduces sampled pixels to a handful of representative colours.
 *
 * Median cut: start with one box holding every colour, then repeatedly split
 * the box that is worth splitting most, until there are `count` boxes. Each
 * box becomes the weighted mean of the pixels inside it, which is why the
 * output is not limited to the histogram's resolution.
 *
 * The split score is population times colour range. Population alone would
 * divide a large flat sky into near-identical blues while a small vivid detail
 * stayed coarse; range alone would chase a single stray pixel. Together they
 * spend the budget where there is both *a lot* and *a variety*.
 *
 * Alpha below 8 is skipped: a transparent pixel is not part of the picture.
 */
export function quantise(pixels: ArrayLike<number>, count = 6): Swatch[] {
  interface Bucket {
    r: number;
    g: number;
    b: number;
    n: number;
  }

  const buckets = new Map<number, Bucket>();
  let total = 0;

  for (let i = 0; i + 3 < pixels.length; i += 4) {
    if (pixels[i + 3] < 8) continue;
    const r = pixels[i];
    const g = pixels[i + 1];
    const b = pixels[i + 2];
    const key =
      ((r >> SHIFT) * LEVELS + (g >> SHIFT)) * LEVELS + (b >> SHIFT);
    const existing = buckets.get(key);
    if (existing) {
      existing.r += r;
      existing.g += g;
      existing.b += b;
      existing.n += 1;
    } else {
      buckets.set(key, { r, g, b, n: 1 });
    }
    total += 1;
  }

  if (total === 0) return [];

  /** A bucket's mean colour, which is the colour actually represented. */
  const meanOf = (bucket: Bucket): Rgb => ({
    r: bucket.r / bucket.n,
    g: bucket.g / bucket.n,
    b: bucket.b / bucket.n,
  });

  interface Box {
    buckets: Bucket[];
    population: number;
    range: number;
    axis: 0 | 1 | 2;
  }

  const summarise = (buckets: Bucket[]): Box => {
    let population = 0;
    const low = [255, 255, 255];
    const high = [0, 0, 0];
    for (const bucket of buckets) {
      population += bucket.n;
      const mean = meanOf(bucket);
      const parts = [mean.r, mean.g, mean.b];
      for (let axis = 0; axis < 3; axis += 1) {
        low[axis] = Math.min(low[axis], parts[axis]);
        high[axis] = Math.max(high[axis], parts[axis]);
      }
    }
    const spans = [
      high[0] - low[0],
      high[1] - low[1],
      high[2] - low[2],
    ] as [number, number, number];
    const axis = spans.indexOf(Math.max(...spans)) as 0 | 1 | 2;
    return { buckets, population, range: spans[axis], axis };
  };

  let boxes: Box[] = [summarise([...buckets.values()])];

  while (boxes.length < count) {
    // The box worth splitting: lots of pixels, spread across a wide range.
    let best = -1;
    let bestScore = 0;
    for (let index = 0; index < boxes.length; index += 1) {
      const box = boxes[index];
      if (box.buckets.length < 2) continue;
      const score = box.population * (1 + box.range / 255);
      if (score > bestScore) {
        bestScore = score;
        best = index;
      }
    }
    // Every remaining box is a single colour, so there is nothing to divide.
    if (best === -1) break;

    const box = boxes[best];
    const partOf = (bucket: Bucket): number => {
      const mean = meanOf(bucket);
      return box.axis === 0 ? mean.r : box.axis === 1 ? mean.g : mean.b;
    };
    const sorted = [...box.buckets].sort((a, b) => partOf(a) - partOf(b));

    // Split at the population-weighted median rather than the midpoint, so both
    // halves carry a comparable share of the picture.
    const half = box.population / 2;
    let running = 0;
    let split = 1;
    for (let index = 0; index < sorted.length - 1; index += 1) {
      running += sorted[index].n;
      if (running >= half) {
        split = index + 1;
        break;
      }
      split = index + 2;
    }

    const left = sorted.slice(0, split);
    const right = sorted.slice(split);
    if (left.length === 0 || right.length === 0) break;

    boxes = [
      ...boxes.slice(0, best),
      summarise(left),
      summarise(right),
      ...boxes.slice(best + 1),
    ];
  }

  return boxes
    .map((box) => {
      let r = 0;
      let g = 0;
      let b = 0;
      let weight = 0;
      for (const bucket of box.buckets) {
        const mean = meanOf(bucket);
        r += mean.r * bucket.n;
        g += mean.g * bucket.n;
        b += mean.b * bucket.n;
        weight += bucket.n;
      }
      return {
        colour: { r: r / weight, g: g / weight, b: b / weight },
        weight: box.population / total,
      };
    })
    .sort((a, b) => b.weight - a.weight);
}

/* ---------------------------------------------------------------------------
   Derivation
--------------------------------------------------------------------------- */

/** The colour the picture is mostly made of. */
/**
 * A stable fingerprint of a set of swatches.
 *
 * Used as the memo key for an adaptive palette. The obvious key — how many
 * swatches there are, plus the dominant colour — is not enough: two different
 * pictures routinely share a length and a red channel, and the palette would
 * then keep the previous background's colours. Every colour and every weight
 * goes in, rounded to three decimals so that float noise from a re-quantise of
 * the *same* image does not read as a change.
 */
export function swatchSignature(swatches: Swatch[]): string {
  if (swatches.length === 0) return "none";
  return swatches
    .map((swatch) => `${toHex(swatch.colour)}@${swatch.weight.toFixed(3)}`)
    .join(",");
}

/** A hex colour opening a gradient stop, with an optional `%` position. */
const GRADIENT_STOP =
  /#([0-9a-fA-F]{3}(?:[0-9a-fA-F]{3})?)(?![\da-fA-F])\s*(?:(-?\d+(?:\.\d+)?)%)?/g;

/**
 * Fills in the stops CSS left unpositioned.
 *
 * A gradient is allowed to name a position for some stops and not others; the
 * spec distributes the unpositioned ones evenly between their nearest
 * positioned neighbours. The first and last default to 0% and 100%.
 */
function resolveStopPositions(positions: (number | null)[]): number[] {
  const resolved = [...positions];
  const last = resolved.length - 1;
  if (resolved[0] === null) resolved[0] = 0;
  if (resolved[last] === null) resolved[last] = 100;

  let i = 0;
  while (i < resolved.length) {
    if (resolved[i] !== null) {
      i += 1;
      continue;
    }
    let end = i;
    while (resolved[end] === null) end += 1;
    // Both ends are known: `i` cannot be 0 and `end` cannot run past the last
    // stop, because those two were resolved above.
    const before = resolved[i - 1] as number;
    const after = resolved[end] as number;
    const gaps = end - i + 1;
    for (let k = i; k < end; k += 1) {
      resolved[k] = before + ((after - before) * (k - i + 1)) / gaps;
    }
    i = end;
  }
  return resolved as number[];
}

/**
 * How much of a gradient each stop actually covers.
 *
 * A stop owns the span from the midpoint of its left gap to the midpoint of its
 * right gap, which is where a gradient's colour visibly changes. That is what
 * makes the *middle* stop of `#ffffff, #dbe4f5 58%, #e2e3f7` the dominant one
 * rather than the first, and it sums to the full length.
 */
function midpointSpans(positions: number[]): number[] {
  const last = positions.length - 1;
  return positions.map((position, index) => {
    const left = index === 0 ? 0 : (position - positions[index - 1]) / 2;
    const right = index === last ? 0 : (positions[index + 1] - position) / 2;
    return Math.max(0, left + right);
  });
}

/**
 * Weighted swatches from a CSS gradient.
 *
 * Built-in backgrounds paint in CSS rather than from a file, so there is
 * nothing to decode — but every preset already carries a `swatch` gradient of
 * its literal colours. Deriving swatches from that is what lets Adaptive do
 * something on the presets, including the woven Porcelain the app opens with,
 * instead of silently falling back to the default accent.
 *
 * Hex only, deliberately: the caller is a gradient this repository writes, and
 * a half-understood `color-mix()` would be worse than no swatch.
 */
export function swatchesFromGradient(css: string): Swatch[] {
  const stops: { colour: Rgb; position: number | null }[] = [];
  for (const match of css.matchAll(GRADIENT_STOP)) {
    const colour = parseHex(`#${match[1]}`);
    if (!colour) continue;
    stops.push({
      colour,
      position: match[2] === undefined ? null : clamp(Number(match[2]), 0, 100),
    });
  }
  if (stops.length === 0) return [];

  const spans = midpointSpans(resolveStopPositions(stops.map((stop) => stop.position)));
  const total = spans.reduce((sum, span) => sum + span, 0);
  const shares =
    total > 0 ? spans.map((span) => span / total) : spans.map(() => 1 / spans.length);

  // The same colour twice in one gradient is one colour, not two.
  const merged = new Map<string, Swatch>();
  stops.forEach((stop, index) => {
    const key = toHex(stop.colour);
    const existing = merged.get(key);
    if (existing) existing.weight += shares[index];
    else merged.set(key, { colour: stop.colour, weight: shares[index] });
  });

  // Heaviest first, with hex as the tie-break so the order — and therefore
  // `swatchSignature` — is the same on every run.
  return [...merged.values()].sort(
    (a, b) => b.weight - a.weight || toHex(a.colour).localeCompare(toHex(b.colour)),
  );
}

export function dominant(swatches: Swatch[]): Swatch | null {
  let best: Swatch | null = null;
  for (const swatch of swatches) {
    if (!best || swatch.weight > best.weight) best = swatch;
  }
  return best;
}

/**
 * The surface: the picture's own hue, held to a usable lightness.
 *
 * Nearly neutral on purpose. The panels are glass laid over the photograph, so
 * a strongly saturated surface would fight the thing it is meant to sit on —
 * and the built-in presets make the same choice, at `#0d1016` and `#eef1f7`.
 * What the image contributes is its *hue*, which is what makes an adaptive
 * theme feel derived from the picture rather than merely tinted by it.
 */
export function deriveSurface(swatches: Swatch[], isDark: boolean): Rgb {
  const anchor = dominant(swatches);
  const hue = anchor ? rgbToHsl(anchor.colour).h : 220;
  const saturation = anchor ? clamp(rgbToHsl(anchor.colour).s, 0, 1) : 0;
  return hslToRgb({
    h: hue,
    s: Math.min(saturation, isDark ? 0.16 : 0.34),
    l: isDark ? 0.09 : 0.96,
  });
}

/**
 * The accent: the most *coloured* thing in the picture, made usable.
 *
 * Scored by saturation times presence, because an accent has to be vivid and
 * has to be actually in the image — a lone neon pixel is not a theme.
 *
 * Saturation is then lifted into a workable band. A photograph's colours are
 * mostly muted, and a muted accent is indistinguishable from a grey one, which
 * is the failure this step exists to prevent. Lightness is left to
 * `adjustForContrast`, which has the actual ratio to work with.
 *
 * Falls back to the base theme's own accent when the picture has no colour at
 * all — a black-and-white photograph should give a default accent, not a
 * washed-out grey one pretending to be a choice.
 */
export function deriveAccent(
  swatches: Swatch[],
  surface: Rgb,
  fallback: Rgb,
): Rgb {
  let best: Swatch | null = null;
  let bestScore = 0;
  for (const swatch of swatches) {
    const { s } = rgbToHsl(swatch.colour);
    const score = swatch.weight * (0.15 + s);
    if (score > bestScore) {
      bestScore = score;
      best = swatch;
    }
  }

  // Below this the picture is effectively monochrome.
  if (!best || rgbToHsl(best.colour).s < 0.08) {
    return adjustForContrast(fallback, surface, MIN_ACCENT_CONTRAST);
  }

  const hsl = rgbToHsl(best.colour);
  const rough = hslToRgb({
    h: hsl.h,
    s: clamp(hsl.s * 1.4, 0.45, 0.8),
    // A mid starting lightness, so the contrast search moves a short distance
    // in whichever direction the surface requires — which is why this function
    // needs no idea of the theme: the surface decides, and it is passed in.
    l: clamp(hsl.l, 0.42, 0.6),
  });
  return adjustForContrast(rough, surface, MIN_ACCENT_CONTRAST);
}

/**
 * The ink: **not** taken from the picture.
 *
 * This is the line that keeps an adaptive theme readable. A photograph can be
 * any colour in any arrangement, so deriving text from it would hand every
 * contrast decision to chance — a pale sky becomes pale text on a pale panel.
 * Instead the ink is the principled near-white or near-black, carrying only a
 * hint of the image's hue so the theme still feels of a piece.
 *
 * The hue hint is deliberately small (12% saturation). Beyond that it stops
 * being a tint and starts being a colour, and the eye reads body text as
 * coloured rather than as text.
 */
export function deriveInk(surface: Rgb, hue: number, isDark: boolean): Rgb {
  const tinted = hslToRgb({ h: hue, s: 0.12, l: isDark ? 0.96 : 0.08 });
  return adjustForContrast(tinted, surface, INK_TARGET);
}

/** The three colours an image resolves to, for one base theme. */
export function deriveAdaptive(
  swatches: Swatch[],
  isDark: boolean,
  fallbackAccent: Rgb,
): Palette {
  const surface = deriveSurface(swatches, isDark);
  const anchor = dominant(swatches);
  return {
    surface,
    accent: deriveAccent(swatches, surface, fallbackAccent),
    ink: deriveInk(surface, anchor ? rgbToHsl(anchor.colour).h : 220, isDark),
  };
}

/**
 * Brings a hand-picked palette inside the contrast contract.
 *
 * A custom theme is the user's to design, but it is not theirs to make
 * unreadable: text they cannot see is a bug in the app, not a preference. So
 * the same targets apply, and a colour that already passes is returned
 * untouched — which is most of them.
 *
 * The surface is the reference and is never adjusted. Moving it would change
 * what every other colour is measured against, so a user's chosen panel colour
 * is the one thing that stays exactly as picked.
 */
export function enforceContrast(palette: Palette): Palette {
  return {
    surface: palette.surface,
    ink: adjustForContrast(palette.ink, palette.surface, MIN_INK_CONTRAST),
    accent: adjustForContrast(
      palette.accent,
      palette.surface,
      MIN_ACCENT_CONTRAST,
    ),
  };
}

/** Whether enforcement changed anything, so the UI can say so. */
export function wasAdjusted(before: Palette, after: Palette): boolean {
  return (
    toHex(before.ink) !== toHex(after.ink) ||
    toHex(before.accent) !== toHex(after.accent)
  );
}

/* ---------------------------------------------------------------------------
   Tokens
--------------------------------------------------------------------------- */

/**
 * Every colour token a theme owns, derived from the three inputs.
 *
 * Full coverage rather than a partial override: leaving some tokens to the base
 * theme would mean a custom surface with the default theme's borders and hover
 * washes, which look wrong in a way nobody can name. Anything derived is
 * derived here, at the same time and from the same values.
 *
 * Deliberately *not* included: `--ansi-*` and `--terminal-*`. A terminal has
 * its own palette on purpose, and it reads `--ink` for its foreground, so it
 * picks up a custom theme's text colour without having its ANSI colours
 * scrambled by whatever photograph is on screen.
 */
export function tokensFor(palette: Palette): Record<string, string> {
  const { accent, ink, surface } = palette;
  const { h, s } = rgbToHsl(surface);
  // Direction follows the *surface*, not the theme's label. A user may pick a
  // light panel while the app is in dark mode, and everything that sits on the
  // panel has to answer to the panel.
  const light = !isDarkColour(surface);

  const danger = adjustForContrast(
    hslToRgb(DANGER),
    surface,
    DANGER_TARGET,
  );

  return {
    "--ink": toHex(ink),
    "--ink-soft": rgba(ink, 0.82),
    "--ink-faint": rgba(ink, 0.64),
    "--ink-ghost": rgba(ink, 0.09),

    "--accent": toHex(accent),
    "--accent-soft": rgba(accent, 0.14),

    "--panel-bg": rgba(surface, light ? 0.88 : 0.66),
    "--panel-bg-strong": rgba(surface, light ? 0.95 : 0.82),
    "--pill-bg": rgba(surface, light ? 0.9 : 0.72),

    // Washes are ink at low alpha, which inverts correctly by construction:
    // near-white washes on a dark surface, near-black on a light one.
    "--hover-bg": rgba(ink, 0.06),
    "--card-bg": rgba(ink, 0.045),
    "--active-bg": rgba(ink, 0.09),

    "--glass-border": rgba(ink, 0.1),
    "--glass-border-strong": rgba(ink, 0.16),

    // The filled-button pair is ink and surface exchanged, so a custom theme
    // gets the inversion for free and it can never fail to contrast.
    "--control-bg": rgba(ink, 0.92),
    "--control-bg-hover": rgba(ink, 0.78),
    "--control-ink": toHex(surface),

    "--danger": toHex(danger),
    "--danger-soft": rgba(danger, 0.12),

    "--glass-highlight": `inset 0 1px 0 rgb(255 255 255 / ${
      light ? 0.9 : 0.05
    })`,
    "--panel-shadow": `0 30px 70px -34px ${rgba(ink, light ? 0.5 : 0.6)}`,

    // The quick-ask overlay's thread, which is the accent in all but name.
    "--thread": toHex(accent),
    "--thread-soft": rgba(accent, 0.16),
    "--thread-line": rgba(accent, 0.38),
    "--thread-glow": rgba(accent, 0.38),

    // The window backdrop, used behind the glass and while the first frame
    // paints. In light mode it is the surface brought to full opacity; in dark
    // mode it is a deeper version, because the veil darkens it again anyway.
    "--app-backdrop": light
      ? toHex(surface)
      : toHex(withLightness(surface, Math.max(0, luminance(surface) > 0 ? 0.03 : 0.04))),
    "--surface-hue": String(Math.round(h)),
    "--surface-saturation": `${Math.round(s * 100)}%`,
  };
}

/** Every token `tokensFor` can set, so the applier can clear them again. */
export const PALETTE_TOKENS = Object.keys(
  // A throwaway palette, only for its key set — the values are irrelevant and
  // are never applied. Cheaper and more honest than a hand-kept list that would
  // drift from the function above. The zero surface is light by luminance, so
  // this exercises the light branch; the key set is identical either way.
  tokensFor({
    accent: { r: 0, g: 0, b: 0 },
    ink: { r: 0, g: 0, b: 0 },
    surface: { r: 0, g: 0, b: 0 },
  }),
);

/** Hex form, for config and the colour inputs. */
export function toPaletteHex(palette: Palette): PaletteHex {
  return {
    accent: toHex(palette.accent),
    ink: toHex(palette.ink),
    surface: toHex(palette.surface),
  };
}

/** The inverse, falling back per channel rather than failing wholesale. */
export function fromPaletteHex(
  hex: PaletteLike,
  fallback: Palette,
): Palette {
  // `||` rather than `&&` on the guard: `hex.accent && parseHex(...)` yields the
  // empty string when the property is absent, which is not an `Rgb` and slips
  // past a `??` — an empty string is not nullish.
  const one = (value: string | undefined, or: Rgb): Rgb =>
    (value ? parseHex(value) : null) ?? or;
  return {
    accent: one(hex.accent, fallback.accent),
    ink: one(hex.ink, fallback.ink),
    surface: one(hex.surface, fallback.surface),
  };
}

/** Whether a chosen palette's text is actually readable on its surface. */
export function isReadable(palette: Palette): boolean {
  return (
    contrast(palette.ink, palette.surface) >= MIN_INK_CONTRAST &&
    contrast(palette.accent, palette.surface) >= MIN_ACCENT_CONTRAST
  );
}

/**
 * The seeded palette for a base theme: the colours the stylesheet already
 * ships, as `Rgb`.
 *
 * These live here rather than in `applyPalette.ts` because they are the
 * *values* of the two built-in themes, which is a fact about the palette layer
 * rather than about installing one. `applyPalette` imports them.
 */
export const BASE_PALETTE: Record<"light" | "dark", Palette> = {
  light: {
    accent: { r: 79, g: 91, b: 213 },
    ink: { r: 10, g: 12, b: 22 },
    surface: { r: 238, g: 241, b: 247 },
  },
  dark: {
    accent: { r: 142, g: 162, b: 255 },
    ink: { r: 242, g: 245, b: 250 },
    surface: { r: 18, g: 21, b: 31 },
  },
};

/** The built-in accent per theme, for an adaptive palette with no usable colour. */
export const BASE_ACCENT: Record<"light" | "dark", Rgb> = {
  light: BASE_PALETTE.light.accent,
  dark: BASE_PALETTE.dark.accent,
};
