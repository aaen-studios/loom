#!/usr/bin/env node
// Measures the rendered page and prints the result as text.
//
//   node probe-layout.mjs                 every viewport, both motion preferences
//   node probe-layout.mjs 1440            one width
//   PORT=3003 node probe-layout.mjs       another dev server
//
// ---------------------------------------------------------------------------
// Why text rather than a screenshot
// ---------------------------------------------------------------------------
//
// Every claim this page makes is one a screenshot cannot settle and a build step cannot see.
//
//   - **The figures are drawn.** A generator that returns an empty path list renders as nothing at all,
//     silently, and a movement with a blank plate in it looks like a deliberate choice. The only way to
//     know is to count the geometry in a laid-out DOM.
//   - **The geometry is identical on both sides.** The hero's figure is computed once on the server and
//     once in the browser, from the same seed. If they disagree the page flashes a different drawing
//     before settling — and the flash is only visible on a fast machine, which is never CI.
//   - **The threads are the application's colours.** Every stroke is a `var(--thread-*)`, so the artwork
//     follows the theme with no second code path. A single literal colour would be the one thing on the
//     page that did not change with the theme, and it would look *nearly* right in the theme it was
//     written in.
//   - **Nothing is wider than the shell.** A figure that bleeds without knowing where the edges are is
//     the one layout bug that gives a page a horizontal scrollbar, and it is invisible at the width it
//     was written for.
//   - **The reveal never hides content.** Its worst case has to be "no animation" rather than "no text".
//
// A screenshot could catch some of that — but only if someone remembered to look at the right width, and
// only by eye. This asserts numbers.
//
// The page reports its own measurements (`components/dev/probe.tsx`, behind `?probe=1`) and this script
// drives headless Chrome over it and prints the numbers. It reads the installed Chrome rather than
// downloading a browser, and it needs no dependency.
//
// Not part of `bun run verify`: that has to stay fast and dependency-free for CI, and this needs a
// running dev server. It is the tool you reach for while changing the layout or the figures.
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";

const PORT = process.env.PORT ?? "3002";
const URL = `http://localhost:${PORT}/?probe=1`;

/**
 * The widths worth checking.
 *
 * 390 is a phone, where the hero's weave falls back to a still frame and the figures have to hold
 * together at a width they were not tuned at. 1024 and 1440 bracket an ordinary laptop. 2560 is here
 * because a fluid figure is most likely to look wrong at a width nobody develops on — a thread field
 * tuned at 1440 is a different texture at 2560, and the failure is invisible until someone with a big
 * monitor opens it.
 *
 * `dsf` is the device scale factor, and the phone entry needs it: headless Chrome refuses to make a
 * window narrower than about 500px, so asking for `--window-size=390` silently gives a 500px CSS
 * viewport — which is a tablet, not a phone, and every narrow-screen check would be made against the
 * wrong layout. Doubling the window and doubling the scale factor produces a genuine 390×844 CSS
 * viewport.
 */
const VIEWPORTS = [
  { width: 2560, height: 1400, dsf: 1, note: "a wide monitor" },
  { width: 1440, height: 900, dsf: 1, note: "an ordinary laptop" },
  { width: 1024, height: 800, dsf: 1, note: "a small laptop" },
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
    // Virtual time, so the effects, the hero's animation and the font swap have all happened before the
    // DOM is dumped. Without it the dump lands before the figures mount and every measurement is taken
    // against an empty frame.
    "--virtual-time-budget=4000",
    `--window-size=${Math.round(viewport.width * viewport.dsf)},${Math.round(viewport.height * viewport.dsf)}`,
    `--force-device-scale-factor=${viewport.dsf}`,
  ];

  // Both branches matter, and not only for the shared `reduce` override. The figures are checked under
  // both because the honest reading of the preference is that it removes *movement*, not *content*: the
  // still frames must be there either way, and if the reduced case were a blank plate the page would be
  // broken for exactly the readers most likely to be reading it carefully.
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
  console.log("the figures, the shell and the reveal hold at every width checked");
} else {
  console.log(`${failures} failure(s)`);
  process.exit(1);
}
