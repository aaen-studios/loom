// Verifies that the *generated* token sheet reached the built stylesheet.
//
// The gate test (`GATE-TEST.md`) proved Tailwind resolves `@utility` through an
// `@import`. This proves the real thing does: that the app's actual design
// tokens — both palettes, the glass surfaces, the buttons, the motion — are
// present in what Next emits, and that the app-only rules are not.
//
// Run after `next build`. Exits 1 on any failure, so it gates CI.
//
// Two lessons that shape the checks below:
//
//  1. **Assert on the minified form.** Next's CSS minifier rewrites
//     `rgb(190 208 255)` as `#bed0ff`. Asserting the source's spelling fails on
//     a stylesheet that is perfectly correct, so the built CSS is normalised
//     (rgb → hex) before comparing.
//  2. **Match rules, not substrings.** A CSS comment mentioning `.ask-reply` is
//     not a rule. Every forbidden check looks for the selector followed by `{`,
//     which a comment cannot produce — that false positive cost a debugging
//     cycle when this check was first written.
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
  console.error("No CSS was emitted by the build.");
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

/** Tokens that must be present, and why each one matters. */
const REQUIRED = [
  // --- both palettes, from the `tokens` region -------------------------
  ["dark palette ink", "--ink:#f4f6fc"],
  ["light palette ink", "--ink:#0a0c16"],
  ["dark accent", "--accent:#8ea2ff"],
  ["light accent", "--accent:#4f5bd5"],
  ["thread tokens (dark)", "--thread-bright:#bed0ff"],
  ["thread tokens (light)", "--thread-bright:#6070e2"],
  ["the dark variant reached the sheet", ":where(.dark,.dark *)"],
  ["nested utilities inside html:not(.dark)", "html:not(.dark)"],
  // --- the radius scale ------------------------------------------------
  ["radius scale: window", "--radius-window:20px"],
  ["radius scale: sheet", "--radius-sheet:18px"],
  ["radius scale: row", "--radius-row:10px"],
  // --- surfaces --------------------------------------------------------
  ["panel utility", ".panel{"],
  ["panel-strong utility", ".panel-strong{"],
  ["panel uses backdrop-filter", "backdrop-filter:blur(34px)"],
  ["pill utility", ".pill{"],
  ["blob utility", ".blob{"],
  // --- controls --------------------------------------------------------
  ["btn-primary", ".btn-primary{"],
  ["btn-ghost", ".btn-ghost{"],
  ["chip", ".chip{"],
  ["kbd", ".kbd{"],
  ["text-soft", ".text-soft{"],
  ["text-faint", ".text-faint{"],
  ["hover-surface", ".hover-surface"],
  ["squircle corners survive minification", "corner-shape:squircle"],
  // --- motion ----------------------------------------------------------
  ["loom-drift keyframes", "@keyframes loom-drift"],
  ["animate-drift", ".animate-drift{"],
  ["loom-fade-up", "@keyframes loom-fade-up"],
  ["intro-step", ".intro-step{"],
  ["thread-draw (the mark weaving in)", "@keyframes loom-thread-draw"],
  ["loom-mark-weaving", ".loom-mark-weaving"],
  ["cursor-blink", "@keyframes loom-blink"],
  ["reduced-motion override", "prefers-reduced-motion"],
  ["thinking shimmer", ".thinking-shimmer"],
  ["loom-thinking spine", ".loom-thinking{"],
];

/**
 * App-only rules that must NOT be in the site's stylesheet, matched as *rules*
 * so a passing mention in a comment cannot register. These are why the token
 * regions are drawn where they are: the app is a fixed, chrome-less window that
 * never scrolls, and inheriting its shell rules would stop the site scrolling.
 */
const FORBIDDEN = [
  ["app shell: #root height", "#root{"],
  ["quick-ask overlay (a different surface)", ".ask-rail{"],
  ["quick-ask overlay", ".ask-shuttle{"],
  ["quick-ask overlay", ".ask-reply{"],
  ["markdown rendering (app only)", ".loom-markdown{"],
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

// The app's shell rule is `body { overflow: hidden }`. `overflow:hidden` alone
// is not evidence of it — Tailwind's own `.truncate` and `.overflow-hidden`
// produce the same string, and the site uses both — so this reads the actual
// `body` rule and checks that one.
console.log("");
const bodyRule = css.match(/(?:^|[},])body\{([^}]*)\}/)?.[1] ?? "";
const bodyHasHidden = bodyRule.includes("overflow:hidden");
console.log(
  `${bodyHasHidden ? "FAIL" : "PASS"}  absent: app shell's body overflow rule`,
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
