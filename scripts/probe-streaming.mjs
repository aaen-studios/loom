// Sends a long request and prints the tail of the rendered transcript over
// time, which shows directly whether text appears progressively.
const port = process.argv[2] ?? "9333";
const prompt =
  process.argv[3] ??
  "List 12 facts about the ocean, one per line, each a full sentence.";

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
await new Promise((resolve) => socket.addEventListener("open", resolve));
await send("Runtime.enable");

const snapshot = `(() => {
  const text = document.body.innerText.replace(/\\s+/g, " ");
  return JSON.stringify({ length: text.length, tail: text.slice(-70) });
})()`;

const before = JSON.parse(await evaluate(snapshot));
await evaluate(`(() => {
  const ta = document.querySelector('textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, ${JSON.stringify(prompt)});
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
  return 'sent';
})()`);

const growth = [];
for (let tick = 0; tick < 40; tick += 1) {
  await sleep(800);
  const state = JSON.parse(await evaluate(snapshot));
  growth.push(state.length);
  console.log(`t+${((tick + 1) * 0.8).toFixed(1)}s len=${state.length} …${state.tail.slice(-50)}`);
  if (tick > 24 && state.length === growth[growth.length - 2]) break;
}

const distinct = new Set(growth.filter((value, index) => index === 0 || value !== growth[index - 1])).size;
console.log("");
console.log(
  distinct > 3
    ? `STREAMING OK: the transcript grew through ${distinct} distinct lengths`
    : `NOT STREAMING: ${distinct} distinct lengths (text appeared in one chunk)`,
);
socket.close();
