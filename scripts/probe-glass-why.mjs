// Why do the Appearance glass sliders and the preview appear to do nothing?
//
// "Don't work" has four distinct causes and they have to be separated before
// anything is changed:
//
//   1. the slider moves but the store does not update
//   2. the store updates but the rendered filter does not change
//   3. the filter changes but the pixels do not — the 164px-blur class of bug
//   4. everything works and the drawer simply hides the thing it controls
//
// Written with flat strings and no nested template literals: the first version
// of this file nested one inside a call and Node refused to parse it, which is
// a silly way to lose a diagnosis.
const port = process.argv[2] ?? "9333";
const category = process.argv[3] ?? "appearance";
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

/* ------------------------------------------------------------- open the drawer */

await evaluate('localStorage.setItem("loomDebug","1")', false);
await send("Page.reload", { ignoreCache: true });
await sleep(4500);
for (let i = 0; i < 40; i += 1) {
  if ((await evaluate('document.querySelectorAll(".lg-stage").length', false)) > 0) break;
  await sleep(500);
}

const closedCount = await evaluate('document.querySelectorAll(".lg-stage").length', false);
console.log("liquid surfaces with the drawer CLOSED: " + closedCount);

const OPEN = [
  "(() => {",
  "  const ui = window.__loom.ui.getState();",
  "  ui.setSettingsOpen(true);",
  "  ui.setSettingsCategory(" + JSON.stringify(category) + ");",
  "  return true;",
  "})()",
].join("\n");
await evaluate(OPEN, false);
await sleep(2000);

const openCount = await evaluate('document.querySelectorAll(".lg-stage").length', false);
console.log("liquid surfaces with the drawer OPEN:   " + openCount);
console.log("  (4 pills + 1 composer = 5; the preview should add 2 more)\n");

/* ------------------------------------------------- 1. the drawer's own styling */

const DRAWER = [
  "(() => {",
  "  const drawer = [...document.querySelectorAll('div')].find(",
  "    (d) => typeof d.className === 'string' && d.className.indexOf('w-[720px]') >= 0);",
  "  if (!drawer) return JSON.stringify({ error: 'no drawer element' });",
  "  const s = getComputedStyle(drawer);",
  "  const stages = [...drawer.querySelectorAll('.lg-stage')];",
  "  return JSON.stringify({",
  "    backdropFilter: s.backdropFilter,",
  "    filter: s.filter,",
  "    transform: s.transform,",
  "    opacity: s.opacity,",
  "    animationName: s.animationName,",
  "    willChange: s.willChange,",
  "    contain: s.contain,",
  "    stagesInDrawer: stages.length,",
  "    stages: stages.map((st) => {",
  "      const warp = st.querySelector('.glass__warp');",
  "      const ws = warp ? getComputedStyle(warp) : null;",
  "      const b = st.getBoundingClientRect();",
  "      const disp = st.querySelector('feDisplacementMap');",
  "      const filterEl = st.querySelector('filter');",
  "      return {",
  "        box: [Math.round(b.width), Math.round(b.height)].join('x'),",
  "        visible: b.width > 0 && b.height > 0,",
  "        warpFilter: ws ? ws.filter : null,",
  "        warpBackdrop: ws ? ws.backdropFilter : null,",
  "        displacementScale: disp ? disp.getAttribute('scale') : null,",
  "        filterIdResolves: filterEl ? !!document.getElementById(filterEl.id) : null,",
  "        tintBg: (function () {",
  "          const t = st.querySelector('.lg-tint');",
  "          return t ? getComputedStyle(t).backgroundColor : null;",
  "        })(),",
  "      };",
  "    }),",
  "  }, null, 2);",
  "})()",
].join("\n");

console.log("=== 1. the drawer, and every liquid surface inside it ===");
console.log(await evaluate(DRAWER, false));

/* -------------------------------------- 2. does the slider reach the store at all? */

const SLIDER_LABEL = "Refraction depth";

const READ = [
  "(() => {",
  "  const input = document.querySelector('input[aria-label=" + JSON.stringify(SLIDER_LABEL) + "]');",
  "  const disp = document.querySelector('feDisplacementMap');",
  "  const liquid = window.__loom.settings.getState().config.interface.glass.liquid;",
  "  return JSON.stringify({",
  "    sliderPresent: !!input,",
  "    sliderValue: input ? input.value : null,",
  "    storeRefraction: liquid.refraction,",
  "    storeFrost: liquid.frost,",
  "    domDisplacementScale: disp ? disp.getAttribute('scale') : null,",
  "  });",
  "})()",
].join("\n");

console.log("\n=== 2. does a slider move reach the store and the DOM? ===");
console.log("  before: " + (await evaluate(READ, false)));

// React listens for its own synthetic event, so writing `.value` alone is
// invisible to it. The native setter plus a dispatched `input` is the same thing
// a real drag produces.
const MOVE = [
  "(() => {",
  "  const input = document.querySelector('input[aria-label=" + JSON.stringify(SLIDER_LABEL) + "]');",
  "  if (!input) return 'no slider found';",
  "  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;",
  "  setter.call(input, '110');",
  "  input.dispatchEvent(new Event('input', { bubbles: true }));",
  "  input.dispatchEvent(new Event('change', { bubbles: true }));",
  "  return 'set to 110';",
  "})()",
].join("\n");
console.log("  move:   " + (await evaluate(MOVE, false)));
await sleep(1300);
console.log("  after:  " + (await evaluate(READ, false)));

/* --------------------------------- 3. do the pixels change when the filter differs? */

const REGION = [
  "(() => {",
  "  const drawer = [...document.querySelectorAll('div')].find(",
  "    (d) => typeof d.className === 'string' && d.className.indexOf('w-[720px]') >= 0);",
  "  if (!drawer) return '';",
  "  const stage = drawer.querySelector('.lg-stage');",
  "  if (!stage) return '';",
  "  const b = stage.getBoundingClientRect();",
  "  return JSON.stringify({",
  "    x: Math.max(0, Math.floor(b.x) - 10),",
  "    y: Math.max(0, Math.floor(b.y) - 10),",
  "    width: Math.ceil(b.width) + 20,",
  "    height: Math.ceil(b.height) + 20,",
  "    scale: 4,",
  "  });",
  "})()",
].join("\n");

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
  "  let differing = 0;",
  "  let maxDelta = 0;",
  "  const total = A.length / 4;",
  "  for (let i = 0; i < A.length; i += 4) {",
  "    const d = Math.max(Math.abs(A[i] - B[i]), Math.abs(A[i + 1] - B[i + 1]), Math.abs(A[i + 2] - B[i + 2]));",
  "    if (d > 2) differing += 1;",
  "    if (d > maxDelta) maxDelta = d;",
  "  }",
  "  return JSON.stringify({",
  "    percentDiffering: +((differing / total) * 100).toFixed(2),",
  "    maxChannelDelta: maxDelta,",
  "  });",
  "})",
].join("\n");

console.log("\n=== 3. does the preview's filter change its pixels? ===");
const region = await evaluate(REGION, false);
if (!region) {
  console.log("  no stage in the drawer to measure");
} else {
  console.log("  region: " + region);
  const clip = JSON.parse(region);
  const shotA = await send("Page.captureScreenshot", { format: "png", clip });
  await evaluate(
    "(() => { const e = document.createElement('style'); e.id = 'lg-why-off';" +
      " e.textContent = '.lg-shell .glass__warp { filter: none !important; }';" +
      " document.head.appendChild(e); return true; })()",
    false,
  );
  await sleep(700);
  const shotB = await send("Page.captureScreenshot", { format: "png", clip });
  await evaluate("document.getElementById('lg-why-off')?.remove()", false);
  if (shotA?.data && shotB?.data) {
    const raw = await evaluate(
      "(" + DIFF + ")(" + JSON.stringify(shotA.data) + ", " + JSON.stringify(shotB.data) + ")",
    );
    console.log("  filter on vs off: " + raw);
  } else {
    console.log("  capture failed");
  }
}

/* ------------------------------------ 4. do the app-wide variables reach the CSS? */

const VARS = [
  "(() => {",
  "  const root = getComputedStyle(document.documentElement);",
  "  const pill = document.querySelector('.pill');",
  "  const thin = document.querySelector('.glass-thin');",
  "  return JSON.stringify({",
  "    rootGlassTint: root.getPropertyValue('--glass-tint').trim(),",
  "    rootGlassBlur: root.getPropertyValue('--glass-blur').trim(),",
  "    pillBackdrop: pill ? getComputedStyle(pill).backdropFilter : null,",
  "    pillBackground: pill ? getComputedStyle(pill).backgroundColor : null,",
  "    thinBackdrop: thin ? getComputedStyle(thin).backdropFilter : null,",
  "    thinPresent: !!thin,",
  "  }, null, 2);",
  "})()",
].join("\n");

console.log("\n=== 4. do the app-wide multipliers reach the surfaces? ===");
console.log(await evaluate(VARS, false));

await evaluate("window.__loom.ui.getState().setSettingsOpen(false)", false);
socket.close();
