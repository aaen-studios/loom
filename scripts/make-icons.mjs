// Rasterizes src-tauri/icons/icon.svg to the 1024px source PNG that
// `tauri icon` consumes. Run via `bun run icons`.
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { Resvg } from "@resvg/resvg-js";

const svgPath = fileURLToPath(new URL("../src-tauri/icons/icon.svg", import.meta.url));
const outPath = fileURLToPath(new URL("../src-tauri/icons/icon-1024.png", import.meta.url));

const svg = readFileSync(svgPath, "utf8");
const resvg = new Resvg(svg, { fitTo: { mode: "width", value: 1024 } });
const png = resvg.render().asPng();

writeFileSync(outPath, png);
console.log(`wrote ${outPath} (${png.length} bytes)`);
