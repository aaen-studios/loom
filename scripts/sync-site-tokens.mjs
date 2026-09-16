#!/usr/bin/env node
// Extracts the design tokens Loom's app and website share.
//
//   node scripts/sync-site-tokens.mjs           write site/src/app/loom-tokens.css
//   node scripts/sync-site-tokens.mjs --check   verify it is current; exit 1 if not
//
// The app's `src/styles.css` marks the shared regions with
// `loom-site:<name>:start` / `loom-site:<name>:end` comments. Those regions are
// copied **verbatim** into a generated stylesheet that the site imports, so the
// two cannot drift: there is one copy of the tokens, and this script is what
// makes it two files.
//
// Copying rather than sharing a package is deliberate. A shared package would
// mean the desktop app's stylesheet either lives outside the app or the app
// depends on a package that exists only to hold CSS; either way a change to the
// app's look would have to travel through the site's build to reach the app.
//
// Why verbatim matters: the regions contain Tailwind v4 *directives*
// (`@utility`, `@custom-variant`), not just custom properties. `site/GATE-TEST.md`
// records the experiment proving Tailwind resolves those when they arrive
// through an `@import`, which is what makes a plain copy valid here.
//
// CI runs this with `--check`, so a change to a marked region cannot reach main
// without shipping the regenerated file alongside it.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const SOURCE = join(root, "src", "styles.css");
const TARGET = join(root, "site", "src", "app", "loom-tokens.css");
const CHECK = process.argv.includes("--check");

/**
 * The shared regions, in the order they appear in the source and in the output.
 *
 * `why` is copied into the generated file's header, so a reader of that file
 * knows what each block is for without opening the app's stylesheet.
 */
const REGIONS = [
  {
    name: "tokens",
    why: "The custom variant, the concentric radius scale, and both palettes",
  },
  {
    name: "surfaces",
    why: "Glass surfaces: panel, panel-strong, pill, blob",
  },
  {
    name: "utilities",
    why: "Text and interaction helpers: text-soft, text-faint, hover-surface",
  },
  {
    name: "motion",
    why: "Keyframes, the animate-* classes, and the reduced-motion override",
  },
  {
    name: "controls",
    why: "Buttons, keycaps, chips, and the thinking panel",
  },
];

const fail = (message) => {
  console.error(`\nsync-site-tokens: ${message}\n`);
  process.exit(1);
};

if (!existsSync(SOURCE)) fail(`cannot find ${relative(root, SOURCE)}`);

// Normalise to LF: the source may be checked out with CRLF on Windows, and a
// generated file that changes line endings per machine would fail --check for
// no real reason.
const source = readFileSync(SOURCE, "utf8").replace(/\r\n/g, "\n");
const sourceLines = source.split("\n");

/** Every marker in the file, with its line number, for validation. */
const markers = [];
sourceLines.forEach((line, index) => {
  const match = line.match(/\/\*\s*loom-site:([a-z]+):(start|end)\s*\*\//);
  if (match) markers.push({ region: match[1], kind: match[2], line: index });
});

if (markers.length === 0) {
  fail(
    `no loom-site:*:start markers found in ${relative(root, SOURCE)}.\n` +
      `  Either the markers were removed, or this file is not the source.`,
  );
}

const extracted = [];
const seen = new Set();

for (const region of REGIONS) {
  const starts = markers.filter(
    (marker) => marker.region === region.name && marker.kind === "start",
  );
  const ends = markers.filter(
    (marker) => marker.region === region.name && marker.kind === "end",
  );

  if (starts.length !== 1 || ends.length !== 1) {
    fail(
      `region "${region.name}" has ${starts.length} start marker(s) and ` +
        `${ends.length} end marker(s); exactly one of each is required.\n` +
        `  Look for /* loom-site:${region.name}:start */ and ` +
        `/* loom-site:${region.name}:end */ in ${relative(root, SOURCE)}.`,
    );
  }

  const [start] = starts;
  const [end] = ends;
  if (start.line >= end.line) {
    fail(
      `region "${region.name}" is inverted: its start marker (line ${start.line + 1}) ` +
        `comes at or after its end marker (line ${end.line + 1}).`,
    );
  }

  for (const other of markers) {
    if (other.region === region.name) continue;
    if (other.line > start.line && other.line < end.line) {
      fail(
        `region "${region.name}" (lines ${start.line + 1}-${end.line + 1}) contains a ` +
          `marker for "${other.region}". Regions may not nest.`,
      );
    }
  }

  // The marker lines themselves are excluded; the payload is what sits between
  // them.
  const body = sourceLines
    .slice(start.line + 1, end.line)
    .join("\n")
    .replace(/^\n+/, "")
    .replace(/\n+$/, "");

  if (body.trim() === "") {
    fail(
      `region "${region.name}" is empty. An empty region would produce a site ` +
        `that builds fine and is missing its styles, so this is treated as an error.`,
    );
  }

  seen.add(region.name);
  extracted.push({ ...region, body, from: start.line + 2, to: end.line });
}

// A marker for a region this script does not know about is almost certainly a
// typo in the region name, which would silently drop a block of styles.
for (const marker of markers) {
  if (!seen.has(marker.region)) {
    fail(
      `unknown region "${marker.region}" at line ${marker.line + 1}. ` +
        `Known regions: ${REGIONS.map((region) => region.name).join(", ")}.`,
    );
  }
}

// Regions must be extracted in source order, or the generated file's rules
// would be reordered relative to the app's, and cascade order is part of the
// design.
const inSourceOrder = [...extracted].sort((a, b) => a.from - b.from);
for (let index = 0; index < inSourceOrder.length; index += 1) {
  if (inSourceOrder[index].name !== extracted[index].name) {
    fail(
      `regions are declared out of source order: the file has ` +
        `${inSourceOrder.map((region) => region.name).join(", ")} but the script ` +
        `expects ${extracted.map((region) => region.name).join(", ")}.`,
    );
  }
}

const payload = extracted
  .map((region) => `/* ${"=".repeat(72)}\n   ${region.name}: ${region.why}\n   ${"=".repeat(72)} */\n\n${region.body}`)
  .join("\n\n");

// The fingerprint makes staleness legible in a diff, and gives CI something to
// name in its failure message.
const fingerprint = createHash("sha256").update(payload).digest("hex").slice(0, 16);

const output = `/* ---------------------------------------------------------------------------
   GENERATED FILE — DO NOT EDIT

   Loom's design tokens, extracted verbatim from src/styles.css so the website
   and the desktop app are styled by the same declarations.

   To change anything here, change it in src/styles.css and run:

     bun run tokens          # from site/

   CI runs \`bun run tokens:check\` and fails when this file is stale, so the two
   cannot drift. The regions are marked in the source by comments of the form
   "loom-site:NAME:start" and "loom-site:NAME:end".

   (Written without the literal comment delimiters on purpose: CSS comments do
   not nest, so quoting a marker inside this header would terminate it early and
   spill the rest of the prose into the stylesheet.)

   source:      src/styles.css
   fingerprint: ${fingerprint}
   regions:     ${extracted
     .map((region) => `${region.name} (${region.from}-${region.to})`)
     .join(", ")}
--------------------------------------------------------------------------- */

${payload}
`;

if (CHECK) {
  const current = existsSync(TARGET) ? readFileSync(TARGET, "utf8") : null;

  if (current === null) {
    fail(
      `site/src/app/loom-tokens.css does not exist.\n` +
        `  Run: node scripts/sync-site-tokens.mjs`,
    );
  }

  if (current === output) {
    console.log(
      `token check: up to date (${extracted.length} regions, fingerprint ${fingerprint})`,
    );
    process.exit(0);
  }

  // Name the regions that actually moved, so the failure is actionable rather
  // than "the file differs".
  const currentFingerprint =
    current.match(/fingerprint:\s*([0-9a-f]+)/)?.[1] ?? "(none)";
  const currentRegions = current.match(/regions:\s*(.+)/)?.[1] ?? "(none)";

  console.error(
    `\nsync-site-tokens: generated file is stale.\n\n` +
      `  src/styles.css changed, but site/src/app/loom-tokens.css was not\n` +
      `  regenerated. Fix it with:\n\n` +
      `    node scripts/sync-site-tokens.mjs\n\n` +
      `  fingerprint now:     ${fingerprint}\n` +
      `  fingerprint in file: ${currentFingerprint}\n` +
      `  regions now:         ${extracted
        .map((region) => `${region.name} (${region.from}-${region.to})`)
        .join(", ")}\n` +
      `  regions in file:     ${currentRegions}\n`,
  );
  process.exit(1);
}

mkdirSync(dirname(TARGET), { recursive: true });

const previous = existsSync(TARGET) ? readFileSync(TARGET, "utf8") : null;
writeFileSync(TARGET, output);

if (previous === output) {
  console.log(
    `token sync: no change (${extracted.length} regions, fingerprint ${fingerprint})`,
  );
} else {
  const lines = output.split("\n").length;
  console.log(
    `token sync: wrote site/src/app/loom-tokens.css\n` +
      `  ${extracted.length} regions, ${lines} lines, fingerprint ${fingerprint}`,
  );
  for (const region of extracted) {
    console.log(
      `    ${region.name.padEnd(10)} src/styles.css:${region.from}-${region.to}  ` +
        `(${region.body.split("\n").length} lines)`,
    );
  }
}
