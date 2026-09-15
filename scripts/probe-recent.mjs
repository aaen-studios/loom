// Confirms the "Recent" group in the model picker.
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

console.log(
  "recent models stored:",
  await evaluate(`JSON.stringify(window.__loom.settings.getState().config.chat.recentModels)`),
);

console.log(
  "open picker:",
  await evaluate(`(() => {
    const chip = [...document.querySelectorAll("button")].find((b) => /deepseek|Select model|glm|gpt/i.test(b.textContent));
    if (!chip) return "chip not found";
    chip.click();
    return "clicked";
  })()`),
);
await sleep(800);
console.log(
  "picker contents:",
  await evaluate(`(() => {
    const panel = document.querySelector(".panel-strong.absolute.bottom-full");
    return panel ? panel.innerText.replace(/\\n+/g, " | ").slice(0, 220) : "picker not open";
  })()`),
);
console.log(
  "Recent heading present:",
  await evaluate(`(() => {
    const panel = document.querySelector(".panel-strong.absolute.bottom-full");
    return panel ? /Recent/.test(panel.innerText) : "no panel";
  })()`),
);
socket.close();
