#!/usr/bin/env node
// Measures the rendered document and prints the result as text.
//
//   node probe-layout.mjs                 every viewport, both motion preferences
//   node probe-layout.mjs 1440            one width
//   PORT=3003 node probe-layout.mjs       another dev server
//
// ---------------------------------------------------------------------------
// Why text rather than a screenshot
// ---------------------------------------------------------------------------
//
// The claims this document makes are *geometric*, and none of them is visible in a
// build, a typecheck or a prerendered HTML dump.
//
//   - The text column must be the measure. This is the defect the rebuild was largely
//     about: the previous layout let prose run a ten-column shed, which at the cap is
//     about 150 characters a line — twice what anyone can read without losing their
//     place — and an over-long line typechecks, builds and renders perfectly.
//   - The margin index and the running head's section indicator must be gated in
//     opposite directions, so "where am I" is answered exactly once at any width:
//     never twice, and never zero times.
//   - A figure must not be shrunk below legibility, and the table breakout must not give
//     the document a horizontal scrollbar.
//
// Every one of those fails in a viewport-dependent way. A column that is two gutters
// too narrow looks fine at 1440 and cramped at 390. An index that appears one breakpoint
// early is invisible until the exact width where it overlaps the text. So a regression
// here is invisible on the machine you develop on and glaring on the machine you own —
// which is the same argument the previous version of this tool made about a weft line
// that followed the scroll, and it is why the tool survived the rebuild when the element
// it was written for did not.
//
// The page reports its own measurements (`components/dev/probe.tsx`, behind `?probe=1`)
// and this script drives headless Chrome over it and prints the numbers. It reads the
// installed Chrome rather than downloading a browser, and it needs no dependency.
//
// Not part of `bun run verify`: that has to stay fast and dependency-free for CI, and
// this needs a running dev server. It is the tool you reach for while changing the
// layout.
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";

const PORT = process.env.PORT ?? "3002";
const URL = `http://localhost:${PORT}/?probe=1`;

/**
 * The widths worth checking.
 *
 * 390 is a phone, where the figure scrolls rather than shrinking and the running head
 * carries the section. 1152 is the margin index's breakpoint exactly — a breakpoint is
 * the one kind of layout decision that can be wrong at exactly one width, so probing it
 * only at 1440 would never test the boundary. 1440 is an ordinary laptop. 5120 is not
 * hypothetical: it is the ultrawide this project is developed on, and it is the width at
 * which the measure either holds or drifts into a 200-character line.
 *
 * `dsf` is the device scale factor, and the phone entry needs it: headless Chrome refuses
 * to make a window narrower than about 500px, so asking for `--window-size=390` silently
 * gives a 500px CSS viewport — which is a tablet, not a phone, and every narrow-screen
 * check would be made against the wrong layout. Doubling the window and doubling the
 * scale factor produces a genuine 390×844 CSS viewport.
 */
const VIEWPORTS = [
  { width: 5120, height: 1400, dsf: 1, note: "the ultrawide this is built on" },
  { width: 1440, height: 900, dsf: 1, note: "an ordinary laptop" },
  { width: 1152, height: 900, dsf: 1, note: "the margin index's breakpoint, exactly" },
  { width: 390, height: 844, dsf: 2, note: "a phone" },
];

const CHROME_CANDIDATES = [
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
  `${process.env.LOCALAPPDATA}\\Google\\Chrome\\Application\\chrome.exe`,
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
];

const chrome = CHROME_CANDIDATES.find((candidate) => candidate && existsSync(candidate));
if (!chrome) {
  console.error(
    "No Chrome found. Looked in:\n" + CHROME_CANDIDATES.map((c) => `  ${c}`).join("\n"),
  );
  process.exit(1);
}

/** Runs the page once at one viewport and returns the probe's lines. */
function probe(viewport, reduced) {
  const args = [
    "--headless=new",
    "--no-sandbox",
    "--hide-scrollbars",
    // Virtual time, so React's effects and the font swap have both happened before the
    // DOM is dumped. Without it the dump can land before the probe mounts.
    "--virtual-time-budget=4000",
    `--window-size=${Math.round(viewport.width * viewport.dsf)},${Math.round(viewport.height * viewport.dsf)}`,
    `--force-device-scale-factor=${viewport.dsf}`,
  ];

  // Both branches matter, and not only for the shared `reduce` override. The section
  // indicator is checked under both because the honest reading of that preference is
  // that it removes *movement*, not *position*: which section you are in is information,
  // and withholding it from someone who asked for less animation would be removing a
  // feature rather than respecting a preference. That distinction is easy to get wrong in
  // the component and impossible to see in a screenshot.
  if (reduced) args.push("--force-prefers-reduced-motion");

  args.push("--dump-dom", URL);

  const dom = execFileSync(chrome, args, {
    encoding: "utf8",
    stdio: "pipe",
    maxBuffer: 64 * 1024 * 1024,
  });

  const block = dom.match(/<pre id="loom-probe">([\s\S]*?)<\/pre>/);
  if (!block) return null;
  return block[1]
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&")
    .split("\n")
    .map((line) => line.trimEnd())
    .filter((line) => line.trim());
}

const only = process.argv[2] ? Number(process.argv[2]) : null;
const targets = only ? VIEWPORTS.filter((v) => v.width === only) : VIEWPORTS;

let failures = 0;

for (const viewport of targets) {
  for (const reduced of [false, true]) {
    console.log(`\n${"=".repeat(74)}`);
    console.log(
      `  ${viewport.width}×${viewport.height} @${viewport.dsf}x — ${viewport.note}` +
        `${reduced ? "   [prefers-reduced-motion: reduce]" : ""}`,
    );
    console.log(`${"=".repeat(74)}`);

    let lines;
    try {
      lines = probe(viewport, reduced);
    } catch (error) {
      console.log(`  could not run Chrome: ${error.message.split("\n")[0]}`);
      console.log(`  is a dev server listening on ${PORT}?`);
      failures += 1;
      continue;
    }

    if (!lines) {
      console.log("  no probe output — is the dev server running, and is it a dev build?");
      console.log("  (the probe is development-only by design)");
      failures += 1;
      continue;
    }

    for (const line of lines) {
      const failed = line.startsWith("CHECK") && line.includes("FAIL");
      if (failed) failures += 1;
      console.log(`  ${failed ? "FAIL " : "     "}${line}`);
    }
  }
}

console.log("");
if (failures === 0) {
  console.log("the column, the index and the drawings hold at every width checked");
} else {
  console.log(`${failures} failure(s)`);
  process.exit(1);
}
