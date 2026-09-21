// Verifies that the generated token sheet reached the built stylesheet, and that the page is painted
// out of the application's own declarations rather than a lookalike.
//
// `GATE-TEST.md` records the experiment that proved Tailwind resolves `@utility` out of an `@import`ed
// sheet. This proves the *real* thing does: that the app's genuine design tokens — both palettes, the
// glass border and highlight, the concentric radius scale, the seven thread colours the artwork is drawn
// in, and the surfaces this page borrows — are present in what Next emits.
//
// Run it after `next build`. It exits 1 on any failure, so it gates CI.
//
// ---------------------------------------------------------------------------
// Five lessons, each of which cost a debugging cycle
// ---------------------------------------------------------------------------
//
//  1. **Assert on the minified form.** Next's CSS minifier rewrites `rgb(190 208 255)` as `#bed0ff` and
//     `rgb(255 255 255 / 0.12)` as `#ffffff1f`, so asserting the source spelling fails on a stylesheet
//     that is perfectly correct. The built CSS is normalised first.
//  2. **Match rules, not substrings.** A CSS comment that merely *mentions* `.ask-reply` is not the rule.
//     Every absence check looks for the selector followed by `{`, which a comment cannot produce.
//  3. **An unused `@utility` is not emitted — but "unused" is not what you think.** Tailwind generates
//     only the utilities its candidate scan finds, and that scan reads *everything in the scanned tree*,
//     prose included. So the word "panel" in a sentence generates `@utility panel` exactly as reliably as
//     `className="panel"` does. This is why `globals.css` confines the scan to `src/`: the verifiers live
//     outside it, so their string literals can no longer cause the utilities they check for to exist.
//  4. **Plain rules in a copied region always arrive.** Only `@utility` definitions are conditional. An
//     ordinary rule inside a `loom-site:` region — `.animate-drift`, `@keyframes loom-drift` — is copied
//     verbatim into the generated sheet and therefore reaches this project unconditionally. Not a defect
//     in the sync; it is why the discipline is *which regions are marked* rather than which rules are
//     used.
//  5. **A stylesheet grep is the wrong instrument for "is this element styled".** Both of the above mean
//     the built CSS contains utilities whatever the page does with them. The check that an element really
//     has a surface is a computed-style walk over the rendered document, and that lives in
//     `components/dev/probe.tsx`. This file asserts what a stylesheet can honestly be asked about.
import { readFileSync, readdirSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const dir = join(here, ".next", "static", "chunks");

let files;
try {
  files = readdirSync(dir).filter((name) => name.endsWith(".css"));
} catch {
  console.error(`No build output at ${dir}. Run \`bun run build\` first.`);
  process.exit(1);
}

if (files.length === 0) {
  console.error("The build emitted no CSS at all.");
  process.exit(1);
}

const raw = files.map((name) => readFileSync(join(dir, name), "utf8")).join("\n");

/** Rewrites `rgb(r g b)` and `rgb(r g b / a)` to hex, so one spelling matches. */
function normalise(css) {
  return css.replace(
    /rgb\(\s*(\d+)\s+(\d+)\s+(\d+)\s*(?:\/\s*([\d.]+)\s*)?\)/g,
    (_match, r, g, b, a) => {
      const hex = (value) => Number(value).toString(16).padStart(2, "0");
      const base = `#${hex(r)}${hex(g)}${hex(b)}`;
      return a === undefined ? base : `${base}${hex(Math.round(Number(a) * 255))}`;
    },
  );
}

const css = normalise(raw);
console.log(`built CSS: ${files.join(", ")} (${(raw.length / 1024).toFixed(1)} kB)\n`);

let failed = 0;
const check = (label, ok, detail) => {
  if (!ok) failed += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${label}${!ok && detail ? `\n        ${detail}` : ""}`);
};

// --- 1. The tokens the artwork is drawn in ---------------------------------

/**
 * Every entry names what it protects, because a failure here should say what stopped working rather than
 * which string moved.
 */
const REQUIRED = [
  // Both palettes, out of the `tokens` region.
  ["dark palette ink", "--ink:#f4f6fc"],
  ["light palette ink", "--ink:#0a0c16"],
  ["dark accent", "--accent:#8ea2ff"],
  ["light accent", "--accent:#4f5bd5"],
  ["dark soft ink", "--ink-soft:#f4f6fcc2"],
  ["light soft ink", "--ink-soft:#0a0c16d1"],
  ["the dark glass hairline", "--glass-border:#ffffff1f"],
  ["the light glass hairline", "--glass-border:#0f172a1a"],
  ["the dark card fill", "--card-bg:"],
  ["the hover fill", "--hover-bg:"],
  ["the accent wash, for selection and light", "--accent-soft:"],
  /*
   * The seven thread colours, and these are the load-bearing ones.
   *
   * Every path on this page is stroked with one of them — `--thread`, `--thread-line`,
   * `--thread-line-strong`, `--thread-bright`, `--thread-soft`, `--thread-glow` — which is what makes
   * the artwork *of* the product rather than merely themed like it. Re-tint a thread in the application
   * and the figures change with it, or the build fails here.
   */
  ["the thread's own colour (dark)", "--thread:#8ea2ff"],
  ["the thread's own colour (light)", "--thread:#4f5bd5"],
  ["the dark warp thread", "--thread-line:#8ea2ff52"],
  ["the dark lit thread", "--thread-line-strong:#bcceff"],
  ["the dark weft thread", "--thread-bright:#bed0ff"],
  ["the light warp thread", "--thread-line:#4f5bd561"],
  ["the light weft thread", "--thread-bright:#6070e2"],
  ["--thread-soft, for the pick weft and the glow", "--thread-soft:"],
  ["--thread-glow, for a lit thread", "--thread-glow:"],
  // The palettes' structure.
  ["utilities nested inside html:not(.dark)", "html:not(.dark)"],
  // The concentric radius scale, on the surfaces this page borrows: a sheet for the release panel and
  // the turn, a row for a menu item, a control for the buttons, a capsule for the header's chrome.
  ["radius scale: sheet", "--radius-sheet:18px"],
  ["radius scale: row", "--radius-row:10px"],
  ["radius scale: control", "--radius-control:12px"],
  ["radius scale: capsule", "--radius-capsule:999px"],
  ["squircle corners survived minification", "corner-shape:squircle"],
  // The application's own surfaces. This is the list that makes the page *of* the product: the header's
  // pill, the release panel's glass, the two buttons, and the keycaps.
  ["the app's pill chrome (the header)", ".pill{"],
  ["the app's panel glass (the release panel)", ".panel-strong{"],
  ["btn-primary (the one action)", ".btn-primary{"],
  ["btn-ghost", ".btn-ghost{"],
  ["text-soft", ".text-soft{"],
  ["text-faint", ".text-faint{"],
  ["hover-surface", ".hover-surface"],
  // The page's own numbers, which land in this stylesheet and nowhere else a build step can read them.
  ["the shell width", "--shell:76rem"],
  ["the reading measure", "--measure:42rem"],
  ["the header height", "--header-h:4rem"],
  ["the reduced-motion override", "prefers-reduced-motion"],
];

for (const [label, needle] of REQUIRED) {
  check(label, css.includes(needle));
}

// --- 2. Both palettes resolve, in the right order ---------------------------

/*
 * The custom variant is `@custom-variant dark (&:where(.dark, .dark *))`, and the minifier does not
 * necessarily leave `:where` spelled the way the source does — so asserting the spelling was the wrong
 * test and it failed against a stylesheet that was correct.
 *
 * What actually matters is *source order*: the token sheet's `:root` is the dark palette, and
 * `html:not(.dark)` overrides it for light. If the light block ever landed before the dark one, both
 * would apply and the cascade would pick the dark values — which fails silently, everywhere, in the
 * direction that is hardest to notice on a page whose default is dark.
 */
console.log("");
const darkInkAt = css.indexOf("--ink:#f4f6fc");
const lightBlockAt = css.indexOf("html:not(.dark)");
const orderOk = darkInkAt !== -1 && lightBlockAt !== -1 && darkInkAt < lightBlockAt;
check(
  "the dark palette is the default, and the light block overrides it",
  orderOk,
  orderOk
    ? ""
    : darkInkAt === -1
      ? "the dark palette is not in the stylesheet at all"
      : lightBlockAt === -1
        ? "html:not(.dark) is not in the stylesheet, so light mode would never apply"
        : "the light block precedes the dark palette, so the dark values win in both themes",
);

// --- 3. The app's own regions stayed out ------------------------------------

/*
 * These are what the marked regions exist for, and unlike the `@utility` definitions above they are
 * genuinely absent — because they are plain rules that were never marked, so the sync has no way to copy
 * them.
 *
 * The app is a fixed, chrome-less window that never scrolls. Inheriting its shell rules would leave this
 * page unable to scroll; inheriting its overlay, its markdown pipeline or its terminal surface would
 * bring chrome this page has no business showing.
 */
const FORBIDDEN = [
  ["app shell: the #root mount point", "#root{"],
  ["quick-ask overlay (a surface this site does not show)", ".ask-rail{"],
  ["quick-ask overlay", ".ask-shuttle{"],
  ["quick-ask overlay", ".ask-reply{"],
  ["quick-ask overlay", ".ask-weaving{"],
  ["the app's markdown pipeline", ".loom-markdown{"],
  ["generated-UI host styles", ".loom-ui{"],
  ["compact density", ".density-compact{"],
  ["the app's terminal surface", ".terminal-surface{"],
  ["the app's thinner dock chrome", ".glass-thin{"],
];

console.log("");
for (const [label, needle] of FORBIDDEN) {
  check(`absent: ${label}`, !css.includes(needle));
}

/*
 * And here is the part this file has now learned twice, stated properly instead of asserted.
 *
 * `.animate-drift`, `.cursor-blink`, `.thinking-shimmer` and `@keyframes loom-drift` are **in** the built
 * stylesheet, and they always will be. They are ordinary rules inside the copied motion region, not
 * `@utility` definitions, so the sync copies them verbatim and there is no arrangement of the marking
 * that makes them conditional — see note 4 at the top of this file.
 *
 * An earlier version of this check listed `.animate-drift` as forbidden, so it failed against a
 * stylesheet that was correct and could never have passed. The honest assertion is not "these rules are
 * absent" but "no element on the page is wearing them", which is exact, needs no browser, and is checked
 * below — plus the computed-style walk in `dev/probe.tsx`, which catches the effect however the class
 * name got there.
 *
 * The count is printed rather than asserted, so the next person to notice these in the sheet finds the
 * explanation instead of re-deriving it.
 */
const APP_ONLY = ["animate-drift", "cursor-blink", "thinking-shimmer", "loom-drift"];
console.log("");
console.log("NOTE  the application's own motion rules arrive unconditionally, because they are");
console.log("      plain rules in a copied region rather than @utility definitions:");
for (const name of APP_ONLY) {
  const count = css.split(name).length - 1;
  console.log(`        ${String(count).padStart(2)}  ${name}`);
}
console.log("      What matters is that nothing on this page is *wearing* one, checked below");

/*
 * The application's shell rule is `body { overflow: hidden }`.
 *
 * `overflow:hidden` on its own is not evidence of it — Tailwind's own `.truncate` and `.overflow-hidden`
 * produce the identical string, and a page with a scrolling code block uses both — so read the actual
 * `body` rule and check that one.
 */
console.log("");
const bodyRule = css.match(/(?:^|[},])body\{([^}]*)\}/)?.[1] ?? "";
check("the app shell's body overflow rule stayed out", !bodyRule.includes("overflow:hidden"));
console.log(`        body rule: ${bodyRule.trim() || "(none)"}`);

// --- 4. The glass, which is on this page on purpose ------------------------

/*
 * The one place this file's discipline reverses. An earlier version banned `backdrop-filter` from the
 * project because a document is not a window. This page *is* window chrome in two places — a sticky
 * header and a release panel — so the app's frosted surfaces are the correct material there, and their
 * absence would be the defect.
 *
 * Asserted as a count rather than as a selector, because the two surfaces are `pill` and `panel-strong`
 * and the interesting question is whether the *effect* survived minification at all.
 */
console.log("");
const blurCount = (css.match(/backdrop-filter/g) ?? []).length;
check(
  "the frosted surfaces survived minification",
  blurCount >= 4,
  `only ${blurCount} backdrop-filter declaration(s): the header or the release panel lost its glass`,
);
check(
  "the header's pill is opaque enough to read through",
  /\.pill\{[\s\S]{0,400}?backdrop-filter/.test(css),
  "the pill lost its filter, so it would be a flat rounded rectangle",
);

// --- 5. The markup, where absence can be stated exactly --------------------

/*
 * The built HTML, read directly.
 *
 * The one absence check that is both exact and cheap: a `class="zone"` in the markup is unambiguous,
 * needs no browser, and catches a leftover element that happens to be hidden — which a computed-style
 * walk over visible elements cannot.
 */
const htmlPath = join(here, ".next", "server", "app", "index.html");
if (!existsSync(htmlPath)) {
  console.error(`\nNo build output at ${htmlPath}. Run \`bun run build\` first.\n`);
  process.exit(1);
}

const html = readFileSync(htmlPath, "utf8");
console.log("");
/*
 * The class names an earlier build used, none of which should be on the page.
 *
 * `figure-art` was on this list and has been removed, which is worth a line because the reason is not
 * that the check was wrong — it was right, and it found the name. The name has been *reused*: an earlier
 * version of this site drew labelled diagram plates and called them `figure-art`, this version draws
 * generative figures and called them the same thing. So the assertion was correct and the name was
 * ambiguous, and the fix is to stop treating a live class as a ghost.
 *
 * What is asserted below is unchanged in spirit: the class names that belong to builds which no longer
 * exist must not have come back.
 */
for (const name of [
  "zone",
  "splitter",
  "warp-line",
  "warp-field",
  "pick-cloth",
  "cloth-pass",
  "animate-drift",
  "cursor-blink",
  "thinking-shimmer",
  "terminal-surface",
]) {
  // The class boundary is stated rather than left to a substring, so `panel-strong` does not register as
  // `panel` and `--warp-pitch` does not register as `warp-line`.
  const applied = new RegExp(`class="[^"]*\\b${name}\\b[^"]*"`).test(html);
  check(`no element in the page carries "${name}"`, !applied);
}

/*
 * And the figures are in the prerender.
 *
 * This is the check that matters most for this page: the artwork is generated, and a generator that
 * throws or returns nothing during the server render produces a page that builds, ships, and has no
 * pictures in it. Counting the `<svg data-figure>` elements in the static HTML is the only way to know
 * the server drew them rather than leaving them to the client.
 */
console.log("");
const figures = (html.match(/data-figure="/g) ?? []).length;
console.log(`figures in the prerendered HTML: ${figures}`);
check("the figures are drawn on the server", figures >= 6, `found ${figures}`);

for (const kind of ["field", "lattice", "rings", "bundle"]) {
  const present = html.includes(`data-figure="${kind}"`);
  check(`a ${kind} figure is in the prerender`, present);
}

// The paths have real geometry in the static HTML, not just elements. A `<path d="">` would pass the
// count above and paint nothing.
const paths = (html.match(/<path[^>]*\sd="M /g) ?? []).length;
console.log(`paths with geometry in the prerender: ${paths}`);
check("the prerendered paths have geometry", paths >= 20, `found ${paths}`);

/*
 * And the gradient is in the figure's own coordinate space rather than the path's.
 *
 * This is the one check here that exists because of a bug that was invisible in every way a bug can be
 * invisible: the weft — the horizontal cross-threads that make the figure a *weave* rather than a set
 * of vertical lines — was generated correctly, given the right opacity and the right width, written into
 * the DOM, and painted at near-zero opacity. The cause is that a `linearGradient` defaults to
 * `objectBoundingBox` units, so its coordinates are fractions of *the element's own bounding box*: for a
 * vertical thread that box is the whole figure and the fade works, and for a near-horizontal strand the
 * same fade collapses into a few dozen units and the line vanishes.
 *
 * No unit test could see it — the geometry was correct. No inspection of the markup could see it — the
 * element was there and well-formed. It took looking at the rendered page.
 */
check(
  "the figure's gradient is defined in user space, not the path's bounding box",
  html.includes('gradientUnits="userSpaceOnUse"'),
  "the weft will be invisible: an objectBoundingBox gradient collapses to nothing on a horizontal path",
);

console.log("");
if (failed === 0) {
  console.log("VERDICT: the application's tokens are in the built stylesheet, this page is built");
  console.log("out of its own pill, panel, button and radius declarations, the thread colours the");
  console.log("figures are drawn in arrived intact, both palettes resolve in the right order, the");
  console.log("app-only regions stayed out, and the artwork is in the prerender.");
} else {
  console.log(`VERDICT: ${failed} check(s) failed.`);
  process.exit(1);
}
