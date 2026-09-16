// Drives a real computer-use turn through the UI: arms the Computer chip,
// asks for a small screenshot and the cursor position, and reports per-step
// timings plus what the transcript received. Requires the app running with
// WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9333".
//
// Usage: node scripts/probe-computer.mjs 9333 [region]
const port = process.argv[2] ?? "9333";
const region = process.argv[3] ?? "400x300";
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
    const text = payload.params.args
      .map((a) => a.value ?? a.description ?? a.type)
      .join(" ");
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
  const result = await send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (result?.exceptionDetails) return `EXCEPTION: ${result.exceptionDetails.text}`;
  return result?.result?.value ?? "";
};

await new Promise((resolve) => socket.addEventListener("open", resolve));
await send("Runtime.enable");
await evaluate(`localStorage.setItem("loomDebug","1")`);
await send("Page.enable");
await send("Page.reload", { ignoreCache: false });
await sleep(6000);

// Arm the chip on the active chat, creating one if needed.
console.log(
  "arm:",
  await evaluate(`(async () => {
    const chat = window.__loom?.chat;
    if (!chat) return "stores not exposed";
    let state = chat.getState();
    if (!state.activeId) await state.ensureSession();
    state = chat.getState();
    await state.setComputerAccess(true);
    const providers = window.__loom.providers.getState();
    const settings = window.__loom.settings.getState();
    const session = state.sessions.find((s) => s.id === state.activeId);
    const ref = session?.modelId
      ? { providerId: session.providerId, modelId: session.modelId }
      : { providerId: settings.config.chat.providerId, modelId: settings.config.chat.modelId };
    const entry = providers.models.find(
      (m) => m.providerId === ref.providerId && m.modelId === ref.modelId,
    );
    const vision = !!entry?.spec.inputModalities?.includes("image");
    return JSON.stringify({ session: state.activeId, model: ref.modelId, vision });
  })()`),
);

const [width, height] = region.split("x").map(Number);
const prompt = `Computer check. Call screenshot with target "region" and region {x:0,y:0,width:${width},height:${height}}. Then call mouse with action "position". Then reply with the cursor coordinates and the screenshot size. Two tool calls only.`;

await evaluate(`(() => {
  const ta = document.querySelector('textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, ${JSON.stringify(prompt)});
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
  return "sent";
})()`);

const snapshot = `(() => {
  const chat = window.__loom?.chat;
  if (!chat) return "stores not exposed";
  const s = chat.getState();
  const active = s.activeId;
  const live = Object.values(s.liveTools ?? {}).flat();
  const stored = s.messages
    .map((m) => {
      try { return JSON.parse(m.extra ?? "null")?.toolCalls ?? []; } catch { return []; }
    })
    .flat();
  const calls = [...stored, ...live].filter((c) => c.name);
  const shots = calls.filter((c) => (c.images ?? []).length > 0);
  return JSON.stringify({
    busy: !!(active && s.busy[active]),
    calls: calls.length,
    names: calls.map((c) => c.name).join(","),
    screenshotImages: shots.reduce((n, c) => n + (c.images ?? []).length, 0),
    last: calls.length ? (calls[calls.length - 1].output ?? "").slice(0, 120) : "",
  });
})()`;

const started = Date.now();
for (let tick = 0; tick < 20; tick += 1) {
  await sleep(2000);
  const state = await evaluate(snapshot);
  console.log(`t+${((Date.now() - started) / 1000).toFixed(1)}s`, state);
  try {
    if (JSON.parse(state).screenshotImages > 0 && !JSON.parse(state).busy) break;
  } catch {
    // keep polling
  }
}

console.log("");
console.log("--- frontend event log (tail) ---");
for (const line of logs.slice(-25)) console.log("  " + line);
socket.close();
