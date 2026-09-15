// Enables the frontend event log, sends a message, and captures both the
// console output and whether the busy state clears.
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
const received = [];
socket.addEventListener("message", (message) => {
  const payload = JSON.parse(message.data);
  if (payload.id && pending.has(payload.id)) {
    pending.get(payload.id)(payload.result ?? payload.error);
    pending.delete(payload.id);
    return;
  }
  if (payload.method === "Runtime.consoleAPICalled") {
    const text = payload.params.args.map((a) => a.value ?? a.description ?? a.type).join(" ");
    if (text.includes("[loom]")) received.push(`${payload.params.type}: ${text}`);
  }
  if (payload.method === "Runtime.exceptionThrown") {
    received.push(`EXCEPTION: ${payload.params.exceptionDetails.text}`);
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
await new Promise((resolve) => socket.addEventListener("open", resolve));
await send("Runtime.enable");

// Turn on the diagnostic log and reload so it takes effect.
await evaluate(`localStorage.setItem("loomDebug","1")`);
await send("Page.enable");
await send("Page.reload", { ignoreCache: false });
await sleep(6000);

console.log("busy before:", await evaluate(`document.querySelector('button[title="Stop generating"]') ? "yes" : "no"`));

await evaluate(`(() => {
  const ta = document.querySelector('textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, "Count to five, one short line each.");
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
  return "sent";
})()`);

// Watch the busy flag, since a stuck spinner is exactly the complaint.
for (let tick = 0; tick < 20; tick += 1) {
  await sleep(1200);
  const busy = await evaluate(`document.querySelector('button[title="Stop generating"]') ? "busy" : "idle"`);
  console.log(`t+${((tick + 1) * 1.2).toFixed(1)}s ${busy}`);
  if (tick > 2 && busy === "idle") break;
}

console.log("");
console.log(`frontend events received: ${received.length}`);
for (const line of received.slice(0, 25)) console.log("  " + line);
console.log("");
console.log(received.length > 0 ? "FRONTEND RECEIVES EVENTS" : "FRONTEND RECEIVES NOTHING");
socket.close();
