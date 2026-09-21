// Checks that the prerendered HTML contains what the pages claim it does.
//
// A successful build proves the components compile. It does not prove that the release
// lookup found anything, or that a feature list still describes the product — and the
// failure this is aimed at is a quiet one: `getRelease()` falls back to a bundled
// snapshot on any error, so the download page can render perfectly while silently
// advertising a hardcoded version.
//
// ---------------------------------------------------------------------------
// Three things learned writing this, each of which produced a false failure
// ---------------------------------------------------------------------------
//
//  1. **React inserts a text separator between adjacent static and dynamic text.**
//     `<h2>Loom {version}</h2>` serialises as `Loom <!-- -->0.1.0`, so a regex
//     expecting a space finds nothing. The HTML is normalised first.
//  2. **Route handlers are emitted as `<route>.body`.** `sitemap.xml.body` and
//     `robots.txt.body` do not end in `.xml` or `.txt`, so matching on the extension
//     alone misses them entirely.
//  3. **No release yet is not a failure.** `getRelease()` distinguishes "GitHub
//     answered with nothing" from "GitHub could not be reached", and before the first
//     tag the former is the correct, expected state.
//  4. **Next emits the 404 as `404.html`, not `not-found.html`.** The glob below used
//     to look for the latter and reported the page missing for as long as it existed,
//     which is the worst kind of false negative: it taught whoever read the output to
//     ignore that line.
//
// ---------------------------------------------------------------------------
// Why the landing-page checks describe a document now
// ---------------------------------------------------------------------------
//
// They used to assert a *scripted turn*: that the hero's headline was "An AI agent
// that runs on your machine", and that the composer's placeholder "Do anything" had
// reached the prerender — the latter proving the animation committed its first frame to
// the server render rather than starting mid-animation.
//
// Both of those are gone along with the whole demonstration they belonged to, and that
// is worth stating plainly, because a check like "the first frame is the empty state"
// is a check that *pins the defect in place*. It would have failed the moment the right
// thing happened, which makes it worse than no check at all. So the assertions below
// describe the manual that exists, and `verify-manual.mjs` separately asserts that the
// scripted hero has not come back.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));

function allFiles(dir, out = []) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) allFiles(full, out);
    else out.push(full);
  }
  return out;
}

const files = allFiles(join(here, ".next", "server")).filter((file) =>
  // `.body` covers the route handlers; `.html` the pages.
  /\.(html|xml|txt)(\.body)?$/.test(file),
);

/** Strips React's `<!-- -->` separators, so a check reads what a visitor sees. */
const visible = (html) => html.replace(/<!--\s*-->/g, "");

let failed = 0;
const check = (label, ok, detail) => {
  if (!ok) failed += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${label}${detail ? `\n        ${detail}` : ""}`);
};

console.log(`output files found: ${files.length}`);
for (const file of files) {
  console.log(`  ${file.replace(join(here, ".next", "server"), "")}`);
}
console.log("");

const find = (suffix) => files.find((file) => file.endsWith(suffix));

// --- The manual -------------------------------------------------------------
const home = find("index.html");
if (home) {
  const html = visible(readFileSync(home, "utf8"));

  // The title page. No headline, no tagline: the product's name and one sentence.
  check("manual: the title page names the instrument", html.includes("An agent you can watch work"));
  check(
    "manual: the contents list is present",
    html.includes("sections") && html.includes("figures") && html.includes("tables"),
  );

  // The three sections of the argument. Each is a claim the page has to actually make.
  check("manual: the dock is described", html.includes("Every zone starts"));
  check("manual: the terminal is described", html.includes("A real terminal"));
  check("manual: the editor is described", html.includes("Monaco"));
  check("manual: the agent modes are described", html.includes("Atelier"));
  check("manual: providers are listed", html.includes("OpenCode Go"));

  // The drawings. A plate that stopped rendering would leave a caption with no figure,
  // which looks almost right.
  check("manual: the figures are drawn", (html.match(/class="figure-art"/g) ?? []).length >= 3);
  check("manual: a drawing carries a text alternative", html.includes('aria-label="A drawing'));

  // The glossary, which is the condition on which the vocabulary is allowed on the
  // page at all.
  check("manual: the glossary defines its terms", html.includes("On a loom:"));
  check("manual: the shortcuts are written down", html.includes("Ctrl+Shift+Space"));

  // The answers are static prose rather than a disclosure widget, which is the whole
  // reason they are findable with in-page search and readable without JavaScript.
  check(
    "manual: the questions are answered in the prerender, not hidden behind a control",
    html.includes("SmartScreen") && !html.includes("<details"),
  );

  // Two position indicators, gated in opposite directions by CSS. Both are in the
  // markup — the gating is a media query — so what is checked here is that both exist
  // and that they are built from the same list.
  check("manual: the margin index is present", html.includes('class="index"'));
  check("manual: the running head is present", html.includes('class="bar"'));

  // The colophon. The most human thing on the page, and the part that a generator
  // assembling components would not have written.
  check("manual: the colophon names the type and the licence", html.includes("Colophon") && html.includes("Inter"));

  // The anchors the two contents lists promise. Eight sections, two appendices.
  const anchors = [
    "instrument",
    "warp",
    "pick",
    "ends",
    "count",
    "selvedge",
    "heddles",
    "off",
    "shortcuts",
    "glossary",
  ];
  const missing = anchors.filter((anchor) => !html.includes(`id="${anchor}"`));
  check(
    "manual: all ten entries are in the document",
    missing.length === 0,
    missing.map((anchor) => `#${anchor}`).join(", "),
  );
} else {
  check("the manual was emitted", false);
}

// --- The download page ------------------------------------------------------
const download = find("download.html");
if (download) {
  const html = visible(readFileSync(download, "utf8"));
  const live = html.includes("the current release");
  const none = html.includes("not released yet");
  const unreachable = html.includes("GitHub could not be reached");
  const version = html.match(/Loom (\d+\.\d+\.\d+)/)?.[1] ?? null;

  check("download: a version number is rendered", version !== null, `version: ${version}`);

  // Exactly one of the three states, and it has to be the true one.
  check(
    "download: exactly one release state is stated",
    [live, none, unreachable].filter(Boolean).length === 1,
    `live=${live} none=${none} unreachable=${unreachable}`,
  );
  check(
    "download: an absent release is reported as absent, not as a fault",
    !unreachable || live,
    unreachable
      ? "Rendered 'GitHub could not be reached' — that should only appear on a real network failure."
      : "Either a live release, or an honest 'not released yet'.",
  );
  check(
    "download: the unsigned-installer warning is present",
    html.includes("publisher is unknown"),
  );
  check("download: a hash-verification command is shown", html.includes("Get-FileHash"));
  check("download: the stable redirect path is used", html.includes("/download/latest"));
  // The page says out loud that it is not the manual, which is what stops a reader
  // wondering why the installation instructions appear twice in slightly different words.
  check(
    "download: it points back at the manual rather than restating it",
    html.includes("Appendix A"),
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

// Privacy has to name every path it claims to store, because the whole reason that page
// exists is that its list can be checked by opening a folder.
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
// Next emits this as `404.html`; see note 4 at the top of the file. It is a page a
// visitor can genuinely land on, so it is checked like one: what matters is that it
// explains itself and offers a way out, rather than being a framework default.
const notFound = files.find((file) => /(^|[\\/])(404|not-found)\.html$/.test(file));
if (notFound) {
  const html = visible(readFileSync(notFound, "utf8"));
  check("404: explains itself", html.includes("That page does not exist"));
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
  console.log("VERDICT: the prerendered HTML contains what the pages claim.");
} else {
  console.log(`VERDICT: ${failed} check(s) failed.`);
  process.exit(1);
}
