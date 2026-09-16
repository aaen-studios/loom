// Copies the app's generated Windows icon into Loom Setup, so the installer
// binary can never ship a stale icon. Run by `bun run icons` after `tauri icon`.
//
// The tauri.conf.json files are touched afterwards: the build scripts decide
// whether to re-embed the Windows resources from their watched inputs, and an
// icon swap alone is not one of them. Without this, a rebuild would silently
// keep the previous icon in the exe.
//
// The copy is verified byte-for-byte rather than assumed. A silent partial
// write here is invisible until someone notices the old logo on a shortcut,
// which is precisely the bug this script exists to prevent.
import { copyFileSync, readFileSync, utimesSync } from "node:fs";
import { fileURLToPath } from "node:url";

const appIco = fileURLToPath(new URL("../src-tauri/icons/icon.ico", import.meta.url));
const setupIco = fileURLToPath(new URL("../setup/src-tauri/icons/icon.ico", import.meta.url));

copyFileSync(appIco, setupIco);

const appBytes = readFileSync(appIco);
const setupBytes = readFileSync(setupIco);
if (!appBytes.equals(setupBytes)) {
  console.error(
    `sync-setup-icons: ${setupIco} does not match ${appIco} after copying ` +
      `(${setupBytes.length} vs ${appBytes.length} bytes). The installer would ` +
      `carry a different icon from the app. Check the file is not locked, then ` +
      `re-run \`bun run icons\`.`,
  );
  process.exit(1);
}
console.log(`copied and verified ${appIco} -> ${setupIco} (${appBytes.length} bytes)`);

// An empty .ico is worse than a stale one: the exe would have no icon at all.
if (appBytes.length === 0) {
  console.error("sync-setup-icons: the app icon is empty; run `tauri icon` first.");
  process.exit(1);
}

const now = new Date();
for (const relative of ["../src-tauri/tauri.conf.json", "../setup/src-tauri/tauri.conf.json"]) {
  const config = fileURLToPath(new URL(relative, import.meta.url));
  utimesSync(config, now, now);
  console.log(`touched ${config}`);
}
