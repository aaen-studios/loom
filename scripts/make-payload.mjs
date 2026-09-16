// Builds setup/src-tauri/payload.zip from a built Loom app directory.
//
// Usage: node scripts/make-payload.mjs [sourceDir] [outFile]
// Defaults: target/release -> setup/src-tauri/payload.zip
//
// Pass the output of `bun run tauri build --no-bundle` (the release profile).
// A `--debug` build also embeds the frontend, but it prefers the Vite dev
// server while one is running, so it is not what you want to ship.
import { execFileSync } from "node:child_process";
import { existsSync, rmSync, mkdirSync, copyFileSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const source = resolve(process.argv[2] ?? join(root, "target", "release"));
const outFile = resolve(process.argv[3] ?? join(root, "setup", "src-tauri", "payload.zip"));

const exe = join(source, "loom.exe");
if (!existsSync(exe)) {
  console.error(`loom.exe not found in ${source}. Run "bun run tauri build --no-bundle" first.`);
  process.exit(1);
}

const distIndex = join(root, "dist", "index.html");
if (!existsSync(distIndex)) {
  console.error(
    `No frontend build found at ${distIndex}. Run "bun run build" so the binary has assets to embed.`,
  );
  process.exit(1);
}

if (exe.includes(`${join("target", "debug")}`)) {
  console.warn(
    `warning: ${exe} looks like a debug build; shipping it works (it falls back to the` +
      ` bundled frontend) but the release profile is smaller and faster.`,
  );
}

const staging = join(root, "target", "payload-staging");
rmSync(staging, { recursive: true, force: true });
mkdirSync(staging, { recursive: true });

// The app binary plus any runtime DLLs it needs. Other executables in
// target/release (loom-setup.exe above all) belong to someone else — shipping
// them would embed the installer inside its own payload. `loom_lib.dll` is the
// crate's cdylib target, not a runtime sidecar: the exe is self-contained.
const skipped = [];
for (const entry of readdirSync(source)) {
  const full = join(source, entry);
  if (!statSync(full).isFile()) continue;
  const runtimeDll = entry.endsWith(".dll") && entry !== "loom_lib.dll";
  if (entry === "loom.exe" || runtimeDll) {
    copyFileSync(full, join(staging, entry));
  } else if (entry.endsWith(".exe") || entry.endsWith(".dll")) {
    skipped.push(entry);
  }
}
if (skipped.length > 0) {
  console.log(`payload: skipped unrelated binaries: ${skipped.join(", ")}`);
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
