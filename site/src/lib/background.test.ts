import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { DARK_DIM_FLOOR, PORCELAIN } from "./background";

/**
 * The backdrop is copied by hand out of the app, which means it is the one thing
 * in this project that can quietly stop matching the product it is advertising.
 *
 * This is the guard. It does not check the layer stack — those strings are
 * compared by eye, and the comment in `background.ts` says so — but it does check
 * the flat fill, which is the value that has to be exact: it is the colour the
 * page paints before React mounts, so being slightly off shows up as a flash of
 * one shade before another.
 *
 * The app's source is read at test time rather than imported. Importing it would
 * drag React and `@tauri-apps/api` into this project's dependency graph for the
 * sake of one string.
 */
const APP_BACKGROUND = fileURLToPath(
  new URL("../../../src/lib/background.ts", import.meta.url),
);

/** Pulls the `base` out of the app's `porcelain` preset. */
function appPorcelainBase(): string {
  const source = readFileSync(APP_BACKGROUND, "utf8");
  const preset = source.match(/id: "porcelain"[\s\S]*?base: "([^"]+)"/);
  if (!preset) throw new Error("could not find the porcelain preset in the app's background.ts");
  return preset[1];
}

describe("the backdrop", () => {
  test("paints the app's Porcelain base colour, exactly", () => {
    // If Porcelain is re-tinted in the app, this fails — which is the point. The
    // alternative is a landing page rendering a background the product never had,
    // and nothing else in the build would notice.
    expect(PORCELAIN.base).toBe(appPorcelainBase());
  });

  test("matches the colour globals.css paints <html> with", () => {
    // Three copies of the same hex exist by necessity — this module, the CSS
    // custom property, and the theme-boot script in `layout.tsx` — because none
    // of them can read the others at the moment they are needed. This asserts the
    // CSS one agrees; the boot script's is asserted by the fact that omitting it
    // produces a visible flash, which no test can see.
    const globals = readFileSync(
      fileURLToPath(new URL("../app/globals.css", import.meta.url)),
      "utf8",
    );
    expect(globals).toContain(`--site-light: ${PORCELAIN.base};`);
  });

  test("keeps a dark veil rather than dropping to zero", () => {
    // Porcelain is a light preset and the dark palette is near-white ink, so at
    // zero the text would be unreadable over it. This is the app's own floor.
    expect(DARK_DIM_FLOOR).toBeGreaterThan(0);
    expect(DARK_DIM_FLOOR).toBeLessThan(100);
  });

  test("builds its layers from gradients, not from a bundled image", () => {
    // No artwork ships, which is the app's arrangement too: nothing needs
    // clearing for redistribution and there is no asset to go stale.
    expect(PORCELAIN.layers).toContain("repeating-linear-gradient");
    expect(PORCELAIN.layers).toContain("radial-gradient");
    expect(PORCELAIN.layers).not.toContain("url(");
  });
});
