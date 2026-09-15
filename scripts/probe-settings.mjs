// Opens the settings drawer and screenshots each category, so the new layout
// can be inspected rather than assumed.
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
await send("Page.enable");

// Open settings and pull the text of the category rail so we know what rendered.
await evaluate(`window.__loom.ui.getState().setSettingsOpen(true)`);
await sleep(1200);
console.log(
  "categories:",
  await evaluate(
    `[...document.querySelectorAll('nav button')].map((b) => b.textContent).join(", ")`,
  ),
);
await shot("target/settings-general.png");

// Visit a few categories by clicking the nav buttons.
for (const label of ["Appearance", "Chat", "Tools", "Providers"]) {
  await evaluate(
    `(() => {
      const button = [...document.querySelectorAll('nav button')].find((b) => b.textContent.trim() === ${JSON.stringify(label)});
      if (button) button.click();
      return button ? "clicked" : "missing";
    })()`,
  );
  await sleep(700);
  await shot(`target/settings-${label.toLowerCase()}.png`);
}

// Search behaviour.
await evaluate(`(() => {
  const input = document.querySelector('input[placeholder="Search settings…"]');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  setter.call(input, "hotkey");
  input.dispatchEvent(new Event('input', { bubbles: true }));
  return "searched";
})()`);
await sleep(800);
console.log("search result region:", await evaluate(`document.querySelector('nav').parentElement.innerText.replace(/\\n+/g, ' | ').slice(0, 200)`));
await shot("target/settings-search.png");
socket.close();
