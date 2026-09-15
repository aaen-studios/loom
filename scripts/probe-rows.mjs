const port = process.argv[2] ?? "9333";
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
  if (result?.exceptionDetails) return `EXC: ${result.exceptionDetails.text}`;
  return result?.result?.value ?? "undefined";
};
await new Promise((resolve) => socket.addEventListener("open", resolve));
await send("Runtime.enable");

console.log("groups:", await evaluate(`document.querySelectorAll(".group").length`));
console.log("message bodies:", await evaluate(`document.querySelectorAll(".message-body").length`));
console.log(
  "row button titles:",
  await evaluate(`
    [...document.querySelectorAll(".group")]
      .map((row) => [...row.querySelectorAll("button")].map((b) => b.title).filter(Boolean).join("/"))
      .join(" || ")
  `),
);
console.log("active session:", await evaluate(`window.__loom.chat.getState().activeId`));
console.log("messages in store:", await evaluate(`window.__loom.chat.getState().messages.length`));
socket.close();
