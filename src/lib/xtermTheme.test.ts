import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { ANSI_SLOTS, resolveColor, toXtermColor } from "./xtermTheme";
import type { ColorLookup } from "./xtermTheme";

/**
 * These tests exist because the terminal shipped black.
 *
 * The palette was written in the modern `rgb(8 10 16 / 0.86)` syntax, which
 * xterm's parser does not accept: it discarded the value and fell back to its
 * default `#000000`, silently, with the foreground still parsing correctly. The
 * symptom was dark ink on a black void in the middle of a light theme.
 *
 * So the coverage is deliberately two-sided. The unit tests pin the accepted
 * syntaxes, and the stylesheet test walks the actual CSS and fails if any token
 * the terminal reads is written in a form xterm would throw away. That second
 * one is the guard: it catches the mistake where it would actually be made.
 */

const ROOT = fileURLToPath(new URL("../../", import.meta.url));
const STYLES = readFileSync(`${ROOT}src/styles.css`, "utf8");

/** Every custom property the stylesheet declares, last declaration winning. */
function declaredProperties(css: string): Map<string, string> {
  const found = new Map<string, string>();
  const pattern = /^\s*(--[A-Za-z0-9_-]+)\s*:\s*([^;]+);/gm;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(css)) !== null) {
    found.set(match[1], match[2].trim());
  }
  return found;
}

const DECLARED = declaredProperties(STYLES);
const lookup: ColorLookup = (name) => DECLARED.get(name);

describe("toXtermColor", () => {
  /**
   * Every value leaves as eight digits, `#rrggbbaa`.
   *
   * That is a property of the module rather than an accident of it. xterm takes
   * both `#rrggbb` and `#rrggbbaa`, and normalising to the longer form means an
   * opaque colour and a translucent one travel exactly the same path — so a
   * caller never has to branch on which shape came back, and the tests below
   * can assert one shape throughout.
   */
  it("always emits eight digits, padding opaque colours", () => {
    expect(toXtermColor("#8ea2ff")).toBe("#8ea2ffff");
    expect(toXtermColor("#8ea2ff47")).toBe("#8ea2ff47");
    expect(toXtermColor("#8EA2FF")).toBe("#8ea2ffff");
  });

  it("expands the shorthand hex forms", () => {
    expect(toXtermColor("#fab")).toBe("#ffaabbff");
    expect(toXtermColor("#fab7")).toBe("#ffaabb77");
  });

  it("accepts the comma form, which is the only functional form xterm takes", () => {
    expect(toXtermColor("rgb(10, 20, 30)")).toBe("#0a141eff");
    expect(toXtermColor("rgba(10, 20, 30, 0.5)")).toBe("#0a141e80");
    expect(toXtermColor("rgb(10,20,30)")).toBe("#0a141eff");
  });

  it("converts the space-and-slash form rather than discarding it", () => {
    // The exact value that made the terminal black: valid CSS, invisible to
    // xterm's parser, and silently replaced by `#000000`.
    expect(toXtermColor("rgb(8 10 16 / 0.86)")).toBe("#080a10db");
    expect(toXtermColor("rgb(250 251 255 / 0.9)")).toBe("#fafbffe6");
    expect(toXtermColor("rgb(8 10 16)")).toBe("#080a10ff");
  });

  it("understands percentage channels and percentage alpha", () => {
    expect(toXtermColor("rgb(100% 0% 0%)")).toBe("#ff0000ff");
    expect(toXtermColor("rgb(0 0 0 / 50%)")).toBe("#00000080");
  });

  it("clamps out-of-range numbers instead of producing a broken hex", () => {
    expect(toXtermColor("rgb(300, 0, 0)")).toBe("#ff0000ff");
    expect(toXtermColor("rgba(0, 0, 0, 2)")).toBe("#000000ff");
  });

  it("returns null for anything it cannot guarantee xterm will read", () => {
    // A named colour resolves to rgb() only through a real element, so it must
    // not be guessed at here.
    expect(toXtermColor("rebeccapurple")).toBeNull();
    expect(toXtermColor("currentColor")).toBeNull();
    expect(toXtermColor("linear-gradient(red, blue)")).toBeNull();
    expect(toXtermColor("")).toBeNull();
    expect(toXtermColor(undefined)).toBeNull();
    expect(toXtermColor("#12345")).toBeNull();
    expect(toXtermColor("rgb(1, 2)")).toBeNull();
  });
});

describe("resolveColor", () => {
  it("follows a var() reference", () => {
    const vars = new Map([
      ["--accent", "#4f5bd5"],
      ["--terminal-cursor", "var(--accent)"],
    ]);
    expect(resolveColor(vars.get("--terminal-cursor"), (n) => vars.get(n))).toBe("#4f5bd5ff");
  });

  it("follows a chain, and stops rather than looping forever", () => {
    const vars = new Map([
      ["--a", "var(--b)"],
      ["--b", "var(--c)"],
      ["--c", "#123456"],
      // A cycle, which a stylesheet should never contain but a typo can.
      ["--loop-a", "var(--loop-b)"],
      ["--loop-b", "var(--loop-a)"],
    ]);
    const get = (n: string) => vars.get(n);
    expect(resolveColor(vars.get("--a"), get)).toBe("#123456ff");
    expect(resolveColor(vars.get("--loop-a"), get)).toBeNull();
  });
});

describe("the stylesheet's terminal tokens", () => {
  /**
   * Everything the terminal hands to xterm. `--terminal-bg` is here because the
   * surface uses it *and* the theme could, so writing it in a form only CSS
   * understands is a trap worth catching even though the theme currently sends
   * a transparent background instead.
   */
  const names = [
    ...ANSI_SLOTS.map(([, property]) => property),
    "--ink",
    "--terminal-bg",
    "--terminal-cursor",
    "--terminal-cursor-ink",
    "--terminal-selection",
  ];

  it("declares every token the terminal reads", () => {
    for (const name of names) {
      expect(DECLARED.has(name), `${name} is missing from styles.css`).toBe(true);
    }
  });

  it("declares each of them in a form xterm can parse", () => {
    // The whole point: an unparseable value is not an error at runtime, it is a
    // black terminal. Catching it here is the only place it is visible.
    for (const name of names) {
      const resolved = resolveColor(DECLARED.get(name), lookup);
      expect(resolved, `${name} = ${DECLARED.get(name)} is not an xterm colour`).not.toBeNull();
    }
  });

  it("would have caught the value that shipped", () => {
    // The exact regression, kept as a case so the guard cannot be weakened into
    // passing. `rgb(8 10 16 / 0.86)` is valid CSS and invisible to xterm, whose
    // parser wants commas and whose canvas fallback refuses any alpha.
    expect(toXtermColor("rgb(8 10 16 / 0.86)")).not.toBeNull();
    expect(toXtermColor("rgb(8 10 16 / 0.86)")).toBe("#080a10db");
  });

  it("spells all sixteen ANSI slots out, rather than leaving xterm's defaults", () => {
    // Only fourteen slots in the original bug: two happened to be set, the rest
    // silently stayed xterm's pure-hue palette.
    expect(ANSI_SLOTS).toHaveLength(16);
    const properties = ANSI_SLOTS.map(([, property]) => property);
    expect(new Set(properties).size).toBe(16);
  });
});
