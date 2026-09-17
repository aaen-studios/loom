// Why is refraction invisible over Loom's own background presets, and what
// fixes it?
//
// Measured earlier: a title-bar pill over a built-in preset bends 0% of pixels
// (max channel delta 2), against 87.75% over a hard-edged pattern. If that
// stays true, adding liquid glass "everywhere" produces a lot of machinery that
// visibly does nothing, so it has to be understood before more surfaces are
// converted.
//
// The hypothesis, from the two properties the library puts on one element:
//
//     backdrop-filter: blur(6px) saturate(1.4)   <- what is behind, blurred
//     filter: url(#displacement)                  <- then displaced
//
// `backdrop-filter` runs first and blurs the backdrop; `filter` then displaces
// the *blurred* result. So the frost destroys the detail before the displacement
// can bend it, and what is left to shift is a smooth gradient — which, shifted,
// still looks like the same smooth gradient.
//
// If that is right, two things should both help, and they are different fixes:
//
//   * less frost on the surface, so the detail survives to the displacement
//   * more high-frequency detail in the background, so there is something for it
//     to grab even after a blur
//
// This measures each, one variable at a time, and then both together. It also
// re-checks the hard-edged case at the end, so a change that fixes the presets
// by breaking the best case would be caught rather than shipped.
const port = process.argv[2] ?? "9333";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let page = null;
for (let i = 0; i < 40 && !page; i += 1) {
  try {
    const targets = await (await fetch("http://127.0.0.1:" + port + "/json")).json();
    page = targets.find((t) => t.type === "page" && t.url.includes("1420")) ?? null;
  } catch {
    /* not up */
  }
  if (!page) await sleep(500);
}
if (!page) {
  console.error("no app page on port " + port);
  process.exit(1);
}

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
  const r = await send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise,
  });
  if (r?.exceptionDetails) return "EVAL ERROR: " + r.exceptionDetails.text;
  return r?.result?.value;
};

await new Promise((r) => socket.addEventListener("open", r));
await send("Runtime.enable");
await send("Page.enable");

await evaluate('localStorage.setItem("loomDebug","1")', false);
await send("Page.reload", { ignoreCache: true });
await sleep(4500);
for (let i = 0; i < 40; i += 1) {
  if ((await evaluate('document.querySelectorAll(".lg-stage").length', false)) > 0) break;
  await sleep(500);
}

// Make sure the surface under test is at its shipped values, and the background
// is one of Loom's own presets rather than whatever the last run left.
await evaluate(
  "(() => { const s = window.__loom.settings.getState();" +
    " s.setBackground({ kind: 'builtin', preset: 'porcelain', path: null, dim: 0, blur: 0 });" +
    " s.setLiquid({ frost: 6, refraction: 32, chromatics: 2, saturation: 140, mode: 'standard' });" +
    " return true; })()",
  false,
);
await sleep(1200);

/* --------------------------------------------------------------- the helpers */

const FREEZE =
  "(() => { let e = document.getElementById('m-freeze');" +
  " if (!e) { e = document.createElement('style'); e.id = 'm-freeze'; document.head.appendChild(e); }" +
  " e.textContent = '*, *::before, *::after { animation-play-state: paused !important; }';" +
  " return true; })()";

// High-frequency detail, appended inside the Background layer's own container so
// it sits above the drifting washes but below the z-10 chrome — which is exactly
// where the glass samples. A few pixels wide rather than hairlines, because the
// whole point is to survive a blur.
// Built line by line and joined, never with `+` inside the injected source. The
// first version concatenated the CSS *inside* the string it was building, which
// produced `...'  root.appendChild(d)` — a syntax error in the page, thrown
// silently, so the texture was never added and the run reported "0% change" for
// an experiment that had not happened. The returned message is checked below.
const TEXTURE = [
  "(() => {",
  "  document.getElementById('m-texture')?.remove();",
  "  const root = document.querySelector('.animate-drift')?.parentElement;",
  "  if (!root) return 'no background layer';",
  "  const d = document.createElement('div');",
  "  d.id = 'm-texture';",
  "  d.style.position = 'absolute';",
  "  d.style.inset = '0';",
  "  d.style.pointerEvents = 'none';",
  "  d.style.backgroundImage = [",
  "    'repeating-linear-gradient(45deg, rgba(0,0,0,.5) 0 3px, rgba(255,255,255,.5) 3px 6px)',",
  "    'repeating-linear-gradient(-45deg, rgba(0,0,0,.35) 0 2px, transparent 2px 9px)',",
  "  ].join(',');",
  "  root.appendChild(d);",
  "  return 'texture added to ' + root.className;",
  "})()",
].join("\n");

const NO_TEXTURE = "(() => { document.getElementById('m-texture')?.remove(); return true; })()";

// The hard-edged case, as the control: injected as a fixed layer above the
// wallpaper and below the chrome.
const PATTERN = [
  "(() => {",
  "  document.getElementById('m-pattern')?.remove();",
  "  const d = document.createElement('div');",
  "  d.id = 'm-pattern';",
  "  d.style.position = 'fixed';",
  "  d.style.inset = '0';",
  // Above the wallpaper, below the chrome (`.z-10`). `z-index: 0` puts it *under*
  // the wallpaper, where nothing samples it.
  "  d.style.zIndex = '5';",
  "  d.style.pointerEvents = 'none';",
  "  d.style.backgroundImage =",
  "    'repeating-linear-gradient(45deg,#e11 0 14px,#11e 14px 28px,#ee1 28px 42px,#1e1 42px 56px)';",
  "  document.body.appendChild(d);",
  "  const b = d.getBoundingClientRect();",
  "  return 'pattern ' + Math.round(b.width) + 'x' + Math.round(b.height);",
  "})()",
].join("\n");

// The widest pill: the most backdrop inside one displacement box.
const REGION =
  "(() => {" +
  "  const stages = [...document.querySelectorAll('header .lg-stage')];" +
  "  if (!stages.length) return '';" +
  "  const best = stages.sort((a, b) => b.getBoundingClientRect().width - a.getBoundingClientRect().width)[0];" +
  "  const b = best.getBoundingClientRect();" +
  "  return JSON.stringify({ x: Math.floor(b.x), y: Math.floor(b.y), width: Math.ceil(b.width), height: Math.ceil(b.height), scale: 4 });" +
  "})()";

const DIFF = [
  "(async (aB64, bB64) => {",
  "  const load = async (b64) => {",
  "    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));",
  "    const bitmap = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));",
  "    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);",
  "    const ctx = canvas.getContext('2d');",
  "    ctx.drawImage(bitmap, 0, 0);",
  "    return ctx.getImageData(0, 0, bitmap.width, bitmap.height).data;",
  "  };",
  "  const A = await load(aB64);",
  "  const B = await load(bB64);",
  "  let differing = 0; let maxDelta = 0; let sum = 0;",
  "  const total = A.length / 4;",
  "  for (let i = 0; i < A.length; i += 4) {",
  "    const d = Math.max(Math.abs(A[i] - B[i]), Math.abs(A[i+1] - B[i+1]), Math.abs(A[i+2] - B[i+2]));",
  "    if (d > 2) differing += 1;",
  "    if (d > maxDelta) maxDelta = d;",
  "    sum += d;",
  "  }",
  "  return JSON.stringify({ pct: +((differing / total) * 100).toFixed(2), max: maxDelta, mean: +(sum / total).toFixed(2) });",
  "})",
].join("\n");

/** A/B the pill's own filter, over whatever is currently behind it. */
async function ab(label) {
  const region = await evaluate(REGION, false);
  if (!region) return null;
  const clip = JSON.parse(region);
  const a = await send("Page.captureScreenshot", { format: "png", clip });
  await evaluate(
    "(() => { let e = document.getElementById('m-off');" +
      " if (!e) { e = document.createElement('style'); e.id = 'm-off'; document.head.appendChild(e); }" +
      " e.textContent = '.lg-shell .glass__warp { filter: none !important; }';" +
      " return true; })()",
    false,
  );
  await sleep(600);
  const b = await send("Page.captureScreenshot", { format: "png", clip });
  await evaluate("document.getElementById('m-off')?.remove()", false);
  await sleep(300);
  if (!a?.data || !b?.data) {
    console.log("  " + label.padEnd(46) + "capture failed");
    return null;
  }
  const raw = await evaluate(
    "(" + DIFF + ")(" + JSON.stringify(a.data) + ", " + JSON.stringify(b.data) + ")",
  );
  let parsed = null;
  try {
    parsed = JSON.parse(raw);
  } catch {
    /* reported below */
  }
  if (parsed) {
    console.log(
      "  " + label.padEnd(46) +
        String(parsed.pct).padStart(6) + "% of pixels bend   max " + String(parsed.max).padStart(3) +
        "   mean " + String(parsed.mean).padStart(6),
    );
  } else {
    console.log("  " + label.padEnd(46) + raw);
  }
  return parsed;
}

/** Set the liquid params through the real store, so this is not a CSS fiction. */
async function setLiquid(patch) {
  await evaluate(
    "(() => { window.__loom.settings.getState().setLiquid(" + JSON.stringify(patch) + "); return true; })()",
    false,
  );
  await sleep(900);
}

const results = {};
async function measure(key, label) {
  results[key] = await ab(label);
  return results[key];
}

/* ------------------------------------------------------------------- the runs */

await evaluate(FREEZE, false);
await evaluate(NO_TEXTURE, false);
await sleep(700);

console.log("=== over Porcelain (Loom's own light preset) ===");
await measure("baseline", "as shipped: frost 6px, refraction 32");

console.log("\n=== each variable on its own ===");
await setLiquid({ frost: 4 });
await measure("lowFrost", "frost 4px (the floor), refraction 32");
await setLiquid({ frost: 6, refraction: 120 });
await measure("highRefraction", "frost 6px, refraction 120 (the ceiling)");
await setLiquid({ frost: 6, refraction: 32 });

// The return value is checked rather than assumed. A failed injection here
// reports "0% change", which is indistinguishable from "the hypothesis is wrong"
// — and that is exactly how the first run of this probe lied.
const tex = await evaluate(TEXTURE, false);
console.log("\n  texture injection: " + tex);
if (typeof tex !== "string" || !tex.startsWith("texture added")) {
  console.log("  ABORTING: the texture did not go in, so the next line would measure nothing.");
  await evaluate(NO_TEXTURE, false);
  socket.close();
  process.exit(1);
}
await measure("texture", "as shipped, + high-frequency texture behind");

console.log("\n=== both together ===");
await setLiquid({ frost: 4 });
await measure("textureLowFrost", "texture + frost 4px");
await setLiquid({ frost: 6, refraction: 120 });
await measure("textureAll", "texture + frost 4px + refraction 120");
await setLiquid({ frost: 6, refraction: 32 });

await evaluate(NO_TEXTURE, false);
await sleep(500);

console.log("\n=== control: the hard-edged pattern, unchanged ===");
const pat = await evaluate(PATTERN, false);
console.log("  pattern injection: " + pat);
if (typeof pat !== "string" || !pat.startsWith("pattern")) {
  console.log("  ABORTING: the pattern did not go in, so this control would read 0% too.");
  socket.close();
  process.exit(1);
}
await sleep(800);
await measure("pattern", "the best case, as shipped");
await evaluate("document.getElementById('m-pattern')?.remove()", false);

/* ------------------------------------------------------------------ the read */

console.log("\n=== what this says ===");
const base = results.baseline;
const lowFrost = results.lowFrost;
const hiRef = results.highRefraction;
const texture = results.texture;

if (base && base.pct < 1) {
  console.log("  Over the preset, the effect is invisible as shipped (" + base.pct + "%).");
} else if (base) {
  console.log("  Over the preset, it already bends " + base.pct + "% of pixels.");
}
if (base && lowFrost) {
  console.log(
    "  Lowering frost alone: " + base.pct + "% -> " + lowFrost.pct + "%  " +
      (lowFrost.pct > base.pct + 1 ? "(detail survives the blur)" : "(no real change)"),
  );
}
if (base && hiRef) {
  console.log(
    "  Raising refraction alone: " + base.pct + "% -> " + hiRef.pct + "%  " +
      (hiRef.pct > base.pct + 1 ? "(shifts a smooth gradient further, still smooth)" : "(no real change)"),
  );
}
if (base && texture) {
  console.log(
    "  Adding background detail: " + base.pct + "% -> " + texture.pct + "%  " +
      (texture.pct > base.pct + 5 ? "(this is the one that matters)" : "(not the whole story)"),
  );
}
const pattern = results.pattern;
if (pattern && pattern.pct < 10) {
  console.log(
    "\n  WARNING: the hard-edged control reads only " + pattern.pct + "%. It measured " +
      "87.75% in an earlier run, so the injection or the page is wrong and nothing " +
      "above should be believed.",
  );
} else if (pattern) {
  console.log("\n  The hard-edged control reads " + pattern.pct + "%, so the measurement works.");
}

await evaluate("window.__loom.settings.getState().setSettingsOpen(false)", false);
socket.close();
