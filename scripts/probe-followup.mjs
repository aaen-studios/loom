// Opens a chat that has stored thinking and sends a follow-up, which is what
// the gateway rejected before the reasoning_content fix.
const port = process.argv[2] ?? "9333";
const sessionId = process.argv[3];
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

await evaluate(`window.__loom.chat.getState().openSession(${JSON.stringify(sessionId)})`);
await sleep(1800);
console.log(
  "opened:",
  await evaluate(
    `JSON.stringify({ active: window.__loom.chat.getState().activeId?.slice(0, 8), messages: window.__loom.chat.getState().messages.length })`,
  ),
);

await evaluate(`(() => {
  const ta = document.querySelector('textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, "Give me one more fact about it.");
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
  return "sent";
})()`);

for (let tick = 0; tick < 14; tick += 1) {
  await sleep(1500);
  const state = JSON.parse(
    await evaluate(
      `JSON.stringify({
         busy: !!document.querySelector('button[title="Stop generating"]'),
         failed: /This reply failed/.test(document.body.innerText),
         error: (window.__loom.chat.getState().error ?? "").slice(0, 80),
         chars: document.body.innerText.length
       })`,
    ),
  );
  console.log(
    `t+${((tick + 1) * 1.5).toFixed(1)}s busy=${state.busy} failed=${state.failed} chars=${state.chars}${state.error ? ` error="${state.error}"` : ""}`,
  );
  if (!state.busy && tick > 1) break;
}
socket.close();
