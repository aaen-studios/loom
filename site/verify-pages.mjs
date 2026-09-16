// TEMPORARY: checks the prerendered HTML actually contains what the pages claim.
//
// A build that succeeds proves the components compile, not that the release
// lookup found anything. The failure worth catching is subtle: `getRelease()`
// falls back to a bundled snapshot on any error, so a download page can render
// perfectly while silently showing a hardcoded version.
//
// Three things learned writing this, all of which produced false failures
// before they were understood:
//
//  1. **React inserts a text separator between adjacent static and dynamic
//     text.** `<h2>Loom {version}</h2>` renders as `Loom <!-- -->0.1.0`, so a
//     regex expecting a space finds nothing. The HTML is normalised first.
//  2. **Route handlers are written as `<route>.body`.** `sitemap.xml.body` and
//     `robots.txt.body` do not end in `.xml` or `.txt`, so a suffix match on
//     the extension misses them.
//  3. **An absent release is not a failure.** `getRelease()` reports whether
//     GitHub answered with nothing, or could not be reached — and before the
//     first tag, "nothing" is the correct, expected state.
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
  // `.body` covers the route handlers (sitemap, robots); `.html` the pages.
  /\.(html|xml|txt)(\.body)?$/.test(file),
);

/**
 * Removes the `<!-- -->` separators React emits between static and dynamic
 * text, so a check can assert what a reader sees rather than what the
 * serializer produced.
 */
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

// --- the landing page ------------------------------------------------------
const home = files.find((f) => f.endsWith("index.html"));
if (home) {
  const html = visible(readFileSync(home, "utf8"));
  check("landing: hero heading", html.includes("An AI agent that runs on"));
  check("landing: hero prompt types in (starts empty, not JS)", html.includes("Do anything"));
  check("landing: feature section present", html.includes("Four agent modes"));
  check("landing: providers listed", html.includes("OpenCode Go"));
  check("landing: FAQ rendered without JS", html.includes("<details"));
  check("landing: the app's glass utility is applied", html.includes("panel-strong"));
  check("landing: short-command feature described", html.includes("outlive the turn"));
} else {
  check("landing page found", false);
}

// --- the download page -----------------------------------------------------
const download = files.find((f) => f.endsWith("download.html"));
if (download) {
  const html = visible(readFileSync(download, "utf8"));
  const live = html.includes("latest release");
  const none = html.includes("not released yet");
  const unreachable = html.includes("could not reach GitHub");
  const version = html.match(/Loom (\d+\.\d+\.\d+)/)?.[1] ?? null;

  check("download: a version number is rendered", version !== null, `version: ${version}`);

  // Exactly one state must be reported, and it must be truthful about which.
  check(
    "download: exactly one release state is stated",
    [live, none, unreachable].filter(Boolean).length === 1,
    `live=${live} none=${none} unreachable=${unreachable}`,
  );
  check(
    "download: no release yet is reported as such, not as a fault",
    !unreachable || live,
    unreachable
      ? "Reported 'could not reach GitHub' — that should only appear on a real network failure."
      : "Either a live release or an honest 'not released yet'.",
  );
  check("download: unsigned-installer warning present", html.includes("publisher is unknown"));
  check("download: hash verification command shown", html.includes("Get-FileHash"));
  check("download: stable redirect path used", html.includes("/download/latest"));
} else {
  check("download page found", false);
}

// --- legal pages -----------------------------------------------------------
for (const [file, label, marker] of [
  ["privacy.html", "privacy", "Windows Credential Manager"],
  ["terms.html", "terms", "MIT licence"],
]) {
  const page = files.find((f) => f.endsWith(file));
  if (!page) {
    check(`${label} page found`, false);
    continue;
  }
  const html = visible(readFileSync(page, "utf8"));
  check(`${label}: substantive content`, html.includes(marker));
}

// --- crawlability ----------------------------------------------------------
const sitemap = files.find((f) => f.endsWith("sitemap.xml.body"));
if (sitemap) {
  const xml = readFileSync(sitemap, "utf8");
  check("sitemap: lists the four real pages", (xml.match(/<url>/g) ?? []).length === 4);
  check("sitemap: omits the redirect endpoint", !xml.includes("/download/latest"));
} else {
  check("sitemap found", false);
}

const robots = files.find((f) => f.endsWith("robots.txt.body"));
if (robots) {
  const text = readFileSync(robots, "utf8");
  check("robots: disallows the redirect endpoint", text.includes("Disallow: /download/latest"));
  check("robots: points at the sitemap", text.includes("sitemap.xml"));
} else {
  check("robots.txt found", false);
}

console.log("");
if (failed === 0) {
  console.log("VERDICT: the prerendered HTML contains what the pages claim.");
} else {
  console.log(`VERDICT: ${failed} check(s) failed.`);
  process.exit(1);
}
