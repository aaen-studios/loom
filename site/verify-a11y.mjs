// Accessibility and structural checks over the prerendered HTML.
//
// Not a substitute for testing with a screen reader, but it catches the class of
// mistake that is easy to make and invisible in a browser: an image without alt
// text, a link with no accessible name, two `<h1>`s, a skipped heading level, a
// control that cannot be reached or operated.
//
// It reads the built HTML rather than the source, so it sees what a visitor and
// a crawler actually receive — including anything a component failed to render.
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

const pages = allFiles(join(here, ".next", "server", "app"))
  .filter((file) => file.endsWith(".html"))
  // The internal error pages are Next's own markup, not ours to audit.
  .filter((file) => !file.includes("_global-error"));

let failed = 0;
const fail = (page, label, detail) => {
  failed += 1;
  console.log(`FAIL  ${page}: ${label}${detail ? `\n        ${detail}` : ""}`);
};
const pass = (page, label) => console.log(`PASS  ${page}: ${label}`);

/** Strips tags so text content can be measured without attributes in the way. */
const textOf = (html) =>
  html
    .replace(/<script[\s\S]*?<\/script>/g, " ")
    .replace(/<style[\s\S]*?<\/style>/g, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&[a-z]+;/g, " ")
    .replace(/\s+/g, " ")
    .trim();

/** Removes the `<!-- -->` separators React inserts between text nodes. */
const visible = (html) => html.replace(/<!--\s*-->/g, "");

for (const file of pages) {
  const raw = readFileSync(file, "utf8");
  const html = visible(raw);
  const name = file.split(/[\\/]/).slice(-1)[0];

  // --- document basics ---------------------------------------------------
  if (!/<html[^>]+lang="en"/.test(html)) fail(name, "html has no lang attribute");
  else pass(name, "html lang is set");

  if (!/<title>[^<]{10,}<\/title>/.test(html)) fail(name, "missing or empty <title>");
  else pass(name, "has a title");

  if (!/<meta name="description" content="[^"]{40,}"/.test(html)) {
    fail(name, "missing or too-short meta description");
  } else {
    pass(name, "has a meta description");
  }

  // --- headings ----------------------------------------------------------
  const h1s = html.match(/<h1[\s>]/g) ?? [];
  if (h1s.length !== 1) {
    fail(name, `expected exactly one <h1>, found ${h1s.length}`);
  } else {
    pass(name, "has exactly one h1");
  }

  // Heading levels must not skip: an h2 followed by an h4 makes the outline
  // nonsense for anyone navigating by heading.
  const levels = [...html.matchAll(/<h([1-6])[\s>]/g)].map((match) =>
    Number(match[1]),
  );
  let skip = null;
  for (let index = 1; index < levels.length; index += 1) {
    if (levels[index] > levels[index - 1] + 1) {
      skip = `h${levels[index - 1]} → h${levels[index]}`;
      break;
    }
  }
  if (skip) fail(name, `heading level skips (${skip})`);
  else pass(name, "heading levels do not skip");

  // --- images ------------------------------------------------------------
  // `alt` is required; `alt=""` is valid for decorative images, so the check is
  // for the attribute's presence, not its content.
  const images = [...html.matchAll(/<img\b[^>]*>/g)].map((match) => match[0]);
  const missingAlt = images.filter((tag) => !/\salt=/.test(tag));
  if (missingAlt.length > 0) {
    fail(name, `${missingAlt.length} image(s) without alt`, missingAlt[0].slice(0, 90));
  } else {
    pass(name, `images all have alt (${images.length})`);
  }

  // --- links and buttons -------------------------------------------------
  // Every link needs text, an aria-label, or a nested image with alt — an icon
  // link with none of those is announced as just "link".
  const anchors = [...html.matchAll(/<a\b([^>]*)>([\s\S]*?)<\/a>/g)];
  const nameless = anchors.filter(([, attrs, inner]) => {
    if (/aria-label=/.test(attrs)) return false;
    if (textOf(inner).length > 0) return false;
    // An SVG inside is fine only if it is labelled or hidden and there is a
    // title/label somewhere; treat an unlabelled svg-only link as nameless.
    if (/<img[^>]+alt="[^"]+"/.test(inner)) return false;
    return true;
  });
  if (nameless.length > 0) {
    fail(
      name,
      `${nameless.length} link(s) with no accessible name`,
      nameless[0][0].slice(0, 110),
    );
  } else {
    pass(name, `all links are named (${anchors.length})`);
  }

  // --- controls ----------------------------------------------------------
  // A `<summary>` is a control; if it has no text it needs an aria-label.
  const summaries = [...html.matchAll(/<summary\b([^>]*)>([\s\S]*?)<\/summary>/g)];
  const badSummary = summaries.filter(
    ([, attrs, inner]) => !/aria-label=/.test(attrs) && textOf(inner).length === 0,
  );
  if (badSummary.length > 0) {
    fail(name, `${badSummary.length} <summary> without an accessible name`);
  } else {
    pass(name, `all summaries are named (${summaries.length})`);
  }

  // --- content -----------------------------------------------------------
  const words = textOf(html).split(/\s+/).filter(Boolean).length;
  if (words < 50) fail(name, `only ${words} words of text content`);
  else pass(name, `has real content (${words} words)`);

  console.log("");
}

// --- cross-page checks -----------------------------------------------------
const index = pages.find((file) => file.endsWith("index.html"));
if (index) {
  const html = visible(readFileSync(index, "utf8"));

  // The small-screen menu must be a `details` element rather than a scripted
  // button, or the site's navigation would be the one part of it that needs
  // JavaScript. This asserts the approach rather than the appearance.
  if (!/<details/.test(html)) {
    fail("index", "no <details> menu — small-screen navigation would need JS");
  } else {
    pass("index", "small-screen navigation works without JavaScript");
  }

  // Every in-page anchor must have a matching id, or a nav link silently does
  // nothing on click — which is worse than a 404, because it looks like a bug
  // in the browser rather than a wrong address.
  //
  // The pattern accepts an optional leading `/`, because Next's `<Link>` renders
  // `href="/#features"` rather than `href="#features"`. Matching only the bare
  // form found a single anchor out of eight and passed anyway, which is the
  // worst kind of green: a check that succeeds because it is looking in the
  // wrong place.
  const anchors = [...html.matchAll(/href="\/?#([a-z0-9-]+)"/g)].map((m) => m[1]);
  const unique = [...new Set(anchors)];
  const ids = new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]));
  const dangling = unique.filter((anchor) => !ids.has(anchor));

  if (unique.length < 4) {
    // The nav has four section links. Finding fewer means the check is not
    // seeing them, not that they are absent — and a check that silently stops
    // looking is worse than one that fails.
    fail("index", `only found ${unique.length} in-page link(s); expected 4+`);
  } else if (dangling.length > 0) {
    fail("index", `in-page links with no target: ${dangling.join(", ")}`);
  } else {
    pass("index", `every in-page link resolves (${unique.length})`);
  }

  // The FAQ is collapsed by default; a keyboard user must be able to open it,
  // which `<details>` gives for free. Asserted so a future refactor to a
  // scripted accordion has to think about it.
  const details = (html.match(/<details/g) ?? []).length;
  if (details === 0) fail("index", "no FAQ <details> found");
  else pass("index", `FAQ uses native disclosure (${details} items)`);
}

console.log("");
if (failed === 0) {
  console.log("VERDICT: the prerendered pages are structured and labelled.");
} else {
  console.log(`VERDICT: ${failed} check(s) failed.`);
  process.exit(1);
}
