// Captures screenshots of the app in each theme and in the empty state, without
// persisting any settings change.
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
  return result?.result?.value ?? "";
};
const shot = async (name) => {
  const result = await send("Page.captureScreenshot", { format: "png" });
  const { writeFileSync } = await import("node:fs");
  writeFileSync(name, Buffer.from(result.data, "base64"));
  console.log(`saved ${name}`);
};

await new Promise((resolve) => socket.addEventListener("open", resolve));
await send("Runtime.enable");
await send("Page.enable");

// Dark theme, chat view.
await evaluate(`document.documentElement.classList.add("dark")`);
await sleep(1200);
await shot("target/theme-dark.png");

// Light theme, chat view.
await evaluate(`document.documentElement.classList.remove("dark")`);
await sleep(1200);
await shot("target/theme-light.png");

// Empty state (new chat), light and dark.
await evaluate(`window.__loom.chat.getState().newSession()`);
await sleep(1500);
await shot("target/empty-light.png");
await evaluate(`document.documentElement.classList.add("dark")`);
await sleep(1000);
await shot("target/empty-dark.png");

socket.close();
