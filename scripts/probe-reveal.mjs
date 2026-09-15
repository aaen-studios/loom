// Verifies the "hidden" thinking mode and the per-message reveal action.
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

// Switch thinking to "hidden" in memory only (no config write).
await evaluate(`(() => {
  const state = window.__loom.settings.getState();
  state.applyRemote({
    ...state.config,
    interface: { ...state.config.interface, showThinking: "hidden" },
  });
  return "hidden";
})()`);
await sleep(900);

console.log("thinking text visible:", await evaluate(`/thinking/i.test(document.body.innerText)`));
console.log(
  "assistant action titles:",
  await evaluate(`(() => {
    const rows = [...document.querySelectorAll(".group")];
    for (const row of rows) {
      const titles = [...row.querySelectorAll("button")].map((b) => b.title).filter(Boolean);
      if (titles.some((t) => t.includes("thinking")) || titles.includes("Regenerate this reply")) {
        return titles.join(" | ");
      }
    }
    return "none";
  })()`),
);

console.log(
  "reveal click:",
  await evaluate(`(() => {
    for (const row of document.querySelectorAll(".group")) {
      const button = [...row.querySelectorAll("button")].find((b) => (b.title || "").startsWith("Show thinking"));
      if (button) { button.click(); return "clicked"; }
    }
    return "no reveal button found";
  })()`),
);
await sleep(700);
console.log("thinking text after reveal:", await evaluate(`/thinking/i.test(document.body.innerText)`));

// Restore what the config says.
await evaluate(`(() => {
  const state = window.__loom.settings.getState();
  return state.load();
})()`);
socket.close();
