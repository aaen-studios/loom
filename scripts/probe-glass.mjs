// The decisive Phase 0 question: does the refraction actually produce different
// pixels, or does the DOM merely look right?
//
// Computed styles cannot answer this. `filter: url(#id)` can parse, resolve, and
// still rasterise to something indistinguishable from no filter at all — which
// is exactly what a heavy `backdrop-filter` will do, since a flat smear has no
// detail left for a displacement map to bend.
//
// Method: A/B the SAME live element.
//
//   A  screenshot with the warp's filter as it ships
//   B  screenshot with `filter: none` forced onto it
//
// Both PNGs are handed back to the page, decoded with `createImageBitmap`, drawn
// to a canvas and compared channel by channel. A difference of zero means the
// effect is not rendering, whatever the styles say.
//
// Run against two backdrops, because the answer genuinely differs:
//
//   * a hard-edged pattern — the best case for a displacement map
//   * Loom's own current background — the case that actually matters
//
//   node target/probe-ab.mjs 9333
import { writeFileSync } from "node:fs";

const port = process.argv[2] ?? "9333";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/* ---------------------------------------------------------- attach properly */

let page = null;
for (let attempt = 0; attempt < 40 && !page; attempt += 1) {
  try {
    const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
    const pages = targets.filter((t) => t.type === "page" && t.webSocketDebuggerUrl);
    page = pages.find((t) => t.url.includes("1420")) ?? null;
  } catch {
    /* not up yet */
  }
  if (!page) await sleep(500);
}
if (!page) {
  console.error("no app page on port", port);
  process.exit(1);
}
console.log(`attached: ${page.url}\n`);

const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
socket.addEventListener("message", (m) => {
  const p = JSON.parse(m.data);
  if (p.id && pending.has(p.id)) {
    pending.get(p.id)(p.result ?? p.error);
    pending.delete(p.id);
  }
});
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const id = nextId++;
    pending.set(id, resolve);
    socket.send(JSON.stringify({ id, method, params }));
  });
const evaluate = async (expression, awaitPromise = true) => {
  const r = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise });
  if (r?.exceptionDetails) return `EVAL ERROR: ${r.exceptionDetails.text}`;
  return r?.result?.value;
};

await new Promise((r) => socket.addEventListener("open", r));
await send("Runtime.enable");
await send("Page.enable");

// Wait for the app.
for (let attempt = 0; attempt < 60; attempt += 1) {
  const ready = await evaluate('!!document.querySelector(".lg-shell .glass__warp")', false);
  if (ready === true) break;
  await sleep(500);
}

/* --------------------------------------------------- geometry of what we test */

const target = await evaluate(`(() => {
  // The widest pill: most backdrop inside one displacement box.
  const stages = [...document.querySelectorAll(".lg-stage")];
  if (!stages.length) return "";
  const best = stages.sort((a, b) =>
    b.getBoundingClientRect().width - a.getBoundingClientRect().width)[0];
  const b = best.getBoundingClientRect();
  return JSON.stringify({
    x: Math.floor(b.x), y: Math.floor(b.y),
    width: Math.ceil(b.width), height: Math.ceil(b.height),
  });
})()`, false);

if (!target) {
  console.error("no liquid surface to measure — is Phase 0 still mounted?");
  process.exit(1);
}
const clip = { ...JSON.parse(target), scale: 3 };
console.log("measuring region:", JSON.stringify(clip), "\n");

/* ------------------------------------------------------------- decode + diff

   Node has no PNG decoder here, but the page does. So both screenshots are
   base64 strings that get handed straight back to the browser.               */

const DIFF = `(async (aB64, bB64) => {
  const load = async (b64) => {
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    const bitmap = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    const ctx = canvas.getContext("2d");
    ctx.drawImage(bitmap, 0, 0);
    return { data: ctx.getImageData(0, 0, bitmap.width, bitmap.height).data, w: bitmap.width, h: bitmap.height };
  };
  const A = await load(aB64);
  const B = await load(bB64);
  if (A.w !== B.w || A.h !== B.h) return JSON.stringify({ error: "size mismatch" });

  let differing = 0;
  let maxDelta = 0;
  let sumDelta = 0;
  const total = A.w * A.h;
  for (let i = 0; i < A.data.length; i += 4) {
    const dr = Math.abs(A.data[i] - B.data[i]);
    const dg = Math.abs(A.data[i + 1] - B.data[i + 1]);
    const db = Math.abs(A.data[i + 2] - B.data[i + 2]);
    const d = Math.max(dr, dg, db);
    if (d > 2) differing += 1;
    if (d > maxDelta) maxDelta = d;
    sumDelta += d;
  }
  return JSON.stringify({
    size: A.w + "x" + A.h,
    pixels: total,
    differingPixels: differing,
    percentDiffering: +((differing / total) * 100).toFixed(2),
    maxChannelDelta: maxDelta,
    meanChannelDelta: +(sumDelta / total).toFixed(2),
  });
})`;

/** A/B one region, returning the raw numbers plus the two PNGs on disk. */
async function ab(label, name) {
  const a = await send("Page.captureScreenshot", { format: "png", clip });
  const disable = await evaluate(
    `(() => {
       const el = document.createElement("style");
       el.id = "lg-ab-disable";
       el.textContent = ".lg-shell .glass__warp { filter: none !important; }";
       document.head.appendChild(el);
       return true;
     })()`,
    false,
  );
  if (disable !== true) console.log("  (could not inject the disable rule)");
  await sleep(700);
  const b = await send("Page.captureScreenshot", { format: "png", clip });
  await evaluate('document.getElementById("lg-ab-disable")?.remove()', false);
  await sleep(400);

  if (!a?.data || !b?.data) {
    console.log(`${label}: screenshot failed`);
    return null;
  }
  writeFileSync(`target/ab-${name}-filter-on.png`, Buffer.from(a.data, "base64"));
  writeFileSync(`target/ab-${name}-filter-off.png`, Buffer.from(b.data, "base64"));
  const bytes = Buffer.from(a.data, "base64").length;
  const identicalBytes = a.data === b.data;
  const diff = await evaluate(`(${DIFF})(${JSON.stringify(a.data)}, ${JSON.stringify(b.data)})`);
  const parsed = typeof diff === "string" ? JSON.parse(diff) : null;
  console.log(`${label}`);
  console.log(`  png bytes ${bytes}, byte-identical: ${identicalBytes}`);
  console.log(`  ${diff}`);
  return parsed;
}

/* --------------------------------------- case 1: Loom's own current background */

console.log("=== CASE 1: Loom's live background ===");
const onLive = await ab("filter on vs filter off, over whatever is behind it now", "live");

/* ------------------------- case 2: a hard-edged backdrop, the best case for it

   The built-in presets are soft radial washes by design (see
   `lib/background.ts`), and a displacement map has nothing to bend in a smooth
   gradient. This puts a hard-edged pattern behind the same element so the
   effect's ceiling is visible — and so a weak result on the live background can
   be attributed to the background rather than to the wiring.                  */

console.log("\n=== CASE 2: a hard-edged backdrop (best case) ===");
const injected = await evaluate(`(() => {
  const layer = document.createElement("div");
  layer.id = "lg-test-backdrop";
  layer.style.cssText =
    "position:fixed;inset:0;z-index:0;pointer-events:none;background:" +
    "repeating-linear-gradient(45deg,#e11 0 14px,#11e 14px 28px,#ee1 28px 42px,#1e1 42px 56px)," +
    "repeating-linear-gradient(-45deg,#fff 0 9px,rgba(0,0,0,.75) 9px 18px)";
  document.body.appendChild(layer);
  return true;
})()`, false);
if (injected !== true) console.log("  (backdrop injection failed)");
await sleep(900);
const onHard = await ab("filter on vs filter off, over a hard-edged pattern", "hard");

/* ----------------------------- case 3: does it survive Loom's own tint + blur?

   The wrapper deliberately lays a tint ABOVE the warp. If that tint is too
   strong the refraction is buried, which is the one trade-off this design makes
   and therefore the one worth measuring rather than eyeballing.               */

console.log("\n=== CASE 3: the same, with the tint reduced to 0 ===");
const cleared = await evaluate(`(() => {
  const el = document.createElement("style");
  el.id = "lg-ab-notint";
  el.textContent = ".lg-tint { background-color: transparent !important; }";
  document.head.appendChild(el);
  return true;
})()`, false);
if (cleared !== true) console.log("  (tint override failed)");
await sleep(700);
const onHardNoTint = await ab("filter on vs filter off, hard backdrop, no tint", "hard-notint");
await evaluate('document.getElementById("lg-ab-notint")?.remove()', false);

// Clean up the injected backdrop so the running app is left as we found it.
await evaluate('document.getElementById("lg-test-backdrop")?.remove()', false);

/* ----------------------------------------------------------------- verdict */

console.log("\n=== VERDICT ===");
const cases = [
  ["live Loom background", onLive],
  ["hard-edged backdrop", onHard],
  ["hard-edged, tint removed", onHardNoTint],
];
for (const [label, result] of cases) {
  if (!result) {
    console.log(`  ?     ${label}: no measurement`);
    continue;
  }
  const ok = result.differingPixels > 0 && result.maxChannelDelta > 8;
  console.log(
    `  ${ok ? "PASS" : "FAIL"}  ${label}: ${result.percentDiffering}% of pixels change, ` +
      `max channel delta ${result.maxChannelDelta}`,
  );
}
console.log(
  "\n  A PASS means the filter is bending real pixels. Percentages in the low\n" +
    "  single digits are expected: `feDisplacementMap` only bites near the edges,\n" +
    "  which is the whole point of the edge mask in the library's filter graph.",
);

socket.close();
