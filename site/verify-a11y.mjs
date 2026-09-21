// Structural and accessibility checks over the prerendered HTML.
//
// Not a replacement for testing with a screen reader. It catches the class of mistake that is easy to
// make and invisible in a browser: an image with no alt text, a link with no accessible name, two `<h1>`s
// on one page, a heading level that skips, a control that cannot be reached or operated.
//
// It reads the *built* HTML rather than the source, so it sees what a visitor and a crawler actually
// receive — including anything a component quietly failed to render.
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
  // The internal error pages are Next's markup, not this project's, so auditing them would mean auditing
  // a framework.
  .filter((file) => !file.includes("_global-error"));

let failed = 0;
const fail = (page, label, detail) => {
  failed += 1;
  console.log(`FAIL  ${page}: ${label}${detail ? `\n        ${detail}` : ""}`);
};
const pass = (page, label) => console.log(`PASS  ${page}: ${label}`);

/** Strips tags, so text content can be measured without attributes in the way. */
const textOf = (html) =>
  html
    .replace(/<script[\s\S]*?<\/script>/g, " ")
    .replace(/<style[\s\S]*?<\/style>/g, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&[a-z]+;/g, " ")
    .replace(/\s+/g, " ")
    .trim();

/** Removes the separators React inserts between adjacent text nodes. */
const visible = (html) => html.replace(/<!--\s*-->/g, "");

for (const file of pages) {
  const html = visible(readFileSync(file, "utf8"));
  const name = file.split(/[\\/]/).slice(-1)[0];

  // --- The document ------------------------------------------------------
  if (!/<html[^>]+lang="en"/.test(html)) fail(name, "no lang attribute on <html>");
  else pass(name, "html lang is set");

  if (!/<title>[^<]{10,}<\/title>/.test(html)) fail(name, "missing or empty <title>");
  else pass(name, "has a title");

  if (!/<meta name="description" content="[^"]{40,}"/.test(html)) {
    fail(name, "missing or too-short meta description");
  } else {
    pass(name, "has a meta description");
  }

  // --- Headings ----------------------------------------------------------
  const h1s = html.match(/<h1[\s>]/g) ?? [];
  if (h1s.length !== 1) fail(name, `expected exactly one <h1>, found ${h1s.length}`);
  else pass(name, "has exactly one h1");

  // Heading levels must not skip. An `h2` followed by an `h4` makes the outline nonsense for anyone
  // moving through the page by heading — and it is invisible to everyone else, which is why it is
  // checked here rather than left to review.
  const levels = [...html.matchAll(/<h([1-6])[\s>]/g)].map((match) => Number(match[1]));
  let skip = null;
  for (let index = 1; index < levels.length; index += 1) {
    if (levels[index] > levels[index - 1] + 1) {
      skip = `h${levels[index - 1]} → h${levels[index]}`;
      break;
    }
  }
  if (skip) fail(name, `heading levels skip (${skip})`);
  else pass(name, "heading levels do not skip");

  // --- Images ------------------------------------------------------------
  // The attribute is what matters, not its contents: `alt=""` is correct for a decorative image and an
  // absent `alt` is not.
  const images = [...html.matchAll(/<img\b[^>]*>/g)].map((match) => match[0]);
  const missingAlt = images.filter((tag) => !/\salt=/.test(tag));
  if (missingAlt.length > 0) {
    fail(name, `${missingAlt.length} image(s) without alt`, missingAlt[0].slice(0, 90));
  } else {
    pass(name, `every image has alt (${images.length} found)`);
  }

  // --- Links -------------------------------------------------------------
  // Every link needs text, an `aria-label`, or a nested image with alt. An icon link with none of those
  // is announced as nothing but "link".
  const anchors = [...html.matchAll(/<a\b([^>]*)>([\s\S]*?)<\/a>/g)];
  const nameless = anchors.filter(([, attrs, inner]) => {
    if (/aria-label=/.test(attrs)) return false;
    if (textOf(inner).length > 0) return false;
    if (/<img[^>]+alt="[^"]+"/.test(inner)) return false;
    return true;
  });
  if (nameless.length > 0) {
    fail(name, `${nameless.length} link(s) with no accessible name`, nameless[0][0].slice(0, 110));
  } else {
    pass(name, `every link is named (${anchors.length})`);
  }

  // --- Controls ----------------------------------------------------------
  // A `<summary>` is a control. With no text it needs an `aria-label`.
  const summaries = [...html.matchAll(/<summary\b([^>]*)>([\s\S]*?)<\/summary>/g)];
  const unnamed = summaries.filter(
    ([, attrs, inner]) => !/aria-label=/.test(attrs) && textOf(inner).length === 0,
  );
  if (unnamed.length > 0) fail(name, `${unnamed.length} <summary> without an accessible name`);
  else pass(name, `every summary is named (${summaries.length})`);

  // --- The artwork --------------------------------------------------------
  /*
   * Every `<svg>` needs a decision made about it: either it carries meaning and has a label, or it is
   * decoration and is hidden. An unlabelled, unhidden SVG is announced by a screen reader as an empty
   * "graphic", which tells a reader that something is there and nothing about what.
   *
   * On this page every figure is decoration — the prose says everything they say — so the correct state
   * is `aria-hidden="true"` on all of them, and that is what is asserted. A figure that later gains a
   * `role="img"` and a label would fail this, which is the right direction: it would be a deliberate
   * change, and the check should be edited when the decision is made.
   */
  const svgs = [...html.matchAll(/<svg\b([^>]*)>/g)].map((match) => match[1]);
  const unhandled = svgs.filter(
    (attrs) => !/aria-hidden="true"/.test(attrs) && !/role="img"/.test(attrs),
  );
  if (unhandled.length > 0) {
    fail(
      name,
      `${unhandled.length} svg(s) are neither labelled nor hidden`,
      unhandled[0].slice(0, 110),
    );
  } else {
    pass(name, `every svg is labelled or hidden (${svgs.length})`);
  }

  // --- Substance ---------------------------------------------------------
  const words = textOf(html).split(/\s+/).filter(Boolean).length;
  if (words < 50) fail(name, `only ${words} words of text content`);
  else pass(name, `has real content (${words} words)`);

  console.log("");
}

// --- Cross-page checks on the product page ---------------------------------
const index = pages.find((file) => file.endsWith("index.html"));
if (index) {
  const html = visible(readFileSync(index, "utf8"));

  /*
   * At most one `<details>`: the small-screen nav menu, and nothing else.
   *
   * Asserted as a ceiling rather than a floor because a disclosure is the one pattern this page has had
   * to argue itself out of twice. The questions were briefly an accordion, and a collapsible answer is
   * hidden from in-page search and from anyone reading without JavaScript — which, for a question that
   * decides whether someone installs the software, is exactly the wrong trade.
   */
  const details = (html.match(/<details/g) ?? []).length;
  if (details > 1) {
    fail("index", `found ${details} <details>; only the nav menu needs to be one`);
  } else {
    pass("index", "nothing but the small-screen menu is behind a disclosure");
  }

  /*
   * Every in-page anchor needs a matching id, or a nav link silently does nothing when clicked — which
   * is worse than a 404, because it looks like the browser misbehaving rather than a wrong address.
   *
   * The pattern allows a leading `/`, because Next's `<Link>` renders `href="/#parts"` rather than
   * `href="#parts"`. Matching only the bare form finds a handful of anchors and passes anyway, which is
   * the worst kind of green: a check that succeeds because it is looking in the wrong place.
   *
   * The floor is six — one per movement — which is derived rather than guessed. An earlier version of
   * this check asserted a number above the true count and failed on a correct page, and the fix was to
   * notice the page was missing links rather than to lower the number.
   */
  const anchors = [...html.matchAll(/href="\/?#([a-z0-9-]+)"/g)].map((m) => m[1]);
  const unique = [...new Set(anchors)];
  const ids = new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]));
  const dangling = unique.filter((anchor) => !ids.has(anchor));

  if (unique.length < 6) {
    fail("index", `only found ${unique.length} in-page link(s); expected at least 6`);
  } else if (dangling.length > 0) {
    fail("index", `in-page links with no target: ${dangling.join(", ")}`);
  } else {
    pass("index", `every in-page link resolves (${unique.length})`);
  }
}

console.log("");
if (failed === 0) {
  console.log("VERDICT: the prerendered pages are structured and labelled.");
} else {
  console.log(`VERDICT: ${failed} check(s) failed.`);
  process.exit(1);
}
