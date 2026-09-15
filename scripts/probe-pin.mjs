const port = process.argv[2] ?? "9333";
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = targets.find((t) => t.type === "page" && (t.title ?? "").includes("Loom"));
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
  const result = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result?.exceptionDetails) return `EXCEPTION: ${result.exceptionDetails.text}`;
  return result?.result?.value ?? "";
};
await new Promise((resolve) => socket.addEventListener("open", resolve));
await send("Runtime.enable");

const state = `JSON.stringify({
  sidebarOpen: window.__loom.ui.getState().sidebarOpen,
  pinned: window.__loom.settings.getState().config.interface.sidebarPinned,
  aside: !!document.querySelector("aside"),
  backdrops: document.querySelectorAll(".absolute.inset-0.cursor-default").length
})`;

console.log("before:", await evaluate(state));
await evaluate(`window.__loom.ui.getState().setSidebarOpen(true)`);
await sleep(500);
console.log("opened:", await evaluate(state));
console.log(
  "pin clicked:",
  await evaluate(`(() => {
    const button = [...document.querySelectorAll("aside button")].find((b) => (b.getAttribute("aria-label") || "").includes("Pin"));
    if (!button) return "missing";
    button.click();
    return "clicked";
  })()`),
);
await sleep(1200);
console.log("after pin:", await evaluate(state));

console.log(
  "unpin clicked:",
  await evaluate(`(() => {
    const button = [...document.querySelectorAll("aside button")].find((b) => (b.getAttribute("aria-label") || "").includes("Pin") || (b.getAttribute("aria-label") || "").includes("Unpin"));
    if (!button) return "missing";
    button.click();
    return "clicked";
  })()`),
);
await sleep(1200);
console.log("after unpin:", await evaluate(state));
socket.close();
