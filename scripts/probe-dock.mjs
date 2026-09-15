// Checks that pinning docks the sidebar (in layout, canvas narrows) and that
// dragging the divider resizes it.
const port = process.argv[2] ?? "9333";
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = targets.find((t) => t.type === "page" && (t.title ?? "").includes("Loom"));
if (!page) {
  console.error("no Loom page");
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
  const result = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result?.exceptionDetails) return `EXC: ${result.exceptionDetails.text}`;
  return result?.result?.value ?? "undefined";
};
await new Promise((resolve) => socket.addEventListener("open", resolve));
await send("Runtime.enable");

const layout = `(() => {
  const aside = document.querySelector("aside");
  const panel = document.querySelector(".panel.relative");
  return JSON.stringify({
    pinned: window.__loom.settings.getState().config.interface.sidebarPinned,
    width: window.__loom.settings.getState().config.interface.sidebarWidth,
    position: aside ? getComputedStyle(aside).position : null,
    asideWidth: aside ? Math.round(aside.getBoundingClientRect().width) : null,
    chatWidth: panel ? Math.round(panel.getBoundingClientRect().width) : null,
    divider: !!document.querySelector('button[aria-label="Resize chats"]')
  });
})()`;

console.log("floating:", await evaluate(layout));
await evaluate(`(() => {
  const button = [...document.querySelectorAll("aside button")].find((b) => (b.getAttribute("aria-label") || "").includes("Pin"));
  button?.click();
  return "pinned";
})()`);
await sleep(900);
console.log("docked  :", await evaluate(layout));

// Drag the divider 80px to the right.
console.log(
  "drag divider:",
  await evaluate(`(() => {
    const handle = document.querySelector('button[aria-label="Resize chats"]');
    if (!handle) return "no handle";
    const rect = handle.getBoundingClientRect();
    const x = rect.left + 1;
    const y = rect.top + 200;
    handle.dispatchEvent(new PointerEvent("pointerdown", { clientX: x, clientY: y, bubbles: true }));
    window.dispatchEvent(new PointerEvent("pointermove", { clientX: x + 80, clientY: y, bubbles: true }));
    window.dispatchEvent(new PointerEvent("pointerup", { clientX: x + 80, clientY: y, bubbles: true }));
    return "dragged";
  })()`),
);
await sleep(1200);
console.log("after   :", await evaluate(layout));

const { writeFileSync } = await import("node:fs");
const shot = await send("Page.captureScreenshot", { format: "png" });
writeFileSync("target/docked.png", Buffer.from(shot.data, "base64"));
console.log("saved target/docked.png");

// Leave the app as it was: floating, default width.
await evaluate(`(() => {
  const state = window.__loom.settings.getState();
  return window.__TAURI_INTERNALS__.invoke("set_interface_settings", {
    interface: { ...state.config.interface, sidebarPinned: false, sidebarWidth: 264 },
  }).then((updated) => state.applyRemote(updated));
})()`);
socket.close();
