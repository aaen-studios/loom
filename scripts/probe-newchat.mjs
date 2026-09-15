// Creates a new chat, sends a question, and reports streaming progress plus
// whether the chat gets an automatic title.
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
await new Promise((resolve) => socket.addEventListener("open", resolve));
await send("Runtime.enable");
await evaluate(`localStorage.setItem("loomDebug","1")`);

await evaluate(`window.__loom.chat.getState().newSession()`);
await sleep(1500);

await evaluate(`(() => {
  const ta = document.querySelector('textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, "What is the capital of Norway, and one fact about it?");
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
  return "sent";
})()`);

const lengths = [];
for (let tick = 0; tick < 22; tick += 1) {
  await sleep(900);
  const raw = await evaluate(
    `JSON.stringify({
       title: window.__loom.chat.getState().sessions[0]?.title ?? "",
       busy: !!document.querySelector('button[title="Stop generating"]'),
       length: document.body.innerText.replace(/\\s+/g, " ").length
     })`,
  );
  const state = JSON.parse(raw);
  lengths.push(state.length);
  console.log(
    `t+${((tick + 1) * 0.9).toFixed(1)}s title="${state.title}" busy=${state.busy} chars=${state.length}`,
  );
}

// Streaming means the transcript grew through several steps, not that many
// distinct lengths were sampled (short replies finish between samples).
let increases = 0;
for (let index = 1; index < lengths.length; index += 1) {
  if (lengths[index] > lengths[index - 1]) increases += 1;
}
console.log("");
console.log(
  increases >= 2
    ? `STREAMING: transcript grew in ${increases} steps`
    : `NOT STREAMING: ${increases} growth step(s)`,
);
console.log("final title:", await evaluate(`window.__loom.chat.getState().sessions[0]?.title ?? ""`));
socket.close();
