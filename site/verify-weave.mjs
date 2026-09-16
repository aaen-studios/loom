// Verifies that the weaving is actually there in the built output.
//
// The other three verifiers check the tokens, the pages and the markup. None of
// them can tell whether the loom itself survived the build — because the loom is
// not a page and not a token, it is a *relationship between three things that
// cannot read each other*:
//
//   1. `--warp-columns` in `globals.css` — how many threads the CSS draws.
//   2. `WARP_THREADS` in `components/chrome/warp.tsx` — how many it renders.
//   3. `PASSES` in `lib/weave/passes.ts` — how many picks the page has, and what
//      they are called.
//
// Those numbers have to agree or the metaphor silently becomes decoration: threads
// that do not line up with the grid, a weft that measures sections the page does
// not have, a nav strip pointing at anchors that were renamed. All three would
// build cleanly and look almost right, which is the worst kind of wrong.
//
// So this script reads all three — TypeScript source as text, because importing a
// React component into a build-time script to count its output would be worse than
// a regex — and then reads the built HTML to confirm the page actually rendered
// what the source promised.
//
// It exits 1 on any failure, so it gates CI.
import { readFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const read = (relative) => readFileSync(join(here, relative), "utf8");

let failed = 0;
const check = (label, ok, detail) => {
  if (!ok) failed += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${label}${detail ? `\n        ${detail}` : ""}`);
};

// --- 1. The source of truth ------------------------------------------------

const passesSource = read("src/lib/weave/passes.ts");
const warpSource = read("src/components/chrome/warp.tsx");
const css = read("src/app/globals.css");

/**
 * Every pass, with its id and its pick number.
 *
 * Matched on the two fields appearing in order rather than by parsing TypeScript,
 * which would need a compiler to do honestly. The pattern is strict enough that a
 * pass written without a `pick` does not match at all, and the count is compared
 * against the number of `id:` keys below so a missed entry cannot pass silently.
 */
const passPattern = /id:\s*"([a-z-]+)",\s*\n\s*pick:\s*(\d+),/g;
const passes = [...passesSource.matchAll(passPattern)].map((match) => ({
  id: match[1],
  pick: Number(match[2]),
}));

const declaredIds = (passesSource.match(/\n\s{4}id:\s*"/g) ?? []).length;
check(
  "the draft was parsed completely",
  passes.length > 0 && passes.length === declaredIds,
  `parsed ${passes.length} pass(es) but found ${declaredIds} id field(s)`,
);

check(
  "the picks are numbered from one, in order, with no gaps",
  passes.every((pass, index) => pass.pick === index + 1),
  passes.map((pass) => `${pass.pick}:${pass.id}`).join(" "),
);

const warpThreads = Number(
  warpSource.match(/WARP_THREADS\s*=\s*(\d+)/)?.[1] ?? "0",
);
const cssColumns = Number(
  css.match(/--warp-columns:\s*(\d+)/)?.[1] ?? "0",
);

// --- 2. The three numbers agreeing -----------------------------------------

check(
  "the CSS and the component agree on how many threads the warp has",
  warpThreads > 1 && warpThreads === cssColumns,
  `warp.tsx says ${warpThreads}, globals.css says ${cssColumns}`,
);

// --- 3. The built page -----------------------------------------------------

const htmlPath = join(here, ".next", "server", "app", "index.html");
if (!existsSync(htmlPath)) {
  console.error(`\nNo build output at ${htmlPath}. Run \`bun run build\` first.\n`);
  process.exit(1);
}

const html = readFileSync(htmlPath, "utf8");

// React emits `<!-- -->` between adjacent text nodes, so a check for a rendered
// string has to look at the normalised form.
const visible = html.replace(/<!--\s*-->/g, "");

const warpLines = (html.match(/class="warp-line"/g) ?? []).length;
check(
  "the warp rendered one thread per column",
  warpLines === warpThreads,
  `found ${warpLines} thread element(s), expected ${warpThreads}`,
);

check(
  "the weft is in the prerender, so it exists before any measurement",
  visible.includes('class="weft"'),
);

const renderedPasses = (html.match(/data-pass="/g) ?? []).length;
check(
  "every pick in the draft is on the page",
  renderedPasses === passes.length,
  `found ${renderedPasses} rendered pass(es), the draft declares ${passes.length}`,
);

// Each pick's number has to survive to the DOM: the shuttle picks the active pass
// by reading these in order, so a wrong or missing number puts the weft in the
// wrong place.
const renderedNumbers = [...html.matchAll(/data-pass="(\d+)"/g)].map((match) =>
  Number(match[1]),
);
check(
  "the rendered picks are the draft's picks",
  renderedNumbers.join(",") === passes.map((pass) => pass.pick).join(","),
  `rendered ${renderedNumbers.join(",")} — draft has ${passes
    .map((pass) => pass.pick)
    .join(",")}`,
);

// --- 4. The anchors the nav strip promises ---------------------------------

// Every destination in the draft strip is an `id` on a rendered element, or the
// link silently does nothing when clicked.
const ids = new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((match) => match[1]));
const dangling = passes.filter((pass) => !ids.has(pass.id));
check(
  "every pass has an anchor the nav can reach",
  dangling.length === 0,
  dangling.map((pass) => `#${pass.id}`).join(", "),
);

// --- 5. The CSS that does the weaving --------------------------------------

/**
 * Classes and properties the loom is made of, checked in the *source* stylesheet.
 *
 * Deliberately not checked in the built CSS the way `verify-tokens.mjs` checks the
 * tokens: Tailwind's minifier is free to reorder and rewrite these, and the thing
 * worth asserting is that they were authored at all. What they *resolve to* is
 * checked by the next two assertions, which are about the shared thread palette —
 * the part that has to come from the app rather than from this file.
 */
const WOVEN = [
  [".warp-field", "the fixed warp layer"],
  [".warp-line", "an individual thread"],
  [".warp-grid", "the shared twelve-column measure"],
  [".weft", "the reading line"],
  [".knot", "a single crossing marker"],
  [".rail", "a vertical thread"],
  [".cloth-pass", "one pass of the transcript"],
  [".draft-row", "one row of the draft beside the cloth"],
  [".draft-cell", "one cell of the nav strip"],
  [".selvedge", "the finished edge"],
  ["loom-knot-pulse", "the pulse on the live pass"],
];

for (const [needle, what] of WOVEN) {
  check(`the stylesheet defines ${what}`, css.includes(needle));
}

/**
 * The loom is drawn from the app's thread palette.
 *
 * This is the assertion that keeps the shared-token arrangement honest: those seven
 * `--thread-*` properties live in the *tokens* region, in both palettes, because the
 * app uses them for the quick-ask overlay's own woven column. If someone re-tints
 * the overlay in the app, the loom on this site follows — which is the entire point
 * of generating the sheet rather than inventing a palette for the landing page.
 *
 * The corresponding prohibition — that the overlay's own `.ask-*` *rules* must not
 * reach this stylesheet — is enforced in `verify-tokens.mjs`, because that is the
 * script that reads the built CSS where a stray rule would actually do damage.
 */
for (const token of ["--thread-line", "--thread-bright", "--thread-glow", "--thread-soft"]) {
  check(`the loom is strung with the app's own ${token}`, css.includes(token));
}

// --- 6. The weft's knots sit on the outermost threads ----------------------

/*
 * The one geometry bug this site has actually had, guarded so it cannot come back.
 *
 * The cloth is capped at `--warp-max` and centred, so on a viewport wider than the
 * cap the outermost threads are inset by the centring margin *plus* the gutter. The
 * weft's end-knots were originally pinned at `--warp-gutter`, which on a 5120px
 * screen put them roughly 1800px away from the threads they are supposed to cross —
 * invisible to anyone testing on a laptop, glaring on a wide monitor.
 *
 * So: the knots must use the derived edge, and the derived edge must include the
 * centring term. Asserting the second as well as the first, because dropping the
 * `max(0px, …)` term would silently break *narrow* viewports instead.
 */
check(
  "the weft's knots are anchored to the derived thread edge",
  /\.weft::(?:before|after)\s*\{[^}]*var\(--warp-edge\)/s.test(css),
  "the knots no longer use --warp-edge, so they will float in mid-air on a wide viewport",
);

check(
  "the derived edge accounts for the cloth being centred",
  /--warp-edge:[^;]*max\(\s*0px/.test(css),
  "--warp-edge lost its `max(0px, …)` term, which breaks narrow viewports instead",
);

// --- 6. The split between frame and content --------------------------------

// The warp is a fixed layer *behind* everything and the content is lifted above
// it, so the threads read as being behind the page rather than laid over it. If the
// content wrapper loses its stacking context, the weft crosses the words instead of
// passing behind them.
const layout = read("src/app/layout.tsx");
check(
  "the content sits above the warp and the weft",
  /relative z-10/.test(layout),
  "layout.tsx no longer wraps the pages in a stacking context above the fixed layers",
);

console.log("");
if (failed === 0) {
  console.log("VERDICT: the warp, the draft and the cloth agree, and the loom is");
  console.log("strung with the app's own thread colours.");
} else {
  console.log(`VERDICT: ${failed} check(s) failed.`);
  process.exit(1);
}
