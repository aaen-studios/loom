// Exercises the chat header (tokens, search) and the hover message actions.
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
const { writeFileSync } = await import("node:fs");
const shot = async (name) => {
  const result = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(name, Buffer.from(result.data, "base64"));
  console.log(`saved ${name}`);
};

await new Promise((resolve) => socket.addEventListener("open", resolve));
await send("Runtime.enable");

console.log(
  "header:",
  await evaluate(
    `(() => {
      const header = document.querySelector('.panel .h-11');
      return header ? header.innerText.replace(/\\n+/g, " | ") : "missing";
    })()`,
  ),
);

console.log(
  "action buttons on the first message:",
  await evaluate(
    `(() => {
      const rows = [...document.querySelectorAll('.group')];
      const button = rows[0] ? rows[0].querySelector('button') : null;
      if (!button) return "none";
      return [...rows[0].querySelectorAll('button')].map((b) => b.title || b.ariaLabel).join(", ");
    })()`,
  ),
);

// Search the transcript.
await evaluate(`(() => {
  const input = document.querySelector('input[placeholder="Search chat"]');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  setter.call(input, "Nobel");
  input.dispatchEvent(new Event('input', { bubbles: true }));
  return "typed";
})()`);
await sleep(700);
console.log(
  "after searching 'Nobel':",
  await evaluate(
    `(() => {
      const text = document.body.innerText;
      const match = text.match(/(\\d+) match/);
      return JSON.stringify({ matches: match ? match[1] : null, blocks: document.querySelectorAll('.message-body').length });
    })()`,
  ),
);
await shot("target/search.png");

// Clear the search again.
await evaluate(`(() => {
  const input = document.querySelector('input[placeholder="Search chat"]');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  setter.call(input, "");
  input.dispatchEvent(new Event('input', { bubbles: true }));
  return "cleared";
})()`);
await sleep(500);

// Reveal thinking on the assistant reply (it is collapsed by default).
console.log(
  "thinking panel before:",
  await evaluate(`document.body.innerText.includes("Hide thinking") ? "open" : "collapsed or absent"`),
);
await evaluate(`(() => {
  const rows = [...document.querySelectorAll('.group')];
  for (const row of rows) {
    const button = [...row.querySelectorAll('button')].find((b) => (b.title || "").startsWith("Show thinking"));
    if (button) { button.click(); return "clicked"; }
  }
  return "no hidden thinking on screen";
})()`);
await sleep(600);
console.log(
  "thinking panel after:",
  await evaluate(`document.body.innerText.includes("Hide thinking") ? "open" : "still collapsed"`),
);
socket.close();
