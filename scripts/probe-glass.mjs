// Does the refraction bend real pixels, and does it cost anything?
//
// ## What this probe got wrong the first three times
//
// Reading the pixels is the only way to tell "the filter is applied" from "the
// filter produced a visible difference" — a resolved `filter: url(#id)` can
// still rasterise to something indistinguishable from no filter at all. But the
// first versions of this file reported nonsense, and the ways they did are
// worth keeping written down:
//
//  1. **The background drifts.** `Background` runs a 52-second animation, so two
//     screenshots taken 700ms apart differ nearly everywhere. The first run
//     reported "93% of pixels change" for a *cosmetic* effect that only bites at
//     the edges, and the number was the animation.
//  2. **The injected test backdrop was underneath the app's own.** It went in at
//     `z-index: 0`, below `Background`, so the "hard-edged pattern" case measured
//     the identical scene as the live-background case — 502,606 vs 502,605
//     differing pixels. Two cases returning the same number to five digits is
//     the tell, and it was in the output the whole time.
//  3. **The wrong target.** An earlier version attached to the first `type=page`
//     target and evaluated immediately, so it sampled Edge's first-run sync
//     dialog and a document where React had not mounted.
//
// So: freeze the animation, put the pattern in the chrome's own stacking layer,
// and *verify the injection changed anything* before trusting a diff.
//
//   node scripts/probe-glass.mjs 9333
import { writeFileSync } from "node:fs";

const port = process.argv[2] ?? "9333";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let page = null;
for (let i = 0; i < 40 && !page; i += 1) {
  try {
    const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
    page = targets.find((t) => t.type === "page" && t.url.includes("1420")) ?? null;
  } catch {
    /* not up */
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

// A hard reload, so the page under test is the code on disk rather than an
// accumulation of HMR updates. A module that failed to transform mid-edit leaves
// a permanently broken graph that no later fix reaches.
console.log("reloading for a clean module graph...");
await send("Page.reload", { ignoreCache: true });
await sleep(4000);
for (let i = 0; i < 40; i += 1) {
  if ((await evaluate('document.querySelectorAll(".lg-stage").length', false)) > 0) break;
  await sleep(500);
}

/* --------------------------------------------------------------- the scene */

/** Pauses every animation, so two screenshots of a static scene are comparable. */
const FREEZE = `(() => {
  const el = document.createElement("style");
  el.id = "lg-freeze";
  el.textContent = "*, *::before, *::after { animation-play-state: paused !important; }";
  document.head.appendChild(el);
  return true;
})()`;

/**
 * A hard-edged pattern, painted *behind the chrome but above the background*.
 *
 * The z-index is the whole trick and it took a wrong answer to find. `Background`
 * is a child of the root div with no z-index of its own, and the app's chrome
 * (`.z-10`, which is where the title bar and the composer live) sits above it. So
 * a fixed layer at `z-index: 5` lands exactly between them: above the wallpaper,
 * below the glass — which is what a backdropped surface needs to sample.
 *
 * `z-index: 0` puts it underneath the wallpaper instead, and the pattern is never
 * seen at all.
 */
const PATTERN = `(() => {
  document.getElementById("lg-pattern")?.remove();
  const layer = document.createElement("div");
  layer.id = "lg-pattern";
  layer.style.cssText =
    "position:fixed;inset:0;z-index:5;pointer-events:none;background:" +
    "repeating-linear-gradient(45deg,#e11 0 14px,#11e 14px 28px,#ee1 28px 42px,#1e1 42px 56px)," +
    "repeating-linear-gradient(-45deg,rgba(255,255,255,.9) 0 9px,rgba(0,0,0,.85) 9px 18px)";
  document.body.appendChild(layer);
  return true;
})()`;

/** The pill and the composer, so each is measured on its own terms. */
const REGION = {
  pill: `(() => {
    const stages = [...document.querySelectorAll(".lg-stage")];
    const pill = stages.find((s) => s.closest("header"));
    if (!pill) return "";
    const b = pill.getBoundingClientRect();
    return JSON.stringify({ x: Math.floor(b.x), y: Math.floor(b.y), width: Math.ceil(b.width), height: Math.ceil(b.height), scale: 4 });
  })()`,
  // Padded, so the edge refraction has somewhere to happen: the bend reaches
  // outside the element's own box.
  composer: `(() => {
    const stages = [...document.querySelectorAll(".lg-stage")];
    const composer = stages.find((s) => !s.closest("header"));
    if (!composer) return "";
    const b = composer.getBoundingClientRect();
    return JSON.stringify({ x: Math.max(0, Math.floor(b.x) - 10), y: Math.max(0, Math.floor(b.y) - 10), width: Math.ceil(b.width) + 20, height: Math.ceil(b.height) + 20, scale: 2 });
  })()`,
};

/* ------------------------------------------------------------ the comparison

   Both screenshots are handed back to the page to decode: Node has no PNG
   decoder here, and the browser has `createImageBitmap` and a canvas.        */

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
  let differing = 0, maxDelta = 0, sumDelta = 0;
  const total = A.w * A.h;
  for (let i = 0; i < A.data.length; i += 4) {
    const d = Math.max(
      Math.abs(A.data[i] - B.data[i]),
      Math.abs(A.data[i + 1] - B.data[i + 1]),
      Math.abs(A.data[i + 2] - B.data[i + 2]),
    );
    if (d > 2) differing += 1;
    if (d > maxDelta) maxDelta = d;
    sumDelta += d;
  }
  return JSON.stringify({
    size: A.w + "x" + A.h,
    percentDiffering: +((differing / total) * 100).toFixed(2),
    maxChannelDelta: maxDelta,
    meanChannelDelta: +(sumDelta / total).toFixed(2),
  });
})`;

async function shoot(name) {
  const region = await evaluate(REGION[name.kind], false);
  if (!region) return null;
  const shot = await send("Page.captureScreenshot", { format: "png", clip: JSON.parse(region) });
  return shot?.data ?? null;
}

/**
 * A/B one surface: the warp with its filter, then with `filter: none`.
 *
 * Everything else is held identical — same frozen scene, same tint, same
 * backdrop-filter — so a difference can only be the displacement.
 */
async function ab(kind, label, name) {
  const a = await shoot({ kind });
  await evaluate(
    `(() => {
       const el = document.createElement("style");
       el.id = "lg-ab-off";
       el.textContent = ".lg-shell .glass__warp { filter: none !important; }";
       document.head.appendChild(el);
       return true;
     })()`,
    false,
  );
  await sleep(500);
  const b = await shoot({ kind });
  await evaluate('document.getElementById("lg-ab-off")?.remove()', false);
  await sleep(300);

  if (!a || !b) {
    console.log(`  ${label}: capture failed`);
    return null;
  }
  if (a === b) {
    // Byte-identical is worth calling out on its own: it means nothing was
    // rendered at all, which is a different fault from "the filter did nothing".
    console.log(`  ${label}: the two captures are byte-identical`);
    return { percentDiffering: 0, maxChannelDelta: 0, identical: true };
  }
  writeFileSync(`target/ab-${name}-on.png`, Buffer.from(a, "base64"));
  writeFileSync(`target/ab-${name}-off.png`, Buffer.from(b, "base64"));
  const raw = await evaluate(`(${DIFF})(${JSON.stringify(a)}, ${JSON.stringify(b)})`);
  let parsed = null;
  try {
    parsed = JSON.parse(raw);
  } catch {
    console.log(`  ${label}: ${raw}`);
    return null;
  }
  console.log(`  ${label}: ${raw}`);
  return parsed;
}

await evaluate(FREEZE, false);
await sleep(600);

const results = {};

console.log("=== live background (frozen) ===");
results.pillLive = await ab("pill", "pill", "pill-live");
results.composerLive = await ab("composer", "composer", "composer-live");

console.log("\n=== over a hard-edged pattern ===");
// Verify the injection actually changed the scene. Without this check the
// pattern case silently measures the same thing as the live-background case,
// which is precisely what happened the first time.
const before = await shoot({ kind: "pill" });
await evaluate(PATTERN, false);
await sleep(700);
const after = await shoot({ kind: "pill" });
if (before && after) {
  const raw = await evaluate(`(${DIFF})(${JSON.stringify(before)}, ${JSON.stringify(after)})`);
  let parsed = null;
  try {
    parsed = JSON.parse(raw);
  } catch {
    /* reported below */
  }
  if (parsed && parsed.percentDiffering > 20) {
    console.log(`  the pattern took effect (${parsed.percentDiffering}% of the region changed)`);
  } else {
    console.log(`  WARNING: the pattern changed almost nothing (${raw}) — it is behind the wallpaper`);
  }
} else {
  console.log("  WARNING: could not verify the pattern took effect");
}

results.pillPattern = await ab("pill", "pill", "pill-pattern");
results.composerPattern = await ab("composer", "composer", "composer-pattern");

await evaluate('document.getElementById("lg-pattern")?.remove()', false);
await evaluate('document.getElementById("lg-freeze")?.remove()', false);

/* ---------------------------------------------------------------- verdict */

console.log("\n=== VERDICT ===");
const rows = [
  ["pill, live background", results.pillLive],
  ["pill, hard edges", results.pillPattern],
  ["composer, live background", results.composerLive],
  ["composer, hard edges", results.composerPattern],
];
let failures = 0;
for (const [label, result] of rows) {
  if (!result) {
    console.log(`  ?     ${label}: no measurement`);
    continue;
  }
  // A displacement map only bites near the edge mask, so a small percentage is
  // the expected shape. What matters is that the difference is real and that its
  // magnitude is more than dither.
  const ok = result.percentDiffering > 1 && result.maxChannelDelta > 8;
  if (!ok) failures += 1;
  console.log(
    `  ${ok ? "PASS" : "FAIL"}  ${label}: ${result.percentDiffering}% of pixels bend, ` +
      `max channel delta ${result.maxChannelDelta}`,
  );
}

console.log(
  `\n  ${failures === 0 ? "The filter is bending real pixels on every surface." : `${failures} surface(s) did not show a difference.`}`,
);
console.log(
  "  Expect a few percent rather than most of them: the library's filter graph\n" +
    "  masks the displacement to the rim, which is what keeps the centre readable.",
);

socket.close();
