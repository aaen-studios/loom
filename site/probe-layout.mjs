#!/usr/bin/env node
// Measures the rendered site and prints the result as text.
//
//   node probe-layout.mjs                 every viewport, both motion preferences
//   node probe-layout.mjs 1440            one width
//   PORT=3003 node probe-layout.mjs       another dev server
//
// ---------------------------------------------------------------------------
// Why text rather than a screenshot
// ---------------------------------------------------------------------------
//
// Several of this page's defining ideas are geometry, not content: the warp threads
// have to line up with the columns the content sits in, and the weft's end-knots have
// to sit on the outermost of those threads. Both are invisible to a build, a
// typecheck and the three `verify-*.mjs` scripts, because all three read markup rather
// than a laid-out page.
//
// And both fail in a viewport-dependent way: the knots are placed with
// `calc(max(0px, (100vw - 92rem) / 2) + <gutter>)`, which on a 1440px laptop is simply
// the gutter and on a 5120px monitor is the gutter *plus* 1824px of centring margin.
// So a regression there is invisible on the machine you develop on and glaring on the
// machine you own.
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
 * 5120 is not hypothetical — it is the monitor this was developed on, and it is the
 * width that exposed the knot bug. 1440 is an ordinary laptop. 390 is a phone, where
 * twelve columns are 39px apart and everything still has to hold together.
 *
 * `dsf` is the device scale factor, and the phone entry needs it: headless Chrome
 * refuses to make a window narrower than about 500px, so asking for `--window-size=390`
 * silently gives a 500px CSS viewport — which is a tablet, not a phone, and the
 * narrow-screen checks would all be made against the wrong layout. Doubling the window
 * and doubling the scale factor produces a genuine 390×844 CSS viewport.
 */
const VIEWPORTS = [
  { width: 5120, height: 1400, dsf: 1, note: "the ultrawide this was built on" },
  { width: 1440, height: 900, dsf: 1, note: "an ordinary laptop" },
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
    // Virtual time, so React's effects and the font swap have both happened before
    // the DOM is dumped. Without it the dump can land before the probe mounts.
    "--virtual-time-budget=4000",
    `--window-size=${Math.round(viewport.width * viewport.dsf)},${Math.round(viewport.height * viewport.dsf)}`,
    `--force-device-scale-factor=${viewport.dsf}`,
  ];

  // Both branches matter. `reduce` is what a headless browser reports by default, and
  // the path a visitor with that preference actually gets — which is how the weft was
  // found to be invisible for them.
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
  console.log("the layout holds at every width and motion preference checked");
} else {
  console.log(`${failures} failure(s)`);
  process.exit(1);
}
