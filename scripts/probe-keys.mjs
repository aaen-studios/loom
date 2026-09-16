// Drives the keyboard navigation: Alt+Arrow selection, C to copy, ? for the
// shortcut sheet.
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
  if (result?.exceptionDetails) return `EXCEPTION: ${result.exceptionDetails.text}`;
  return result?.result?.value ?? "";
};
await new Promise((resolve) => socket.addEventListener("open", resolve));
await send("Runtime.enable");

const press = (key, options = {}) =>
  evaluate(`(() => {
    const event = new KeyboardEvent("keydown", { key: ${JSON.stringify(key)}, bubbles: true, cancelable: true, ...${JSON.stringify(options)} });
    window.dispatchEvent(event);
    return "sent";
  })()`);

const selection = `(() => {
  const ring = document.querySelector("[data-message-id].ring-1");
  return ring ? ring.getAttribute("data-message-id") : null;
})()`;

console.log("selected before:", await evaluate(selection));
await press("ArrowDown", { altKey: true });
await sleep(400);
console.log("after Alt+Down:", await evaluate(selection));
await press("ArrowDown", { altKey: true });
await sleep(400);
console.log("after Alt+Down again:", await evaluate(selection));
await press("ArrowUp", { altKey: true });
await sleep(400);
console.log("after Alt+Up:", await evaluate(selection));

await press("Escape");
await sleep(300);
console.log("after Escape:", await evaluate(selection));

await press("?");
await sleep(600);
console.log(
  "shortcut sheet:",
  await evaluate(`/Keyboard shortcuts/.test(document.body.innerText) ? "visible" : "missing"`),
);
console.log(
  "sheet lists navigation:",
  await evaluate(`/Select the previous/.test(document.body.innerText) ? "yes" : "no"`),
);
const { writeFileSync } = await import("node:fs");
const shot = await send("Page.captureScreenshot", { format: "png" });
writeFileSync("target/shortcuts.png", Buffer.from(shot.data, "base64"));
console.log("saved target/shortcuts.png");

await press("Escape");
await sleep(400);
console.log(
  "sheet closed:",
  await evaluate(`/Keyboard shortcuts/.test(document.body.innerText) ? "still open" : "closed"`),
);
socket.close();
