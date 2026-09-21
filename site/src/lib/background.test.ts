import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { DARK, LIGHT, PAGE } from "./background";

/**
 * The page ground is copied out of the application, which makes it the one value
 * in this project that can quietly stop matching the product it documents.
 *
 * The app's source is read as text at test time rather than imported: importing it
 * would drag React and `@tauri-apps/api` into this project's dependency graph for
 * the sake of two strings.
 *
 * ---------------------------------------------------------------------------
 * A bug this file had, and why the assertions are about *arrangement*
 * ---------------------------------------------------------------------------
 *
 * The first version looked for `base: "…"` inside the porcelain preset — a quoted
 * literal. It passed for as long as the app wrote one, and then the app was
 * refactored so that porcelain's base became `base: THEME_BACKDROP.light`, a
 * reference to a shared constant. At that point a lazy `[\s\S]*?` walked past
 * porcelain entirely and matched the next preset that still had a literal
 * (`linen`, `#f1ece2`) — so the test reported that the site was wrong when in fact
 * the site was exactly right and the test was reading a preset two hundred lines
 * further down the file.
 *
 * A check that matches the wrong thing passes for months and then fails for a
 * reason that has nothing to do with the code under test. The fix is not a better
 * regex; it is to assert the arrangement the app actually uses, which is stronger
 * than any value comparison:
 *
 *   1. `THEME_BACKDROP` holds the two pre-mount colours as literals.
 *   2. Porcelain's `base` is a *reference* to it, not a second copy of the hex.
 *   3. This module equals those two literals, and `globals.css` agrees.
 *
 * Step 2 is a claim about the app rather than about this site, and it is the most
 * useful of the three: it is what stops the app reintroducing a second literal
 * that could drift from its own `index.html`. If someone inlines the hex again,
 * this fails and says so.
 */
const APP_BACKGROUND = fileURLToPath(
  new URL("../../../src/lib/background.ts", import.meta.url),
);

const appSource = readFileSync(APP_BACKGROUND, "utf8");

/** The two pre-mount colours, read out of the app's `THEME_BACKDROP`. */
function appThemeBackdrop(): { light: string; dark: string } {
  const block = appSource.match(/THEME_BACKDROP[^=]*=\s*\{([^}]*)\}/)?.[1];
  if (!block) throw new Error("could not find THEME_BACKDROP in the app's background.ts");

  const read = (key: string): string => {
    const value = block.match(new RegExp(`${key}:\\s*"([^"]+)"`))?.[1];
    if (!value) throw new Error(`THEME_BACKDROP has no string "${key}"`);
    return value;
  };

  return { light: read("light"), dark: read("dark") };
}

/** What porcelain's `base` field is *set to* — a literal or a reference. */
function porcelainBase(): string {
  const preset = appSource.match(/id: "porcelain"[\s\S]*?\n  \},/)?.[0];
  if (!preset) throw new Error("could not find the porcelain preset in the app's background.ts");

  const base = preset.match(/base:\s*([^,\n]+),/)?.[1]?.trim();
  if (!base) throw new Error("porcelain has no base field");
  return base;
}

const globals = readFileSync(
  fileURLToPath(new URL("../app/globals.css", import.meta.url)),
  "utf8",
);

describe("the page ground", () => {
  test("is the app's own pre-mount colour, exactly", () => {
    // If either moves in the app, this fails — which is the point. The
    // alternative is a page whose first painted frame is a colour the product
    // never paints.
    const backdrop = appThemeBackdrop();
    expect(LIGHT).toBe(backdrop.light);
    expect(DARK).toBe(backdrop.dark);
  });

  test("porcelain still resolves its base from that one constant", () => {
    // See the note at the top: this is the assertion that would have caught the
    // bug the previous version of this file had, and it protects the *app* rather
    // than this site.
    expect(porcelainBase()).toBe("THEME_BACKDROP.light");
  });

  test("is named in globals.css under the same two values", () => {
    // Three copies of each hex exist necessarily — this module, the custom
    // properties, and the theme-boot script in `layout.tsx` — because none of them
    // can read the others at the moment it is needed. This checks the CSS ones;
    // the boot script's are checked by the fact that omitting them produces a
    // visible flash, which no test can see.
    expect(globals).toContain(`--site-light: ${LIGHT};`);
    expect(globals).toContain(`--site-dark: ${DARK};`);
  });

  test("picks the dark ground as the default, matching the token sheet", () => {
    // The generated sheet's `:root` is the *dark* palette — the app ships with
    // artwork behind it — so `--page` has to default to the dark colour and light
    // has to be the exception on `html:not(.dark)`. Getting this backwards is
    // invisible in a browser and wrong for anyone whose stored theme is dark.
    expect(globals).toContain("--page: var(--site-dark);");
    expect(globals).toContain("html:not(.dark)");
    expect(PAGE.dark).toBe(DARK);
    expect(PAGE.light).toBe(LIGHT);
  });

  test("is legible in both themes, by more than the app's own veil", () => {
    // The regression this pins down: dark mode used to be the app's 48% veil over
    // Porcelain, which lands near `#7d8188` and gives near-white ink about 2.4:1.
    // Nothing failed, because nothing was checking — so now something does, and it
    // checks the composite a reader actually looks at rather than the veil.
    const luminance = (hex: string): number => {
      const value = hex.replace("#", "");
      const channels = [0, 2, 4].map(
        (offset) => Number.parseInt(value.slice(offset, offset + 2), 16) / 255,
      );
      const linear = channels.map((channel) =>
        channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4,
      );
      return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
    };

    const ratio = (a: string, b: string) => {
      const [bright, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
      return (bright + 0.05) / (dark + 0.05);
    };

    // The light palette's `--ink-soft` is `rgb(10 12 22 / 0.82)`; over Porcelain
    // that composites to about this.
    expect(ratio(LIGHT, "#26293a")).toBeGreaterThan(7);
    // The dark palette's `--ink-soft` is `rgb(244 246 252 / 0.76)`.
    expect(ratio(DARK, "#bcc0cd")).toBeGreaterThan(7);
  });

  test("paints one flat colour, with nothing behind the text", () => {
    // The check that keeps the deletions deleted. A gradient, a texture, a
    // blurred wash or a grain overlay is exactly what this rebuild was asked to
    // remove, and each is a two-line change to add back.
    for (const value of [LIGHT, DARK]) {
      expect(value).toMatch(/^#[0-9a-f]{6}$/);
    }
    expect(globals).not.toContain("repeating-linear-gradient");
    expect(globals).not.toContain("radial-gradient");
    expect(globals).not.toContain("feTurbulence");
    expect(globals).not.toContain("backdrop-filter");
  });
});
