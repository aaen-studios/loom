// Verifies that the generated token sheet actually reached the built stylesheet.
//
// `GATE-TEST.md` records the experiment that proved Tailwind resolves `@utility`
// out of an `@import`ed sheet. This proves the *real* thing does: that the app's
// genuine design tokens — both palettes, the glass surfaces, the buttons, the
// motion — are present in what Next emits, and that the app-only rules are not.
//
// Run it after `next build`. It exits 1 on any failure, so it gates CI.
//
// Two lessons shape the checks below, and both of them cost a debugging cycle
// when they were learned.
//
//  1. **Assert on the minified form.** Next's CSS minifier rewrites
//     `rgb(190 208 255)` as `#bed0ff`, so asserting the source spelling fails on
//     a stylesheet that is perfectly correct. The built CSS is normalised first.
//  2. **Match rules, not substrings.** A CSS comment that merely *mentions*
//     `.ask-reply` is not the rule. Every forbidden check looks for the selector
//     followed by `{`, which a comment cannot produce.
import { readFileSync, readdirSync } from "node:fs";
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
console.log(
  `built CSS: ${files.join(", ")} (${(raw.length / 1024).toFixed(1)} kB)\n`,
);

/**
 * Tokens that must be present. Each entry names the thing it protects, because a
 * failure here should say what stopped working rather than which string moved.
 */
const REQUIRED = [
  // Both palettes, out of the `tokens` region.
  ["dark palette ink", "--ink:#f4f6fc"],
  ["light palette ink", "--ink:#0a0c16"],
  ["dark accent", "--accent:#8ea2ff"],
  ["light accent", "--accent:#4f5bd5"],
  ["thread tokens (dark)", "--thread-bright:#bed0ff"],
  ["thread tokens (light)", "--thread-bright:#6070e2"],
  ["the dark variant survived the import", ":where(.dark,.dark *)"],
  ["utilities nested inside html:not(.dark)", "html:not(.dark)"],
  // The concentric radius scale.
  ["radius scale: window", "--radius-window:20px"],
  ["radius scale: sheet", "--radius-sheet:18px"],
  ["radius scale: row", "--radius-row:10px"],
  ["radius scale: control", "--radius-control:12px"],
  // Surfaces.
  ["panel utility", ".panel{"],
  ["panel-strong utility", ".panel-strong{"],
  ["panel keeps its blur", "backdrop-filter:blur(34px)"],
  ["pill utility", ".pill{"],
  ["blob utility", ".blob{"],
  // Controls.
  ["btn-primary", ".btn-primary{"],
  ["btn-ghost", ".btn-ghost{"],
  ["chip", ".chip{"],
  ["kbd", ".kbd{"],
  ["text-soft", ".text-soft{"],
  ["text-faint", ".text-faint{"],
  ["hover-surface", ".hover-surface"],
  ["squircle corners survived minification", "corner-shape:squircle"],
  // Motion.
  ["loom-drift keyframes", "@keyframes loom-drift"],
  ["animate-drift", ".animate-drift{"],
  ["loom-fade-up", "@keyframes loom-fade-up"],
  ["intro-step", ".intro-step{"],
  ["thread-draw (the mark weaving in)", "@keyframes loom-thread-draw"],
  ["loom-mark-weaving", ".loom-mark-weaving"],
  ["cursor-blink", "@keyframes loom-blink"],
  ["reduced-motion override", "prefers-reduced-motion"],
  // The thinking panel.
  ["thinking shimmer", ".thinking-shimmer"],
  ["loom-thinking spine", ".loom-thinking{"],
];

/**
 * App-only rules that must NOT be in this stylesheet, matched as *rules* so a
 * passing mention in a comment cannot register as a hit.
 *
 * These are the reason the marked regions are drawn where they are rather than
 * around the whole file. The app is a fixed, chrome-less window that never
 * scrolls; inheriting its shell rules would leave the site unable to scroll at
 * all, which is a spectacular way to break a marketing page.
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
];

let failed = 0;

for (const [label, needle] of REQUIRED) {
  const ok = css.includes(needle);
  if (!ok) failed += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${label}`);
}

console.log("");
for (const [label, needle] of FORBIDDEN) {
  const ok = !css.includes(needle);
  if (!ok) failed += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  absent: ${label}`);
}

// The app's shell rule is `body { overflow: hidden }`. `overflow:hidden` on its
// own is not evidence of it — Tailwind's own `.truncate` and `.overflow-hidden`
// produce the identical string, and this site uses both — so read the actual
// `body` rule and check that one.
console.log("");
const bodyRule = css.match(/(?:^|[},])body\{([^}]*)\}/)?.[1] ?? "";
const bodyHasHidden = bodyRule.includes("overflow:hidden");
console.log(
  `${bodyHasHidden ? "FAIL" : "PASS"}  absent: the app shell's body overflow rule`,
);
if (bodyHasHidden) failed += 1;
console.log(`        body rule: ${bodyRule.trim() || "(none)"}`);

console.log("");
if (failed === 0) {
  console.log("VERDICT: the app's tokens are in the built stylesheet, and the");
  console.log("app-only shell rules stayed out of it.");
} else {
  console.log(`VERDICT: ${failed} check(s) failed.`);
  process.exit(1);
}
