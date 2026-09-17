import type { ITheme } from "@xterm/xterm";

/**
 * The terminal's palette, and the one rule that keeps it working.
 *
 * **xterm accepts hex only.** Not a style preference — it is what the parser
 * takes. From the shipped bundle:
 *
 *     if (value.match(/#[\da-f]{3,8}/i)) switch (value.length) { 4: 5: 7: 9: }
 *     const fn = value.match(/rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3}).../);
 *     if (fillRect(...), [r,g,b,a] = getImageData(...), a !== 255)
 *       throw new Error("css.toColor: Unsupported css format");
 *
 * Two consequences, both of which bit this app:
 *
 * 1. The functional form is **comma-separated only**. The modern
 *    `rgb(8 10 16 / 0.86)` does not match, falls through to a canvas probe, and
 *    a canvas probe of a *translucent* colour throws on the alpha check.
 * 2. Every caller upstream swallows that throw and silently falls back to the
 *    default — so an unparseable background is not an error you ever see, it is
 *    a terminal that is quietly black. The foreground kept parsing, which is why
 *    the symptom was dark ink on a black void rather than a missing colour.
 *
 * So this module normalises: whatever the stylesheet says, xterm is handed
 * `#rrggbb` or `#rrggbbaa` and nothing else. `toXtermColor` returning null is a
 * programming error the tests catch, not a runtime condition to paper over —
 * except at the edges, where a fallback is better than a crash.
 *
 * `var()` is resolved here rather than left to `getComputedStyle`, because
 * whether a browser substitutes a `var()` inside a custom property before
 * returning it is not something to bet the palette on.
 */

export interface ColorLookup {
  (name: string): string | undefined;
}

/** How deep a `var(--a)` → `var(--b)` → … chain may go before giving up. */
const MAX_DEPTH = 8;

const VAR_REFERENCE = /^var\(\s*(--[A-Za-z0-9_-]+)\s*\)$/;
const HEX = /^#([0-9a-f]{3,8})$/i;
const FUNCTIONAL = /^rgba?\(\s*([^)]+)\)$/i;

/** One colour channel: `255`, `127.5`, or a `%` of 255. */
function parseChannel(part: string): number | null {
  const text = part.trim();
  if (text === "") return null;
  if (text.endsWith("%")) {
    const percent = Number(text.slice(0, -1));
    if (!Number.isFinite(percent)) return null;
    return Math.max(0, Math.min(255, Math.round((percent / 100) * 255)));
  }
  const value = Number(text);
  if (!Number.isFinite(value)) return null;
  return Math.max(0, Math.min(255, Math.round(value)));
}

/** The alpha channel: absent means opaque, `0.5` and `50%` both mean half. */
function parseAlpha(part: string | undefined): number | null {
  if (part === undefined || part.trim() === "") return 255;
  const text = part.trim();
  if (text.endsWith("%")) {
    const percent = Number(text.slice(0, -1));
    if (!Number.isFinite(percent)) return null;
    return Math.max(0, Math.min(255, Math.round((percent / 100) * 255)));
  }
  const value = Number(text);
  if (!Number.isFinite(value)) return null;
  return Math.max(0, Math.min(255, Math.round(value * 255)));
}

function toHex(bytes: number[]): string {
  return `#${bytes.map((byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

/**
 * `#rgb`, `#rgba`, `#rrggbb` and `#rrggbbaa` all normalised to eight digits.
 *
 * Two steps, and both are needed: doubling turns the shorthand into long form
 * (`#abc` → `#aabbcc`, `#abcd` → `#aabbccdd`), then padding fills in the alpha
 * that the three- and six-digit forms leave implied. Doubling alone is not
 * enough for `#abc` — it lands on six digits, which is the one case that still
 * needs an alpha appended.
 *
 * Anything else returns null. `#12345` is not a colour CSS defines, so guessing
 * at a length with no meaning would be worse than refusing it.
 */
function expand(hexDigits: string): string | null {
  const digits = hexDigits.toLowerCase();
  if (digits.length === 3 || digits.length === 4) {
    const doubled = digits.replace(/[0-9a-f]/g, (char) => char + char);
    // `#abcd` already carries its alpha; `#abc` does not.
    return doubled.length === 6 ? `#${doubled}ff` : `#${doubled}`;
  }
  if (digits.length === 6) return `#${digits}ff`;
  if (digits.length === 8) return `#${digits}`;
  return null;
}

/**
 * A colour in a form xterm's parser accepts — always `#rrggbbaa`.
 *
 * Eight digits for every input, opaque ones padded with `ff`. xterm takes both
 * `#rrggbb` and `#rrggbbaa`, so the padding is not required for it; it is so
 * that one shape leaves this module and a caller never has to branch on which
 * it got. `xtermTheme.test.ts` asserts the single shape.
 *
 * Returns null for anything else — a named colour, a gradient, a `calc()` — so
 * the caller can use a known-good fallback instead of handing xterm something
 * it will discard in silence.
 */
export function toXtermColor(value: string | undefined | null): string | null {
  if (!value) return null;
  const text = value.trim();
  if (text === "") return null;

  if (HEX.test(text)) return expand(text.slice(1));

  const fn = text.match(FUNCTIONAL);
  if (fn) {
    // Semicolons cannot appear in a colour, spaces and commas both separate,
    // and a slash introduces the alpha. Splitting on all of them handles
    // `rgb(1,2,3)`, `rgb(1 2 3)`, `rgb(1 2 3 / .5)` and `rgba(1,2,3,.5)`.
    const parts = fn[1]
      .split(/[,/\s]+/)
      .map((part) => part.trim())
      .filter((part) => part !== "");
    if (parts.length < 3) return null;
    const channels = [parseChannel(parts[0]), parseChannel(parts[1]), parseChannel(parts[2])];
    if (channels.some((channel) => channel === null)) return null;
    const alpha = parseAlpha(parts[3]);
    if (alpha === null) return null;
    return toHex([...(channels as number[]), alpha]);
  }

  return null;
}

/**
 * Resolves a custom property to a concrete colour, following `var()` references.
 *
 * `lookup` is the raw value of a custom property as the stylesheet declares it —
 * `var(--accent)`, `#8ea2ff`, `rgb(1 2 3 / 0.5)` — not its computed value.
 */
export function resolveColor(
  raw: string | undefined,
  lookup: ColorLookup,
  depth = 0,
): string | null {
  if (!raw || depth > MAX_DEPTH) return null;
  const text = raw.trim();
  const reference = text.match(VAR_REFERENCE);
  if (reference) return resolveColor(lookup(reference[1]), lookup, depth + 1);
  return toXtermColor(text);
}

/** A resolved colour, or the fallback. Never null, never unparseable. */
export function colorOr(raw: string | undefined, lookup: ColorLookup, fallback: string): string {
  return resolveColor(raw, lookup) ?? fallback;
}

/**
 * Which custom property feeds which xterm theme slot.
 *
 * Data rather than a block of assignments so the tests can walk the same list —
 * an ANSI slot added to the stylesheet but forgotten here is a colour that
 * silently keeps xterm's stock value.
 */
export const ANSI_SLOTS: [keyof ITheme, string][] = [
  ["black", "--ansi-black"],
  ["red", "--ansi-red"],
  ["green", "--ansi-green"],
  ["yellow", "--ansi-yellow"],
  ["blue", "--ansi-blue"],
  ["magenta", "--ansi-magenta"],
  ["cyan", "--ansi-cyan"],
  ["white", "--ansi-white"],
  ["brightBlack", "--ansi-bright-black"],
  ["brightRed", "--ansi-bright-red"],
  ["brightGreen", "--ansi-bright-green"],
  ["brightYellow", "--ansi-bright-yellow"],
  ["brightBlue", "--ansi-bright-blue"],
  ["brightMagenta", "--ansi-bright-magenta"],
  ["brightCyan", "--ansi-bright-cyan"],
  ["brightWhite", "--ansi-bright-white"],
];

/**
 * The dark palette, as fallbacks.
 *
 * Used only when a custom property is missing — a stylesheet that has not
 * applied yet, or a name someone renamed without updating the list above. These
 * are deliberately *not* xterm's defaults: falling back to pure-hue reds would
 * reintroduce the mismatch the tokens exist to avoid.
 */
export const FALLBACK_ANSI: Record<string, string> = {
  black: "#12151f",
  red: "#ff7a85",
  green: "#7fd88f",
  yellow: "#f0c674",
  blue: "#8ea2ff",
  magenta: "#c9a2ff",
  cyan: "#7fd4e0",
  white: "#e6e9f5",
  brightBlack: "#5d6478",
  brightRed: "#ff9aa2",
  brightGreen: "#a3e8b0",
  brightYellow: "#ffd98a",
  brightBlue: "#aebcff",
  brightMagenta: "#dcbcff",
  brightCyan: "#a3e6ef",
  brightWhite: "#ffffff",
};

/** The ink a filled cursor sits on. */
const FALLBACK_CURSOR_INK = "#06080e";
const FALLBACK_FOREGROUND = "#e6e9f5";
const FALLBACK_CURSOR = "#8ea2ff";
const FALLBACK_SELECTION = "#8ea2ff47";

/**
 * Builds the theme xterm is given.
 *
 * `background` is **fully transparent on purpose**. The tint and the backdrop
 * blur belong to the panel around the terminal (`.terminal-surface` in the
 * stylesheet), not to the canvas: that keeps one declaration for the surface
 * instead of one in CSS and one in JavaScript, and it means xterm is never
 * handed the alpha channel it cannot parse. Everything else is opaque or
 * 8-digit hex, both of which it reads correctly.
 */
export function xtermTheme(lookup: ColorLookup): ITheme {
  const theme: ITheme = {
    background: "#00000000",
    foreground: colorOr(lookup("--ink"), lookup, FALLBACK_FOREGROUND),
    cursor: colorOr(lookup("--terminal-cursor"), lookup, FALLBACK_CURSOR),
    cursorAccent: colorOr(
      lookup("--terminal-cursor-ink"),
      lookup,
      FALLBACK_CURSOR_INK,
    ),
    selectionBackground: colorOr(
      lookup("--terminal-selection"),
      lookup,
      FALLBACK_SELECTION,
    ),
  };

  for (const [slot, property] of ANSI_SLOTS) {
    // The index signature keeps this loop honest without a cast per slot.
    (theme as Record<string, string>)[slot as string] = colorOr(
      lookup(property),
      lookup,
      FALLBACK_ANSI[slot as string] ?? FALLBACK_FOREGROUND,
    );
  }

  return theme;
}

/**
 * Reads custom properties off an element, raw.
 *
 * Raw matters: `getPropertyValue` may or may not substitute a `var()` before
 * returning it depending on the engine and on whether the property is
 * registered, which is exactly the kind of ambiguity that produces a palette
 * that works in one WebView and not the next. `resolveColor` does the
 * substitution itself so the result is the same everywhere.
 */
export function elementLookup(element: Element): ColorLookup {
  const styles = getComputedStyle(element);
  return (name) => styles.getPropertyValue(name) || undefined;
}

/** The theme for the document as it currently stands. */
export function currentXtermTheme(): ITheme {
  if (typeof document === "undefined") return xtermTheme(() => undefined);
  // Read from the root, where the theme's custom properties are declared.
  return xtermTheme(elementLookup(document.documentElement));
}
