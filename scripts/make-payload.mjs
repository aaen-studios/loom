// Builds setup/src-tauri/payload.zip from a built Loom app directory.
//
// Usage: node scripts/make-payload.mjs [sourceDir] [outFile]
// Defaults: target/release -> setup/src-tauri/payload.zip
import { execFileSync } from "node:child_process";
import { existsSync, rmSync, mkdirSync, copyFileSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const source = process.argv[2] ?? join(root, "target", "release");
const outFile = process.argv[3] ?? join(root, "setup", "src-tauri", "payload.zip");

const exe = join(source, "loom.exe");
if (!existsSync(exe)) {
  console.error(`loom.exe not found in ${source}. Run "bun run tauri build --no-bundle" first.`);
  process.exit(1);
}

const staging = join(root, "target", "payload-staging");
rmSync(staging, { recursive: true, force: true });
mkdirSync(staging, { recursive: true });

// Copy the exe plus any sidecar resources the app needs.
for (const entry of readdirSync(source)) {
  const full = join(source, entry);
  if (statSync(full).isFile() && (entry.endsWith(".exe") || entry.endsWith(".dll"))) {
    copyFileSync(full, join(staging, entry));
  }
}

rmSync(outFile, { force: true });
execFileSync(
  "powershell",
  [
    "-NoProfile",
    "-Command",
    `Compress-Archive -Path '${staging}\\*' -DestinationPath '${outFile}' -Force`,
  ],
  { stdio: "inherit" },
);

console.log(`payload written: ${outFile}`);
