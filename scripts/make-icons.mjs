// Renders the app's mark to every icon the project ships.
//
// `src-tauri/icons/icon.svg` is the single source of truth for the artwork: the
// three thread paths, the 1.9 stroke width and the 0.55 weft opacity. Every
// other copy of the glyph is *derived* from it here — the rounded browser
// variants, the 1024px PNG that `tauri icon` consumes, and the installer's
// .ico — so a redrawn mark cannot leave half the project on the old one. That
// is exactly what happened before: five hand-pasted copies of the same paths,
// and only some of them regenerated.
//
// Run via `bun run icons`. It is idempotent and safe to run in CI, which is how
// a release stops trusting whatever rasters happened to be committed.
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { Resvg } from "@resvg/resvg-js";

const at = (relative) => fileURLToPath(new URL(relative, import.meta.url));

const svgPath = at("../src-tauri/icons/icon.svg");
const pngPath = at("../src-tauri/icons/icon-1024.png");
const roundPath = at("../src-tauri/icons/icon-round.svg");
const indexHtmlPath = at("../index.html");
const siteIconPath = at("../site/src/app/icon.svg");

/** The window radius (20px) scaled onto the 24-unit grid, as the app uses it. */
const ROUND_RADIUS = "5.4";

// ---------------------------------------------------------------------------
// Read the source of truth, and insist it is the shape the rest of the code
// assumes: a full-bleed square. The rounded variants are derived below; a
// radius in the source would double-round every one of them.
// ---------------------------------------------------------------------------
const source = readFileSync(svgPath, "utf8");

const GLYPH = {
  warp: "M6.5 5.5c0 6.5 5.5 6.5 5.5 13",
  weft: "M12 5.5c0 6.5 5.5 6.5 5.5 13",
  hem: "M6.5 18.5h11",
};
const STROKE = "1.9";
const HEM_OPACITY = "0.55";

function bail(message) {
  console.error(`make-icons: ${message}`);
  console.error(
    `make-icons: fix ${svgPath} — it is the one file the rest are generated from.`,
  );
  process.exit(1);
}

for (const [name, path] of Object.entries(GLYPH)) {
  if (!source.includes(path)) bail(`icon.svg is missing the ${name} path (${path})`);
}
if (!source.includes(`stroke-width="${STROKE}"`)) {
  bail(`icon.svg no longer uses stroke-width="${STROKE}"`);
}
if (!source.includes(`stroke-linecap="round"`)) {
  bail("icon.svg no longer rounds its line caps");
}
if (!new RegExp(`opacity="${HEM_OPACITY}"`).test(source)) {
  bail(`icon.svg no longer draws the hem at opacity="${HEM_OPACITY}"`);
}
if (/<rect[^>]*\brx=/.test(source)) {
  bail(
    "icon.svg now has rounded corners. Keep it square: the OS icons need a full " +
      "bleed, and the rounded browser variants are derived from this file",
  );
}

// ---------------------------------------------------------------------------
// The 1024px PNG that `tauri icon` consumes.
// ---------------------------------------------------------------------------
const png = new Resvg(source, { fitTo: { mode: "width", value: 1024 } })
  .render()
  .asPng();
writeFileSync(pngPath, png);
console.log(`wrote ${pngPath} (${png.length} bytes)`);

// ---------------------------------------------------------------------------
// The rounded tile, for places that draw the mark as a *browser* icon: a tab, a
// bookmark, a social card. Those render the artwork directly rather than an OS
// shell, so they carry their own corner radius.
// ---------------------------------------------------------------------------
function roundedSvg({ size, stroke }) {
  const dimensions = size ? ` width="${size}" height="${size}"` : "";
  return `<svg${dimensions} viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
  <rect width="24" height="24" rx="${ROUND_RADIUS}" fill="#ffffff"/>
  <g fill="none" stroke="${stroke}" stroke-width="${STROKE}" stroke-linecap="round">
    <path d="${GLYPH.warp}"/>
    <path d="${GLYPH.weft}"/>
    <path d="${GLYPH.hem}" opacity="${HEM_OPACITY}"/>
  </g>
</svg>
`;
}

const round = roundedSvg({ size: 1024, stroke: "#000000" });
writeFileSync(roundPath, round);
console.log(`wrote ${roundPath}`);

// ---------------------------------------------------------------------------
// `index.html`: the dev-server favicon, as an inline data URI. Generated rather
// than pasted so it cannot drift from the mark.
// ---------------------------------------------------------------------------
// Single-quoted attributes and no newlines: the URI is going inside a
// double-quoted HTML attribute, and `encodeURIComponent` leaves `'` alone, so
// the mark stays readable in the source instead of becoming %22 soup.
//
// `#` is left for `encodeURIComponent` to escape. Escaping it here first would
// make the `%` itself encode to `%25`, and the colour would arrive as
// `%2523000000` — an SVG that renders nothing.
const favicon = roundedSvg({ size: null, stroke: "#000000" }).replace(/\n\s*/g, "");
const faviconUri = `data:image/svg+xml,${encodeURIComponent(
  favicon.replace(/"/g, "'"),
).replace(/%20/g, " ")}`;

const html = readFileSync(indexHtmlPath, "utf8");
const link = /(<link\s+rel="icon"\s+href=")[^"]*(")/;
if (!link.test(html)) {
  bail(`could not find the favicon <link> in ${indexHtmlPath}`);
}
const nextHtml = html.replace(link, `$1${faviconUri}$2`);
writeFileSync(indexHtmlPath, nextHtml);
console.log(`updated the favicon in ${indexHtmlPath}`);

// ---------------------------------------------------------------------------
// The marketing site's icon. Same rounded tile, and the same black as the app:
// it used to carry its own `#0a0c16`, which read as a different mark beside it.
// ---------------------------------------------------------------------------
writeFileSync(
  siteIconPath,
  `<!-- Generated from src-tauri/icons/icon.svg by scripts/make-icons.mjs.
     Do not edit by hand: run \`bun run icons\` instead.

     The app's mark on a rounded white tile, which is how a browser icon is
     drawn. The OS icons stay full-bleed squares — a shell supplies its own
     corner radius, and the installer writes a plain .ico. -->
${roundedSvg({ size: null, stroke: "#000000" })}`,
);
console.log(`wrote ${siteIconPath}`);
