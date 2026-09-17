// Verifies the Appearance split, and that the glass controls actually move
// something. Written to be run repeatedly against a long-lived page, so it
// starts by cleaning up after any earlier probe.
//
// Four things are checked:
//
//   1. every nav entry renders, and the four "look" categories show only their
//      own sections — a split that leaves sections in two tabs is worse than no
//      split at all
//   2. the app-wide multipliers are actually written to <html>
//   3. moving the Tint slider changes the refracting surfaces' tint, not only
//      the plain ones (this was half-missing until now)
//   4. the preview's own pixels change when its filter changes
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

// Clean up after earlier probes. `#lg-freeze` pauses every animation, and one
// left behind by a previous run makes the drawer read `opacity: 0` forever,
// which looks exactly like a broken animation in the app. The page is long-lived
// across probe runs, so this is not hypothetical.
const cleaned = await evaluate(`(() => {
  const ids = ["lg-freeze", "lg-probe-off", "lg-why-off", "lg-cost", "lg-ab-off", "lg-pattern", "lg-test-backdrop"];
  const found = ids.filter((id) => document.getElementById(id));
  found.forEach((id) => document.getElementById(id)?.remove());
  return JSON.stringify(found);
})()`, false);
console.log("stale probe overlays removed: " + cleaned + "\n");

/* ------------------------------------------------ 1. each category's sections */

const CATEGORIES = ["general", "appearance", "glass", "background", "layout"];

/** The section `<h3>`s inside the drawer, in order. */
const SECTIONS = [
  "(() => {",
  "  const drawer = [...document.querySelectorAll('div')].find(",
  "    (d) => typeof d.className === 'string' && d.className.indexOf('w-[720px]') >= 0);",
  "  if (!drawer) return JSON.stringify({ error: 'no drawer' });",
  "  const body = drawer.querySelector('div[class*=\"overflow-y-auto\"]');",
  "  const headings = [...(body ?? drawer).querySelectorAll('h3')].map((h) => h.textContent.trim());",
  "  const style = getComputedStyle(drawer);",
  "  return JSON.stringify({",
  "    heading: headings[0] ?? null,",
  "    sections: headings.slice(1),",
  "    opacity: style.opacity,",
  "    animationPlayState: style.animationPlayState,",
  "  });",
  "})()",
].join("\n");

console.log("=== 1. what each nav entry shows ===");
const seen = {};
for (const category of CATEGORIES) {
  await evaluate(
    "(() => { const u = window.__loom.ui.getState(); u.setSettingsOpen(true); u.setSettingsCategory(" +
      JSON.stringify(category) +
      "); return true; })()",
    false,
  );
  await sleep(700);
  const raw = await evaluate(SECTIONS, false);
  let parsed = null;
  try {
    parsed = JSON.parse(raw);
  } catch {
    /* reported below */
  }
  seen[category] = parsed;
  console.log(
    "  " +
      category.padEnd(11) +
      (parsed ? parsed.heading + " — " + (parsed.sections.join(", ") || "(no sections)") : raw),
  );
}

/* -------------------------------- 2. the multipliers, and the nav entry count */

const NAV = [
  "(() => {",
  "  const drawer = [...document.querySelectorAll('div')].find(",
  "    (d) => typeof d.className === 'string' && d.className.indexOf('w-[720px]') >= 0);",
  "  const nav = drawer ? drawer.querySelector('nav') : null;",
  "  const groups = nav ? [...nav.querySelectorAll('div[class*=\"pt-3\"]')] : [];",
  "  const root = getComputedStyle(document.documentElement);",
  "  return JSON.stringify({",
  "    navGroups: groups.map((g) => ({",
  "      label: (g.querySelector('p')?.textContent ?? '').trim(),",
  "      entries: [...g.querySelectorAll('button')].map((b) => b.textContent.trim()),",
  "    })),",
  "    glassTint: root.getPropertyValue('--glass-tint').trim(),",
  "    glassBlur: root.getPropertyValue('--glass-blur').trim(),",
  "  }, null, 2);",
  "})()",
].join("\n");

console.log("\n=== 2. the nav, and the app-wide multipliers ===");
console.log(await evaluate(NAV, false));

/* -------------------------- 3. does the Tint slider move the refracting tints? */

const TINTS = [
  "(() => {",
  "  const tints = [...document.querySelectorAll('.lg-tint')].map((t) => getComputedStyle(t).backgroundColor);",
  "  const store = window.__loom.settings.getState().config.interface.glass;",
  "  return JSON.stringify({ storeTint: store.tint, count: tints.length, tints: tints.slice(0, 3) });",
  "})()",
].join("\n");

const SET_TINT = (value) =>
  [
    "(() => {",
    "  const input = document.querySelector('input[aria-label=" + JSON.stringify("Glass tint opacity") + "]');",
    "  if (!input) return 'no tint slider on this page';",
    "  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;",
    "  setter.call(input, " + JSON.stringify(String(value)) + ");",
    "  input.dispatchEvent(new Event('input', { bubbles: true }));",
    "  return 'set to " + value + "';",
    "})()",
  ].join("\n");

// Back on the glass page, where the tint slider lives.
await evaluate(
  "(() => { const u = window.__loom.ui.getState(); u.setSettingsCategory('glass'); return true; })()",
  false,
);
await sleep(800);

console.log("\n=== 3. does Tint move the refracting surfaces? ===");
console.log("  at 100: " + (await evaluate(TINTS, false)));
console.log("  " + (await evaluate(SET_TINT(60), false)));
await sleep(900);
console.log("  at 60:  " + (await evaluate(TINTS, false)));
await evaluate(SET_TINT(100), false);
await sleep(700);
console.log("  back:   " + (await evaluate(TINTS, false)));

/* ------------------------------------------- 4. the preview's own pixel diff */

const REGION = [
  "(() => {",
  "  const drawer = [...document.querySelectorAll('div')].find(",
  "    (d) => typeof d.className === 'string' && d.className.indexOf('w-[720px]') >= 0);",
  "  const stage = drawer ? drawer.querySelector('.lg-stage') : null;",
  "  if (!stage) return '';",
  "  const b = stage.getBoundingClientRect();",
  "  return JSON.stringify({",
  "    x: Math.max(0, Math.floor(b.x) - 8),",
  "    y: Math.max(0, Math.floor(b.y) - 8),",
  "    width: Math.ceil(b.width) + 16,",
  "    height: Math.ceil(b.height) + 16,",
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
  "  let differing = 0; let maxDelta = 0;",
  "  const total = A.length / 4;",
  "  for (let i = 0; i < A.length; i += 4) {",
  "    const d = Math.max(Math.abs(A[i] - B[i]), Math.abs(A[i+1] - B[i+1]), Math.abs(A[i+2] - B[i+2]));",
  "    if (d > 2) differing += 1;",
  "    if (d > maxDelta) maxDelta = d;",
  "  }",
  "  return JSON.stringify({ percentDiffering: +((differing / total) * 100).toFixed(2), maxChannelDelta: maxDelta });",
  "})",
].join("\n");

console.log("\n=== 4. the preview's pixels ===");
const region = await evaluate(REGION, false);
if (!region) {
  console.log("  no preview on this page");
} else {
  const clip = JSON.parse(region);
  const a = await send("Page.captureScreenshot", { format: "png", clip });
  await evaluate(
    "(() => { const e = document.createElement('style'); e.id = 'verify-off';" +
      " e.textContent = '.lg-shell .glass__warp { filter: none !important; }';" +
      " document.head.appendChild(e); return true; })()",
    false,
  );
  await sleep(700);
  const b = await send("Page.captureScreenshot", { format: "png", clip });
  await evaluate("document.getElementById('verify-off')?.remove()", false);
  if (a?.data && b?.data) {
    console.log("  filter on vs off: " + (await evaluate(
      "(" + DIFF + ")(" + JSON.stringify(a.data) + ", " + JSON.stringify(b.data) + ")",
    )));
  } else {
    console.log("  capture failed");
  }
}

await evaluate("window.__loom.ui.getState().setSettingsOpen(false)", false);
socket.close();
