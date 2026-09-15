// Fails the build when a source file carries a UTF-8 BOM.
//
// PowerShell's `Set-Content -Encoding UTF8` writes one on Windows 5.1, and a
// BOM in package.json breaks Vite's PostCSS config lookup. CI runs this so it
// cannot come back.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { dirname } from "node:path";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const extensions = [
  ".json", ".ts", ".tsx", ".js", ".mjs", ".css", ".html",
  ".rs", ".toml", ".yml", ".yaml", ".md", ".nsi",
];
const skip = new Set(["node_modules", "target", "dist", ".git", "gen"]);

const offenders = [];

function walk(directory) {
  for (const entry of readdirSync(directory)) {
    if (skip.has(entry)) continue;
    const full = join(directory, entry);
    if (statSync(full).isDirectory()) {
      walk(full);
      continue;
    }
    const extension = full.slice(full.lastIndexOf("."));
    if (!extensions.includes(extension)) continue;

    const head = readFileSync(full).subarray(0, 3);
    if (head[0] === 0xef && head[1] === 0xbb && head[2] === 0xbf) {
      offenders.push(relative(root, full));
    }
  }
}

walk(root);

if (offenders.length > 0) {
  console.error("UTF-8 BOM found in:\n" + offenders.map((f) => `  ${f}`).join("\n"));
  process.exit(1);
}
console.log("encoding check: no BOMs");
