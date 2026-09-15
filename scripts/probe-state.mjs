// Inspects real store state before, during and after a turn.
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
const logs = [];
socket.addEventListener("message", (message) => {
  const payload = JSON.parse(message.data);
  if (payload.id && pending.has(payload.id)) {
    pending.get(payload.id)(payload.result ?? payload.error);
    pending.delete(payload.id);
    return;
  }
  if (payload.method === "Runtime.consoleAPICalled") {
    const text = payload.params.args.map((a) => a.value ?? a.description ?? a.type).join(" ");
    logs.push(`${payload.params.type}: ${text}`);
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
await evaluate(`localStorage.setItem("loomDebug","1")`);
await send("Page.enable");
await send("Page.reload", { ignoreCache: false });
await sleep(6000);

const state = `(() => {
  const chat = window.__loom?.chat;
  if (!chat) return "stores not exposed";
  const s = chat.getState();
  const active = s.activeId;
  return JSON.stringify({
    activeId: active,
    busy: s.busy,
    busyForActive: !!(active && s.busy[active]),
    stopButtonInDom: !!document.querySelector('button[title="Stop generating"]'),
    live: Object.keys(s.live),
    messages: s.messages.map((m) => ({ role: m.role, len: m.content.length })),
    lastText: document.body.innerText.replace(/\\s+/g, " ").slice(-60)
  });
})()`;

console.log("before:", await evaluate(state));
await evaluate(`(() => {
  const ta = document.querySelector('textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, "Name three colours, one word each.");
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
  return "sent";
})()`);

for (let tick = 0; tick < 8; tick += 1) {
  await sleep(2500);
  console.log(`t+${((tick + 1) * 2.5).toFixed(1)}s`, await evaluate(state));
}
console.log("");
console.log("--- all frontend console output ---"); for (const line of logs.slice(0, 30)) console.log("  " + line);
socket.close();
