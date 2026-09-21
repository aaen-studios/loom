// Verifies that the generated token sheet reached the built stylesheet, and that the
// application's own surfaces stayed out of the document.
//
// `GATE-TEST.md` records the experiment that proved Tailwind resolves `@utility` out of
// an `@import`ed sheet. This proves the *real* thing does: that the app's genuine design
// tokens — both palettes, the concentric radius scale, the two controls this document
// borrows, the motion override — are present in what Next emits.
//
// Run it after `next build`. It exits 1 on any failure, so it gates CI.
//
// ---------------------------------------------------------------------------
// Four lessons, each of which cost a debugging cycle
// ---------------------------------------------------------------------------
//
//  1. **Assert on the minified form.** Next's CSS minifier rewrites `rgb(190 208 255)`
//     as `#bed0ff`, so asserting the source spelling fails on a stylesheet that is
//     perfectly correct. The built CSS is normalised first.
//  2. **Match rules, not substrings.** A CSS comment that merely *mentions* `.ask-reply`
//     is not the rule. Every absence check looks for the selector followed by `{`, which
//     a comment cannot produce.
//  3. **An unused `@utility` is not emitted — but "unused" is not what you think.**
//     Tailwind generates only the utilities its candidate scan finds, and that scan
//     reads *everything in the project*, prose included. So the word "panel" in a
//     sentence generates `@utility panel` exactly as reliably as `className="panel"`
//     does. This is the finding that reshaped this file; the long note below sets out
//     what it means and what is checked instead.
//  4. **Plain rules in a copied region always arrive.** Only `@utility` definitions are
//     conditional. An ordinary rule inside a `loom-site:` region — `.animate-drift`,
//     `.cursor-blink`, `@keyframes loom-drift` — is copied verbatim into the generated
//     sheet and therefore reaches this project unconditionally. That is not a defect in
//     the sync; it is the reason the discipline is *which regions are marked* rather
//     than which rules are used.
//
// ---------------------------------------------------------------------------
// The glass: what is not checkable here, and what is checked instead
// ---------------------------------------------------------------------------
//
// The previous version of this file asserted that the built stylesheet contained no
// `.panel{`, `.pill{`, `.blob{`, `.animate-drift{` and no `backdrop-filter` at all. All
// of those failed, and every one of them was failing for a reason that was *not* a
// defect in the document:
//
//   - `.panel{` and `.pill{` were emitted because this document's own prose says
//     "panel" and "pill". The page discusses eight dock panels and a control pill; the
//     scanner cannot tell those words from class names, and correctly refuses to try.
//     Removing the words would be the tail wagging the dog — they are the subject
//     matter.
//   - `.blob{` and `.panel-strong{` were emitted because the *dev probe* named them in
//     a `querySelectorAll` string, and because this very file named them in its
//     `FORBIDDEN` list. The check was generating the thing it checked. The probe now
//     measures computed style instead, and Tailwind's scan is confined to `src/`, which
//     removes both of those self-references.
//   - `backdrop-filter` and `.animate-drift{` arrive from **plain rules** in the copied
//     regions, which by (4) above can never be conditional. There is no arrangement of
//     the sync in which they are absent while the regions carry them.
//
// So a stylesheet grep is the wrong instrument for "is there glass on this page". The
// right instrument is the rendered document, and the assertion lives in two places that
// can state it truthfully:
//
//   - `components/dev/probe.tsx` walks every element and checks its *computed* style
//     for a blur, a texture, an infinite animation or a painted fixed layer. That is
//     stronger than any grep: it holds however the class name arrived.
//   - the markup check below reads the built HTML and asserts no element carries one of
//     the glass class names. This is exact, it needs no browser, and it is the one
//     thing that would catch the case a computed-style walk cannot see — glass on an
//     element that happens to be hidden.
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
  console.log(`${ok ? "PASS" : "FAIL"}  ${label}${detail ? `\n        ${detail}` : ""}`);
};

// --- 1. The tokens the document is set in ----------------------------------

/**
 * Every entry names what it protects, because a failure here should say what stopped
 * working rather than which string moved.
 */
const REQUIRED = [
  // Both palettes, out of the `tokens` region. A document set in the application's own
  // colours is the one thing that makes this site *of* the product rather than a
  // brochure about it, so these must never be missing.
  ["dark palette ink", "--ink:#f4f6fc"],
  ["light palette ink", "--ink:#0a0c16"],
  ["dark accent", "--accent:#8ea2ff"],
  ["light accent", "--accent:#4f5bd5"],
  ["dark soft ink", "--ink-soft:#f4f6fcc2"],
  ["light soft ink", "--ink-soft:#0a0c16d1"],
  ["the hairline the whole document is ruled with", "--glass-border:#ffffff1f"],
  // The app's thread row, which the figures are drawn in.
  ["thread tokens (dark)", "--thread-bright:#bed0ff"],
  ["thread tokens (light)", "--thread-bright:#6070e2"],
  ["utilities nested inside html:not(.dark)", "html:not(.dark)"],
  // The concentric radius scale, on the two blocks a document has: the release panel
  // and a shell session.
  ["radius scale: sheet", "--radius-sheet:18px"],
  ["radius scale: control", "--radius-control:12px"],
  ["squircle corners survived minification", "corner-shape:squircle"],
  // The application's controls that a document genuinely has a use for: one action, one
  // secondary one, and keycaps in the shortcut appendix.
  ["btn-primary (the one action)", ".btn-primary{"],
  ["btn-ghost (the secondary one)", ".btn-ghost{"],
  ["kbd (the shortcut appendix)", ".kbd{"],
  ["text-soft", ".text-soft{"],
  ["text-faint", ".text-faint{"],
  ["hover-surface", ".hover-surface"],
  // The document's own numbers, which land in this stylesheet and nowhere else a build
  // step can read them.
  ["the reading measure", "--measure:34rem"],
  ["the ground, named once", "--page:"],
  ["the reduced-motion override", "prefers-reduced-motion"],
];

for (const [label, needle] of REQUIRED) {
  const ok = css.includes(needle);
  if (!ok) failed += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${label}`);
}

// --- 2. Both palettes resolve, and in the right order ----------------------

/*
 * The custom variant is `@custom-variant dark (&:where(.dark, .dark *))`, and the
 * minifier does not necessarily leave `:where` spelled that way — so asserting the
 * spelling was the wrong test and it failed against a stylesheet that was correct.
 *
 * What actually matters is *source order*: the token sheet's `:root` is the dark
 * palette, and `html:not(.dark)` overrides it for light. If the light block ever landed
 * before the dark one, both would apply and the cascade would pick the dark values —
 * which fails silently, everywhere, in the direction that is hardest to notice.
 *
 * So: the dark values must come first, and the light block must come after them.
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

// --- 3. The application's own regions stayed out ---------------------------

/*
 * These are the checks the marked regions exist for, and unlike the `@utility`
 * definitions above they are genuinely absent — because they are plain rules that were
 * never marked, so the sync has no way to copy them.
 *
 * The app is a fixed, chrome-less window that never scrolls. Inheriting its shell rules
 * would leave this document unable to scroll; inheriting its overlay or its markdown
 * pipeline would bring surfaces this page has no business showing.
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
  ["the app's dock chrome", ".glass-thin{"],
];

console.log("");
for (const [label, needle] of FORBIDDEN) {
  const ok = !css.includes(needle);
  if (!ok) failed += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  absent: ${label}`);
}

/*
 * The application's shell rule is `body { overflow: hidden }`.
 *
 * `overflow:hidden` on its own is not evidence of it — Tailwind's own `.truncate` and
 * `.overflow-hidden` produce the identical string, and a document uses both — so read
 * the actual `body` rule and check that one. The same rule also asserts the absence of
 * any frost on the page ground, which is the one surface where glass would be both
 * invisible and wrong.
 */
console.log("");
const bodyRule = css.match(/(?:^|[},])body\{([^}]*)\}/)?.[1] ?? "";
const bodyHasHidden = bodyRule.includes("overflow:hidden");
const bodyHasBlur = bodyRule.includes("backdrop-filter");
check("the app shell's body overflow rule stayed out", !bodyHasHidden);
check("the page ground is a flat colour, with no frost on it", !bodyHasBlur);
console.log(`        body rule: ${bodyRule.trim() || "(none)"}`);

/*
 * And here is the honest note about what this file can no longer claim.
 *
 * `backdrop-filter` *is* present in the built stylesheet, several times over, and the
 * reasons are set out in the header comment. What is worth doing here — rather than
 * asserting something untrue — is printing the count and the sources, so the next
 * person to notice it finds the explanation instead of re-deriving it.
 */
console.log("");
const blurCount = (css.match(/backdrop-filter/g) ?? []).length;
console.log(`NOTE  backdrop-filter appears ${blurCount} time(s) in the built stylesheet.`);
console.log("      None of it is reachable from the document: the app's glass utilities");
console.log("      are emitted because this page's prose says \"panel\" and \"pill\", and");
console.log("      the rest is from plain rules in the copied motion region, which the");
console.log("      token sync can never make conditional. The check that no element");
console.log("      actually paints a blur is the computed-style walk in dev/probe.tsx,");
console.log("      and the exact markup check immediately below.");

// --- 4. The markup, which is where absence can be stated exactly -----------

/*
 * The built HTML, read directly.
 *
 * This is the one absence check that is both exact and cheap: a `class="panel"` in the
 * markup is unambiguous, needs no browser, and catches a glass surface on an element
 * that happens to be hidden — which a computed-style walk over visible elements cannot.
 *
 * Between them the two checks cover the question completely: this one catches the class
 * being *applied*, the probe catches the effect being *painted*, and neither can be
 * satisfied by accident.
 */
const htmlPath = join(here, ".next", "server", "app", "index.html");
if (!existsSync(htmlPath)) {
  console.error(`\nNo build output at ${htmlPath}. Run \`bun run build\` first.\n`);
  process.exit(1);
}

const html = readFileSync(htmlPath, "utf8");
console.log("");
for (const name of [
  "panel",
  "panel-strong",
  "pill",
  "blob",
  "glass-thin",
  "terminal-surface",
  "animate-drift",
  "intro-step",
  "cursor-blink",
  "thinking-shimmer",
  "loom-thinking",
  "loom-mark-weaving",
  "warp-field",
  "warp-line",
  "spine",
  "weft",
  "draft-cell",
]) {
  // `class="panel"` and `class="panel …"`, but not `panel-strong` when checking
  // `panel` — which is why the boundary is stated rather than left to a substring.
  const applied = new RegExp(`class="[^"]*\\b${name}\\b[^"]*"`).test(html);
  check(`no element in the document carries "${name}"`, !applied);
}

// The strongest single statement available without a browser: the ground is painted in
// `globals.css` and in the boot script, both as a flat colour, so no element in the
// document may declare a filter, a texture or a gradient in an inline style.
const inlineGlass = html.match(/style="[^"]*(backdrop-filter|url\(&quot;data:image\/svg)/g) ?? [];
check(
  "no element carries a filter or a texture inline",
  inlineGlass.length === 0,
  inlineGlass.slice(0, 2).join(" | "),
);

console.log("");
if (failed === 0) {
  console.log("VERDICT: the application's tokens are in the built stylesheet, the three");
  console.log("controls the document borrows arrive, the app-only regions stayed out,");
  console.log("and no element in the document carries the application's glass.");
} else {
  console.log(`VERDICT: ${failed} check(s) failed.`);
  process.exit(1);
}
