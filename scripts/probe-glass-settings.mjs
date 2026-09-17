// Do the new Appearance sections actually render?
//
// 350 lines of new settings UI, and none of it is covered by the other probes —
// the structural gate only ever looks at the title bar and the composer. A
// missing import or a bad `Segmented` value would throw inside the drawer, and
// the ErrorBoundary would catch it *inside* the panel, so the title bar would
// still look perfect.
//
// Opens the drawer, switches to Appearance, and asserts the sections are in the
// DOM with their controls.
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
  console.error("no app page");
  process.exit(1);
}

const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
const errors = [];
socket.addEventListener("message", (m) => {
  const p = JSON.parse(m.data);
  if (p.id && pending.has(p.id)) {
    pending.get(p.id)(p.result ?? p.error);
    pending.delete(p.id);
    return;
  }
  if (p.method === "Runtime.exceptionThrown") {
    errors.push(p.params.exceptionDetails.text + " " + (p.params.exceptionDetails.exception?.description ?? ""));
  }
  if (p.method === "Runtime.consoleAPICalled" && p.params.type === "error") {
    errors.push(p.params.args.map((a) => a.value ?? a.description).join(" "));
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

// Fresh load: a module that failed to transform earlier leaves a broken graph
// that no later edit reaches, and this probe would then test stale code.
await send("Page.reload", { ignoreCache: true });
await sleep(4500);

for (let i = 0; i < 40; i += 1) {
  if ((await evaluate('document.querySelectorAll(".lg-stage").length', false)) > 0) break;
  await sleep(500);
}

// The drawer is driven from `window.__loom`, which `main.tsx` only exposes when
// `loomDebug` is set in localStorage. Set it and reload once, so the store is
// reachable.
const debugOn = await evaluate(`localStorage.getItem("loomDebug") === "1"`, false);
if (debugOn !== true) {
  console.log("enabling loomDebug and reloading…");
  await evaluate(`localStorage.setItem("loomDebug", "1")`, false);
  await send("Page.reload", { ignoreCache: true });
  await sleep(4500);
  for (let i = 0; i < 40; i += 1) {
    if ((await evaluate('document.querySelectorAll(".lg-stage").length', false)) > 0) break;
    await sleep(500);
  }
}

// There is no settings button in the title bar — the drawer is opened with
// Ctrl+, or from the empty state — so the honest way to drive it is the store
// the shortcut itself writes to. A probe that clicked a button which does not
// exist would report "no nav" and look like a UI failure.
console.log("opening Settings…");
const opened = await evaluate(`(() => {
  const store = window.__loom?.ui;
  if (!store) return "no ui store (is loomDebug set?)";
  store.getState().setSettingsOpen(true);
  store.getState().setSettingsCategory("appearance");
  return "opened via the store, on the appearance category";
})()`, false);
console.log("  " + opened);
await sleep(2000);

const found = await evaluate(`(() => {
  // \`textContent\`, not \`innerText\`. The section titles are drawn with
  // \`text-transform: uppercase\`, and \`innerText\` reports the text *as rendered*
  // — so a check for "Liquid glass" failed against "LIQUID GLASS" and reported
  // the section as missing while it was on screen the whole time. The headings
  // below are the source strings, which is what a check should assert on.
  const text = [...document.querySelectorAll("h3")].map((h) => h.textContent.trim()).join("\\n");
  const headings = [...document.querySelectorAll("h3")].map((h) => h.textContent.trim());
  const ranges = [...document.querySelectorAll('input[type="range"]')].map((r) => ({
    label: r.getAttribute("aria-label"),
    value: r.value,
    min: r.min,
    max: r.max,
  }));
  const switches = [...document.querySelectorAll('button[role="switch"]')].map((s) => ({
    label: (s.textContent ?? "").split("\\n")[0].trim().slice(0, 40),
    checked: s.getAttribute("aria-checked"),
    disabled: s.disabled,
  }));
  return JSON.stringify({
    hasGlass: text.includes("Tint opacity"),
    hasLiquid: text.includes("Liquid glass"),
    hasRefraction: text.includes("Refraction depth"),
    hasPreview: text.includes("Preview"),
    hasPatternToggle: text.includes("Hard edges"),
    headings,
    rangeCount: ranges.length,
    ranges,
    switchCount: switches.length,
    switches,
  }, null, 2);
})()`, false);

console.log("\n=== what is on screen ===");
console.log(found);

console.log(`\n=== console errors during this (${errors.length}) ===`);
for (const e of errors) console.log("  " + e);

// Leave the drawer how we found it, and the debug flag too — a probe should not
// change the state of the app it is measuring.
await evaluate(`(() => {
  window.__loom?.ui?.getState().setSettingsOpen(false);
  localStorage.removeItem("loomDebug");
  return true;
})()`, false);

socket.close();
