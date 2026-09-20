// Temporary: the browser page, end to end, against the real runtime.
//
// Run with the app started under the DevTools protocol:
//
//   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9333"
//   .\target\debug\loom.exe
//   node scripts/probe-browser.mjs 9333
//
// It drives the real UI — opens the browser panel from the composer's chip,
// types a URL, waits, and reports what the panel and the shell actually say —
// so a wiring fault reads as a fault rather than as "nothing happened".
const port = process.argv[2] ?? "9333";
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = targets.find((t) => t.type === "page" && (t.title ?? "").includes("Loom"));
if (!page) {
  console.error("no Loom page on port " + port);
  process.exit(1);
}

const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
socket.addEventListener("message", (message) => {
  const payload = JSON.parse(message.data);
  if (payload.id && pending.has(payload.id)) {
    pending.get(payload.id)(payload.result ?? payload.error);
    pending.delete(payload.id);
  }
});
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const id = nextId++;
    pending.set(id, resolve);
    socket.send(JSON.stringify({ id, method, params }));
  });
const evaluate = async (expression) => {
  const result = await send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (result?.exceptionDetails) return `EXC: ${result.exceptionDetails.text}`;
  return result?.result?.value ?? "undefined";
};

await new Promise((resolve) => socket.addEventListener("open", resolve));
await send("Runtime.enable");

/** What the panel and the shell between them believe. */
const state = `(async () => {
  const store = window.__loom?.browser?.getState?.();
  const tabs = store?.tabs ?? [];
  const wire = await window.__loom.ipc.browserTabs();
  const slot = document.querySelector("[data-browser-panel] [data-browser-scroll] > div");
  return JSON.stringify({
    panelMounted: !!document.querySelector("[data-browser-panel]"),
    storeTabs: tabs.map((t) => ({ id: t.id, url: t.url, host: t.host, active: t.active })),
    shellTabs: (wire?.tabs ?? []).map((t) => ({ id: t.id, url: t.url, host: t.host, active: t.active })),
    slotRect: slot
      ? (() => { const r = slot.getBoundingClientRect(); return { x: Math.round(r.left), y: Math.round(r.top), w: Math.round(r.width), h: Math.round(r.height) }; })()
      : null,
    host: store?.host ?? null,
  });
})()`;

if (!(await evaluate("!!window.__loom"))) {
  console.error("window.__loom is not exposed — set localStorage loomDebug=1 and reload");
  process.exit(1);
}

console.log("before :", await evaluate(state));

// Open the browser panel the way the composer chip does.
console.log(
  "opening:",
  await evaluate(`(() => {
    const dock = window.__loom.dock.getState();
    dock.openPanel("browser", "right");
    return window.__loom.browser.getState().load().then(() => "panel opened");
  })()`),
);
await sleep(900);
console.log("panel  :", await evaluate(state));

// Type into the omnibox and press Enter, as a user would.
console.log(
  "navigate:",
  await evaluate(`(() => {
    const input = document.querySelector("[data-browser-panel] input[placeholder^='Search']");
    if (!input) return "no omnibox";
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
    setter.call(input, "example.com");
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    return "typed example.com";
  })()`),
);
await sleep(3500);
console.log("after  :", await evaluate(state));

// Ask the collector whether there is actually a live page behind it.
const ping = await evaluate(`(async () => {
  const tabs = window.__loom.browser.getState().tabs;
  if (!tabs.length) return "no tab to ping";
  const result = await window.__loom.ipc.browserPing(tabs[0].id);
  return JSON.stringify(result);
})()`);
console.log("ping   :", ping);

// And a screenshot, which is the one path that needs the COM surface.
const shot = await evaluate(`(async () => {
  const tabs = window.__loom.browser.getState().tabs;
  if (!tabs.length) return "no tab to capture";
  const result = await window.__loom.ipc.browserCall("probe", "browser_screenshot", {});
  const text = JSON.stringify(result);
  return text.length > 400 ? text.slice(0, 400) + "…" : text;
})()`);
console.log("shot   :", shot);

const { writeFileSync } = await import("node:fs");
const image = await send("Page.captureScreenshot", { format: "png" });
writeFileSync("target/browser-probe.png", Buffer.from(image.data, "base64"));
console.log("saved target/browser-probe.png");
socket.close();
