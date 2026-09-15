// Inspects the running Loom webview over the Chrome DevTools Protocol:
// prints console output and uncaught exceptions, then saves a screenshot.
//
// Requires the app to be running with
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
import { writeFileSync } from "node:fs";

const args = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const flags = process.argv.slice(2).filter((a) => a.startsWith("--"));
const port = args[0] ?? "9222";
const shot = args[1] ?? "target/webview.png";
const expectedTitle = args[2] ?? "Loom";
const reload = flags.includes("--reload");

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const pages = targets.filter((t) => t.type === "page" && !t.url.startsWith("devtools"));
const page = pages.find((t) => (t.title ?? "").includes(expectedTitle));
if (!page) {
  console.error(
    `no page titled "${expectedTitle}" on port ${port}; found:`,
    pages.map((t) => `${t.title} | ${t.url}`),
  );
  process.exit(1);
}
console.log(`attached: ${page.url}`);

const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
const events = [];

socket.addEventListener("message", (message) => {
  const payload = JSON.parse(message.data);
  if (payload.id && pending.has(payload.id)) {
    pending.get(payload.id)(payload.result ?? payload.error);
    pending.delete(payload.id);
    return;
  }
  if (payload.method === "Runtime.consoleAPICalled") {
    events.push(
      `console.${payload.params.type}: ` +
        payload.params.args.map((arg) => arg.value ?? arg.description ?? arg.type).join(" "),
    );
  }
  if (payload.method === "Runtime.exceptionThrown") {
    const details = payload.params.exceptionDetails;
    events.push(
      `EXCEPTION: ${details.text} ${details.exception?.description ?? ""} @ ${details.url ?? ""}:${details.lineNumber ?? "?"}`,
    );
  }
  if (payload.method === "Log.entryAdded") {
    const entry = payload.params.entry;
    events.push(`log[${entry.level}] ${entry.text} ${entry.url ?? ""}`);
  }
});

const send = (method, params = {}) =>
  new Promise((resolve) => {
    const id = nextId++;
    pending.set(id, resolve);
    socket.send(JSON.stringify({ id, method, params }));
  });

await new Promise((resolve) => socket.addEventListener("open", resolve));
await send("Runtime.enable");
await send("Log.enable");
await send("Page.enable");

if (reload) {
  await send("Page.reload", { ignoreCache: true });
  await sleep(5000);
} else {
  await sleep(500);
}

// What does the DOM actually contain?
const dom = await send("Runtime.evaluate", {
  expression: `JSON.stringify({
    url: location.href,
    title: document.title,
    bodyLength: document.body ? document.body.innerHTML.length : -1,
    rootChildren: document.getElementById("root") ? document.getElementById("root").childElementCount : -1,
    bodyText: document.body ? document.body.innerText.slice(0, 200) : "",
    styleSheets: document.styleSheets.length,
    bg: getComputedStyle(document.documentElement).backgroundColor,
    color: getComputedStyle(document.body).color
  })`,
  returnByValue: true,
});
console.log("DOM:", dom?.result?.value ?? dom);

const screenshot = await send("Page.captureScreenshot", { format: "png" });
if (screenshot?.data) {
  writeFileSync(shot, Buffer.from(screenshot.data, "base64"));
  console.log(`screenshot: ${shot}`);
} else {
  console.log("screenshot failed:", screenshot);
}

console.log(`--- events (${events.length}) ---`);
for (const event of events.slice(0, 60)) console.log(event);
socket.close();
