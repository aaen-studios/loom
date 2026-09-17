/**
 * Colour maths for the palette layer.
 *
 * sRGB with an HSL side-trip, and every contrast decision verified with the
 * WCAG relative-luminance formula — the standard's own definition rather than
 * an approximation of it, so the guarantee in `palette.ts` means what it says.
 *
 * OKLCH would give more perceptually even lightness steps. It is not used here
 * because the adjustment below is a *search*, not a formula: propose a colour,
 * measure it with the real formula, move until it passes. A more uniform space
 * would converge in fewer steps, not to a different answer.
 */

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

/** Hue in degrees 0..360; saturation and lightness as 0..1. */
export interface Hsl {
  h: number;
  s: number;
  l: number;
}

const HEX_SHORT = /^#([0-9a-f])([0-9a-f])([0-9a-f])$/i;
const HEX_LONG = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i;

export function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), high);
}

/** `#abc` or `#aabbcc`, or null. Deliberately hex-only: this is also the shape
 *  `<input type="color">` round-trips, so accepting more input than the UI can
 *  produce would only hide mistakes. */
export function parseHex(value: string): Rgb | null {
  const text = value.trim();
  const short = text.match(HEX_SHORT);
  if (short) {
    return {
      r: parseInt(short[1] + short[1], 16),
      g: parseInt(short[2] + short[2], 16),
      b: parseInt(short[3] + short[3], 16),
    };
  }
  const long = text.match(HEX_LONG);
  if (long) {
    return {
      r: parseInt(long[1], 16),
      g: parseInt(long[2], 16),
      b: parseInt(long[3], 16),
    };
  }
  return null;
}

function byte(value: number): number {
  return Math.round(clamp(value, 0, 255));
}

export function toHex({ r, g, b }: Rgb): string {
  const part = (value: number) => byte(value).toString(16).padStart(2, "0");
  return `#${part(r)}${part(g)}${part(b)}`;
}

/** `rgb(r g b / a)`, the modern space-separated form the stylesheet already
 *  uses throughout. Alpha is 0..1. */
export function rgba({ r, g, b }: Rgb, alpha: number): string {
  const round = (value: number) => Math.round(clamp(value, 0, 255));
  return `rgb(${round(r)} ${round(g)} ${round(b)} / ${clamp(alpha, 0, 1).toFixed(3)})`;
}

export function rgbToHsl({ r, g, b }: Rgb): Hsl {
  const red = r / 255;
  const green = g / 255;
  const blue = b / 255;
  const max = Math.max(red, green, blue);
  const min = Math.min(red, green, blue);
  const l = (max + min) / 2;
  const delta = max - min;

  if (delta === 0) return { h: 0, s: 0, l };

  const s = delta / (1 - Math.abs(2 * l - 1));
  let h: number;
  if (max === red) h = ((green - blue) / delta) % 6;
  else if (max === green) h = (blue - red) / delta + 2;
  else h = (red - green) / delta + 4;

  return { h: (h * 60 + 360) % 360, s, l };
}

export function hslToRgb({ h, s, l }: Hsl): Rgb {
  const hue = ((h % 360) + 360) % 360;
  const chroma = (1 - Math.abs(2 * l - 1)) * s;
  const second = chroma * (1 - Math.abs(((hue / 60) % 2) - 1));
  const match = l - chroma / 2;

  let triple: [number, number, number];
  if (hue < 60) triple = [chroma, second, 0];
  else if (hue < 120) triple = [second, chroma, 0];
  else if (hue < 180) triple = [0, chroma, second];
  else if (hue < 240) triple = [0, second, chroma];
  else if (hue < 300) triple = [second, 0, chroma];
  else triple = [chroma, 0, second];

  return {
    r: (triple[0] + match) * 255,
    g: (triple[1] + match) * 255,
    b: (triple[2] + match) * 255,
  };
}

/** WCAG relative luminance, 0 (black) to 1 (white). */
export function luminance({ r, g, b }: Rgb): number {
  const channel = (value: number) => {
    const c = value / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

/** WCAG contrast ratio, 1:1 to 21:1. Order does not matter. */
export function contrast(a: Rgb, b: Rgb): number {
  const first = luminance(a);
  const second = luminance(b);
  const lighter = Math.max(first, second);
  const darker = Math.min(first, second);
  return (lighter + 0.05) / (darker + 0.05);
}

/** The same colour at a different lightness, hue and saturation untouched. */
export function withLightness(colour: Rgb, lightness: number): Rgb {
  const hsl = rgbToHsl(colour);
  return hslToRgb({ ...hsl, l: clamp(lightness, 0, 1) });
}

/** Linear blend, `t` 0 gives `a` and 1 gives `b`. */
export function mix(a: Rgb, b: Rgb, t: number): Rgb {
  const amount = clamp(t, 0, 1);
  return {
    r: a.r + (b.r - a.r) * amount,
    g: a.g + (b.g - a.g) * amount,
    b: a.b + (b.b - a.b) * amount,
  };
}

/**
 * Whether a surface *looks* dark, as a question about appearance.
 *
 * Answered by luminance rather than a channel average: mid-grey (`#808080`) has
 * a luminance of 0.216, not 0.5, and anything that treats the two as
 * interchangeable will call a dark surface light.
 *
 * Deliberately **not** used to decide which way to push a colour for contrast —
 * see `adjustForContrast`, which measures instead. This answers "is this a dark
 * surface", which is what a shadow's direction and a highlight's alpha need to
 * know, and that is a question about how the surface reads rather than about
 * what is achievable on it.
 */
export function isDarkColour(colour: Rgb): boolean {
  return luminance(colour) < 0.5;
}

/**
 * The nearest colour to `colour` that reaches `min` contrast against
 * `against`, searching away from the backdrop's own lightness.
 *
 * A search, not a formula, which is why it needs no colour-space conversion to
 * be correct: each candidate is measured with the real WCAG ratio, so whatever
 * it returns satisfies the standard by construction. The invariant is that
 * `fails` never passes and `passes` always does, and each step halves the gap.
 *
 * Returns the original when it already passes, so a deliberately chosen colour
 * is left alone wherever it is legible.
 */
export function adjustForContrast(colour: Rgb, against: Rgb, min: number): Rgb {
  if (contrast(colour, against) >= min) return colour;

  const base = rgbToHsl(colour);
  const at = (lightness: number) => hslToRgb({ ...base, l: lightness });

  // Which way to move is *measured*, not assumed.
  //
  // The obvious implementation picks the direction from a luminance threshold —
  // lighten on a dark backdrop, darken on a light one — and that is wrong for
  // most of the range. Which extreme reaches further depends on the backdrop,
  // and one direction is usually strictly better: on `#7d7d7d` (luminance
  // 0.205, so "dark" by any midpoint rule) white reaches only 4.12:1 while
  // black reaches 5.10:1. A threshold would have chosen the failing direction
  // and had 5:1 available the whole time.
  //
  // Contrast rises monotonically toward whichever end is further from the
  // backdrop, so the comparison is exact rather than a heuristic.
  const lighter = at(1);
  const darker = at(0);
  const up = contrast(lighter, against);
  const down = contrast(darker, against);
  const lighterWins = up >= down;
  const best = lighterWins ? up : down;

  // A backdrop can be positioned so that neither extreme reaches the target —
  // mid-grey leaves no room at either end for a very high target. Returning the
  // best available is the honest answer, and better than spinning.
  if (best < min) return lighterWins ? lighter : darker;

  let fails = base.l;
  let passes = lighterWins ? 1 : 0;
  for (let step = 0; step < 20; step += 1) {
    const mid = (fails + passes) / 2;
    if (contrast(at(mid), against) >= min) passes = mid;
    else fails = mid;
  }
  return at(passes);
}
