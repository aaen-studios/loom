// Checks that the prerendered HTML contains what the pages claim it does.
//
// A successful build proves the components compile. It does not prove that the release lookup found
// anything, or that a page still describes the product — and the failure this is aimed at is a quiet
// one: `getRelease()` falls back to a bundled snapshot on any error, so the download page can render
// perfectly while silently advertising a hardcoded version.
//
// ---------------------------------------------------------------------------
// Lessons, each of which cost a debugging cycle
// ---------------------------------------------------------------------------
//
//  1. **React inserts a text separator between adjacent static and dynamic text.** `<h2>Loom
//     {version}</h2>` serialises as `Loom <!-- -->0.1.0`, so a regex expecting a space finds nothing.
//     The HTML is normalised first.
//  2. **Route handlers are emitted as `<route>.body`.** `sitemap.xml.body` and `robots.txt.body` do not
//     end in `.xml` or `.txt`, so matching on the extension alone misses them entirely.
//  3. **No release yet is not a failure.** `getRelease()` distinguishes "GitHub answered with nothing"
//     from "GitHub could not be reached", and before the first tag the former is the correct state.
//  4. **Next emits the 404 as `404.html`, not `not-found.html`.** An earlier version of this file looked
//     for the latter and reported the page missing for as long as it existed — the worst kind of false
//     negative, because it teaches whoever reads the output to ignore that line.
//  5. **A release state cannot be detected by searching the prose.** The three states carry three
//     sentences, and one of those sentences occurs elsewhere on the page in ordinary words, so a prose
//     search reports two states at once on a page that rendered one. The panel declares the state as
//     `data-release-state` and this reads the attribute.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));

/**
 * The page's movements, read out of `src/lib/movements.ts` as text.
 *
 * Read rather than imported, for the same reason `verify-tokens.mjs` reads the stylesheet as text: this
 * project's build step has to stay dependency-free so CI can run it with nothing but Node, and importing
 * a TypeScript module would need a loader for the sake of six ids. The pattern is strict enough that a
 * malformed entry does not match at all, and the ids are cross-checked against the rendered anchors
 * below, so a missed entry cannot pass as a shorter list.
 */
const movementsSource = readFileSync(join(here, "src", "lib", "movements.ts"), "utf8");
const MOVEMENT_IDS = [...movementsSource.matchAll(/\{\s*id:\s*"([a-z-]+)"/g)].map((m) => m[1]);

function allFiles(dir, out = []) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) allFiles(full, out);
    else out.push(full);
  }
  return out;
}

const files = allFiles(join(here, ".next", "server")).filter((file) =>
  /\.(html|xml|txt)(\.body)?$/.test(file),
);

/** Strips React's `<!-- -->` separators, so a check reads what a visitor sees. */
const visible = (html) => html.replace(/<!--\s*-->/g, "");

let failed = 0;
const check = (label, ok, detail) => {
  if (!ok) failed += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${label}${!ok && detail ? `\n        ${detail}` : ""}`);
};

console.log(`output files found: ${files.length}`);
for (const file of files) {
  console.log(`  ${file.replace(join(here, ".next", "server"), "")}`);
}
console.log(`movements declared: ${MOVEMENT_IDS.join(", ")}`);
console.log("");

const find = (suffix) => files.find((file) => file.endsWith(suffix));

// --- The product page -------------------------------------------------------
const home = find("index.html");
if (home) {
  const html = visible(readFileSync(home, "utf8"));

  // The headline and the standfirst. The claim is five words on purpose, and this is what holds it at
  // five.
  check("home: the headline is the claim", html.includes("A window that holds everything"));
  check(
    "home: the standfirst says what the thing is",
    html.includes("desktop workspace for AI chat and agents"),
  );

  /*
   * The figures.
   *
   * This is the check that matters most for this page, and it is here rather than in the token verifier
   * because it is about the *document*: the artwork is generated at render time from a seed, so a
   * generator that throws or returns nothing during the server render produces a page that builds,
   * ships, and has no pictures in it. Counting the SVGs and the paths with real geometry is the only way
   * to know the server drew them rather than leaving them to the client.
   */
  const figures = (html.match(/data-figure="/g) ?? []).length;
  check("home: the figures are drawn on the server", figures >= 6, `found ${figures}`);
  check(
    "home: the prerendered paths have geometry",
    (html.match(/<path[^>]*\sd="M /g) ?? []).length >= 20,
  );
  for (const kind of ["field", "lattice", "rings", "bundle"]) {
    check(`home: a ${kind} figure is present`, html.includes(`data-figure="${kind}"`));
  }

  // The product. Every one of these is a claim the page has to actually make rather than gesture at.
  check("home: the terminal is described", html.includes("real pty") || html.includes("A real pty"));
  check("home: the editor is described", html.includes("Monaco"));
  check("home: diffs are described as diffs", html.includes("exact diff"));
  check("home: git is described as your own git", html.includes("your own git"));
  check("home: the autosave hash guard is described", html.includes("SHA-256"));
  check("home: the four agent modes are named", html.includes("Atelier") && html.includes("Review"));
  check("home: the permission levels are stated", html.includes("Auto read-only"));
  check("home: providers are listed", html.includes("OpenCode Go"));
  check("home: local models are covered", html.includes("LM Studio") || html.includes("Ollama"));
  check("home: the question about code signing is answered", html.includes("SmartScreen"));
  check("home: the Windows-only limitation is stated", html.includes("macOS or Linux"));
  check("home: the source is linked", html.includes("github.com/aaen-studios/loom"));

  // The answers are static prose rather than a collapsed widget, which is the whole reason they are
  // findable with in-page search and readable without JavaScript.
  check(
    "home: the questions are not hidden behind a disclosure",
    (html.match(/<details/g) ?? []).length <= 1,
    "the only <details> should be the small-screen nav menu",
  );

  // Every movement in the page's own list is rendered, and reachable by a link.
  const missing = MOVEMENT_IDS.filter((id) => !html.includes(`id="${id}"`));
  check(
    "home: every movement in the page's list is rendered",
    missing.length === 0,
    missing.map((id) => `#${id}`).join(", "),
  );

  // The failure that produced `lib/movements.ts`: the footer used to hardcode one anchor and the nav
  // three, leaving movements that could only be reached by scrolling.
  const linked = [...html.matchAll(/href="\/#([a-z-]+)"/g)].map((m) => m[1]);
  const unreachable = MOVEMENT_IDS.filter((id) => !linked.includes(id));
  check(
    "home: every movement is reachable by a link",
    unreachable.length === 0,
    unreachable.map((id) => `#${id}`).join(", "),
  );

  /*
   * And the numbering opens at one.
   *
   * This is the check for a bug that was invisible to every other test on this page. The hero does not
   * use the `Movement` primitive — it has the `h1`, the display-size heading and the full-bleed figure,
   * so it builds its own section — and for several revisions it wrote its own eyebrow too, using the
   * platform line where the other five movements put an index and a word.
   *
   * The result was a page whose movements ran **02, 03, 04, 05, 06** with no `01` anywhere. Nothing
   * failed: every anchor resolved, every heading was present, the outline was valid, and the page was
   * entirely self-consistent. A sequence that opens at two only looks wrong if you are counting — and
   * nothing here was.
   *
   * Matching on `class="t-index"` and the digits together, rather than on the digits alone: `01`
   * appears in version numbers, in hashes and inside path data, and a check that matches those would
   * pass on a page with no indices at all.
   */
  const indices = MOVEMENT_IDS.map((_, i) => String(i + 1).padStart(2, "0"));

  /*
   * Read out of the movement eyebrows specifically — the `<p class="movement-index">` blocks — and not
   * out of the document at large.
   *
   * The first version of this check searched the whole page for `class="t-index"` and the digits, and it
   * would have passed on a page with no eyebrows at all. The panel cards use the same class for their
   * numbers (`01`–`08`), and so do the modes and the questions, so searching the document finds every
   * index the assertion is looking for whether or not the thing that names the *movements* is there. A
   * check that cannot fail is worse than no check, because it reads as coverage.
   *
   * Scoped to the eyebrows, the expectation is exact: one per movement, in order, starting at 01. The
   * hero carries one too, which is the whole point — it builds its own section rather than using the
   * `Movement` primitive, and that is how it came to be missing.
   */
  const eyebrows = [...html.matchAll(/class="movement-index"[^>]*>([\s\S]*?)<\/p>/g)].map(
    (match) => match[1].match(/class="t-index"[^>]*>\s*(\d\d)\s*</)?.[1] ?? "??",
  );

  check(
    "home: every movement's eyebrow is numbered, in order, from 01",
    eyebrows.join(",") === indices.join(","),
    eyebrows.length === 0
      ? "no movement eyebrows at all — the hero is writing its own again"
      : `rendered ${eyebrows.join(", ")}; expected ${indices.join(", ")}`,
  );

  /*
   * The publisher's link, which goes to the studio's own site rather than to its repository.
   *
   * Asserted rather than assumed because it is the one outbound link on the page that does not go to
   * GitHub, and there are two things that could quietly break it: someone "tidying up" the constant to
   * match the organisation's GitHub URL, or the colophon being rewritten to spell the URL out inline —
   * which would then drift from the `author` metadata that reads the same constant.
   *
   * The negative assertion matters as much as the positive one: `github.com/aaen-studios` still appears
   * legitimately on this page, because that is where the source *is*. What must not appear is the
   * organisation *root* as the publisher link, which is what the constant used to hold.
   */
  check(
    "home: the publisher links to the studio's own site",
    html.includes('href="https://aaenz.no"'),
    "the Studio link in the colophon is not pointing at aaenz.no",
  );
  check(
    "home: the publisher link is not the GitHub organisation",
    !html.includes('href="https://github.com/aaen-studios"'),
  );
} else {
  check("the product page was emitted", false);
}

// --- The download page ------------------------------------------------------
const download = find("download.html");
if (download) {
  const html = visible(readFileSync(download, "utf8"));
  const version = html.match(/Loom (\d+\.\d+\.\d+)/)?.[1] ?? null;
  check("download: a version number is rendered", version !== null, `version: ${version}`);

  const state = html.match(/data-release-state="(live|none|unreachable)"/)?.[1] ?? null;
  check(
    "download: the release state is declared in the markup",
    state !== null,
    "no data-release-state attribute found",
  );

  const LABEL = {
    live: "the current release",
    none: "not released yet",
    unreachable: "GitHub could not be reached",
  };

  if (state && LABEL[state]) {
    const present = html.includes(LABEL[state]);
    check(
      "download: the declared state is the one rendered",
      present,
      `state is "${state}", whose label is "${LABEL[state]}" — not found in the page`,
    );

    const contradictory = Object.entries(LABEL)
      .filter(([key]) => key !== state)
      .filter(([, label]) => html.includes(label))
      .map(([key]) => key);

    check(
      "download: no other release state's wording appears",
      contradictory.length === 0,
      contradictory.length === 0 ? "" : `also rendered: ${contradictory.join(", ")}`,
    );
  }

  check(
    "download: an absent release is reported as absent, not as a fault",
    state !== "unreachable",
    "Rendered the unreachable state — that should only appear on a real network failure.",
  );
  check(
    "download: the unsigned-installer warning is present",
    html.includes("publisher is unknown"),
  );
  check("download: a hash-verification command is shown", html.includes("Get-FileHash"));
  check("download: the stable redirect path is used", html.includes("/download/latest"));
  // The page says out loud that it is a separate document, which is what stops a reader wondering why
  // the installation instructions appear twice in different words.
  check(
    "download: it points back at the front page rather than restating it",
    html.includes("/#parts") && html.includes("/#install"),
  );
} else {
  check("the download page was emitted", false);
}

// --- The legal pages --------------------------------------------------------
for (const [file, label, marker] of [
  ["privacy.html", "privacy", "Windows Credential Manager"],
  ["terms.html", "terms", "MIT licence"],
]) {
  const page = find(file);
  if (!page) {
    check(`the ${label} page was emitted`, false);
    continue;
  }
  const html = visible(readFileSync(page, "utf8"));
  check(`${label}: has substantive content`, html.includes(marker));
}

// Privacy has to name every path it claims to store, because the whole reason that page exists is that
// its list can be checked by opening a folder.
const privacy = find("privacy.html");
if (privacy) {
  const html = visible(readFileSync(privacy, "utf8"));
  const paths = ["~/.loom", "loom.db", "config.json", "attachments", "logs", "backups"];
  const missing = paths.filter((path) => !html.includes(path));
  check(
    "privacy: every storage path it relies on is named",
    missing.length === 0,
    missing.join(", "),
  );
  check(
    "privacy: it states the telemetry claim in the negative",
    html.includes("There is none, and it is not a setting"),
  );
}

// --- The 404 ----------------------------------------------------------------
// Next emits this as `404.html`; see note 4 at the top of the file. It is a page a visitor can genuinely
// land on, so it is checked like one: what matters is that it explains itself and offers a way out,
// rather than being a framework default.
const notFound = files.find((file) => /(^|[\\/])(404|not-found)\.html$/.test(file));
if (notFound) {
  const html = visible(readFileSync(notFound, "utf8"));
  check("404: explains itself", html.includes("thread goes nowhere"));
  check("404: offers a way home", html.includes('href="/"'));
} else {
  check("the 404 page was emitted", false);
}

// --- Crawlability -----------------------------------------------------------
const sitemap = find("sitemap.xml.body");
if (sitemap) {
  const xml = readFileSync(sitemap, "utf8");
  check("sitemap: lists the four real pages", (xml.match(/<url>/g) ?? []).length === 4);
  check("sitemap: omits the redirect endpoint", !xml.includes("/download/latest"));
} else {
  check("the sitemap was emitted", false);
}

const robots = find("robots.txt.body");
if (robots) {
  const text = readFileSync(robots, "utf8");
  check("robots: disallows the redirect endpoint", text.includes("Disallow: /download/latest"));
  check("robots: points at the sitemap", text.includes("sitemap.xml"));
} else {
  check("robots.txt was emitted", false);
}

console.log("");
if (failed === 0) {
  console.log("VERDICT: the prerendered HTML contains what the pages claim, and the artwork is");
  console.log("in it rather than arriving late.");
} else {
  console.log(`VERDICT: ${failed} check(s) failed.`);
  process.exit(1);
}
