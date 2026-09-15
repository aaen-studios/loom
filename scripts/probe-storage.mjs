// Confirms the storage command returns real numbers.
const port = process.argv[2] ?? "9333";
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

console.log(
  "storage_usage:",
  JSON.stringify(await evaluate(`window.__TAURI_INTERNALS__.invoke("storage_usage")`)),
);

// Open Settings -> Data and read the rendered numbers.
await evaluate(`window.__loom.ui.getState().setSettingsOpen(true)`);
await new Promise((resolve) => setTimeout(resolve, 800));
await evaluate(`(() => {
  const button = [...document.querySelectorAll("nav button")].find((b) => b.textContent.trim() === "Data");
  button?.click();
  return "data category";
})()`);
await new Promise((resolve) => setTimeout(resolve, 900));
console.log(
  "settings text:",
  await evaluate(`(() => {
    const pane = document.querySelector("nav")?.parentElement;
    return pane ? pane.innerText.replace(/\\n+/g, " | ").slice(0, 260) : "no pane";
  })()`),
);
socket.close();
