// Checks that the prerendered HTML contains what the pages claim it does.
//
// A successful build proves the components compile. It does not prove that the
// release lookup found anything, or that a feature list still describes the
// product — and the failure this is aimed at is a quiet one: `getRelease()`
// falls back to a bundled snapshot on any error, so the download page can render
// perfectly while silently advertising a hardcoded version.
//
// Three things learned writing this, each of which produced a false failure
// before it was understood:
//
//  1. **React inserts a text separator between adjacent static and dynamic
//     text.** `<h2>Loom {version}</h2>` serialises as `Loom <!-- -->0.1.0`, so a
//     regex expecting a space finds nothing. The HTML is normalised first.
//  2. **Route handlers are emitted as `<route>.body`.** `sitemap.xml.body` and
//     `robots.txt.body` do not end in `.xml` or `.txt`, so matching on the
//     extension alone misses them entirely.
//  3. **No release yet is not a failure.** `getRelease()` distinguishes "GitHub
//     answered with nothing" from "GitHub could not be reached", and before the
//     first tag the former is the correct, expected state.
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

// --- The landing page -------------------------------------------------------
const home = find("index.html");
if (home) {
  const html = visible(readFileSync(home, "utf8"));
  check("landing: the hero heading is present", html.includes("An AI agent that runs on"));
  // The composer's placeholder. It proves the scene committed its *first* frame
  // to the server render rather than starting mid-animation.
  check("landing: the first frame is the empty state", html.includes("Do anything"));
  check("landing: the agent-modes feature is described", html.includes("Four agent modes"));
  check("landing: providers are listed", html.includes("OpenCode Go"));
  // A `<details>` FAQ is readable with JavaScript disabled and searchable with
  // in-page find; a scripted accordion is neither.
  check("landing: the FAQ renders without JavaScript", html.includes("<details"));
  check("landing: the app's own glass is applied", html.includes("panel-strong"));
  check("landing: long-lived commands are described", html.includes("outlive the turn"));
} else {
  check("the landing page was emitted", false);
}

// --- The download page ------------------------------------------------------
const download = find("download.html");
if (download) {
  const html = visible(readFileSync(download, "utf8"));
  const live = html.includes("latest release");
  const none = html.includes("not released yet");
  const unreachable = html.includes("could not reach GitHub");
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
      ? "Rendered 'could not reach GitHub' — that should only appear on a real network failure."
      : "Either a live release, or an honest 'not released yet'.",
  );
  check("download: the unsigned-installer warning is present", html.includes("publisher is unknown"));
  check("download: a hash-verification command is shown", html.includes("Get-FileHash"));
  check("download: the stable redirect path is used", html.includes("/download/latest"));
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
