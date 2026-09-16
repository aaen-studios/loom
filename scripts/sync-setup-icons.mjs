// Copies the app's generated Windows icon into Loom Setup, so the installer
// binary can never ship a stale icon. Run by `bun run icons` after `tauri icon`.
//
// The tauri.conf.json files are touched afterwards: the build scripts decide
// whether to re-embed the Windows resources from their watched inputs, and an
// icon swap alone is not one of them. Without this, a rebuild would silently
// keep the previous icon in the exe.
import { copyFileSync, utimesSync } from "node:fs";
import { fileURLToPath } from "node:url";

const appIco = fileURLToPath(new URL("../src-tauri/icons/icon.ico", import.meta.url));
const setupIco = fileURLToPath(new URL("../setup/src-tauri/icons/icon.ico", import.meta.url));
copyFileSync(appIco, setupIco);
console.log(`copied ${appIco} -> ${setupIco}`);

const now = new Date();
for (const relative of ["../src-tauri/tauri.conf.json", "../setup/src-tauri/tauri.conf.json"]) {
  const config = fileURLToPath(new URL(relative, import.meta.url));
  utimesSync(config, now, now);
  console.log(`touched ${config}`);
}
