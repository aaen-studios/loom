// Sends a message through the real UI over the DevTools Protocol and reports
// what happened, so the whole pipeline is exercised end to end.
const port = process.argv[2] ?? "9333";
const text = process.argv[3] ?? "Reply with exactly: pong";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = targets.find((t) => t.type === "page" && (t.title ?? "").includes("Loom"));
if (!page) {
  console.error("no Loom page:", targets.map((t) => `${t.title} ${t.url}`));
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
  return result?.result?.value ?? JSON.stringify(result);
};

await new Promise((resolve) => socket.addEventListener("open", resolve));
await send("Runtime.enable");

console.log("before:", await evaluate(`document.body.innerText.replace(/\\n+/g, " | ").slice(0, 160)`));

console.log(
  "type:",
  await evaluate(`(() => {
    const ta = document.querySelector('textarea');
    if (!ta) return 'no textarea found';
    const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
    setter.call(ta, ${JSON.stringify(text)});
    ta.dispatchEvent(new Event('input', { bubbles: true }));
    return 'set: ' + ta.value;
  })()`),
);

console.log(
  "send:",
  await evaluate(`(() => {
    const ta = document.querySelector('textarea');
    ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    return 'enter dispatched';
  })()`),
);

console.log("waiting for the reply…");
for (let attempt = 0; attempt < 20; attempt += 1) {
  await sleep(3000);
  const state = await evaluate(
    `(() => {
      const text = document.body.innerText.replace(/\\n+/g, " | ");
      return text.slice(0, 400);
    })()`,
  );
  console.log(`t+${(attempt + 1) * 3}s: ${state}`);
  if (/pong/i.test(state)) {
    console.log("REPLY RECEIVED");
    break;
  }
}

const shot = await send("Page.captureScreenshot", { format: "png" });
if (shot?.data) {
  const { writeFileSync } = await import("node:fs");
  writeFileSync("target/webview-send.png", Buffer.from(shot.data, "base64"));
  console.log("screenshot: target/webview-send.png");
}
socket.close();
