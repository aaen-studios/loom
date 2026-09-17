// A focused check on ONE question: does the content inside each liquid surface
// fit inside it, and is it laid out in a row?
//
// The structural gate checks the *shell* geometry — root on shell, filter
// resolving — and all of that passed while the content was 72px tall inside a
// 40px stage. `LiquidSurface` had treated `contentClassName` as the whole class
// list rather than an addition, so the pills' `"gap-0.5 p-1"` replaced
// `flex h-full items-center` and the buttons stacked vertically. Every shell
// measurement was perfect; only the content box was wrong.
//
// So this asserts the thing that was wrong: content height <= stage height, and
// the content's children share a row.
const port = process.argv[2] ?? "9333";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let page = null;
for (let i = 0; i < 40 && !page; i += 1) {
  try {
    const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
    page = targets.find((t) => t.type === "page" && t.url.includes("1420")) ?? null;
  } catch {
    /* not up */
  }
  if (!page) await sleep(500);
}
if (!page) {
  console.error("no app page");
  process.exit(1);
}

const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
socket.addEventListener("message", (m) => {
  const p = JSON.parse(m.data);
  if (p.id && pending.has(p.id)) {
    pending.get(p.id)(p.result ?? p.error);
    pending.delete(p.id);
  }
});
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const id = nextId++;
    pending.set(id, resolve);
    socket.send(JSON.stringify({ id, method, params }));
  });
const evaluate = async (expression) => {
  const r = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (r?.exceptionDetails) return `EVAL ERROR: ${r.exceptionDetails.text}`;
  return r?.result?.value;
};

await new Promise((r) => socket.addEventListener("open", r));
await send("Runtime.enable");

// Fresh load, so this measures current code rather than an HMR graph that may
// have failed to transform mid-edit.
await send("Page.reload", { ignoreCache: true });
await sleep(4500);
for (let i = 0; i < 40; i += 1) {
  if ((await evaluate("document.querySelectorAll('.lg-stage').length")) > 0) break;
  await sleep(500);
}

const report = await evaluate(`(() => {
  const round = (n) => Math.round(n * 10) / 10;
  return JSON.stringify([...document.querySelectorAll(".lg-stage")].map((stage) => {
    const content = stage.querySelector(".lg-content");
    const style = getComputedStyle(content);
    const kids = [...content.children].map((c) => ({
      tag: c.tagName.toLowerCase(),
      y: round(c.getBoundingClientRect().y),
      h: round(c.getBoundingClientRect().height),
    }));
    const stageBox = stage.getBoundingClientRect();
    const contentBox = content.getBoundingClientRect();
    // Are the children on one row? Compare their vertical centres.
    const centres = kids.map((k) => k.y + k.h / 2);
    const spread = centres.length > 1 ? Math.max(...centres) - Math.min(...centres) : 0;
    return {
      stage: { w: round(stageBox.width), h: round(stageBox.height) },
      content: { w: round(contentBox.width), h: round(contentBox.height) },
      display: style.display,
      flexDirection: style.flexDirection,
      // The bug: content taller than its stage means it is overflowing.
      overflowPx: round(contentBox.height - stageBox.height),
      childCount: kids.length,
      rowSpreadPx: round(spread),
      fitsInStage: contentBox.height <= stageBox.height + 1,
      oneRow: spread <= 2 || kids.length <= 1,
    };
  }));
})()`);

let parsed = null;
try {
  parsed = JSON.parse(report);
} catch {
  console.log(report);
}

if (parsed) {
  console.log("=== content layout ===");
  for (const [index, stage] of parsed.entries()) {
    console.log(
      `  #${index} stage ${stage.stage.w}x${stage.stage.h}  content ${stage.content.w}x${stage.content.h}` +
        `  display=${stage.display} dir=${stage.flexDirection} children=${stage.childCount}` +
        `  rowSpread=${stage.rowSpreadPx}px  overflow=${stage.overflowPx}px`,
    );
  }
  console.log("\n=== verdict ===");
  const overflowing = parsed.filter((s) => !s.fitsInStage);
  const stacked = parsed.filter((s) => !s.oneRow);
  console.log(`  ${overflowing.length === 0 ? "PASS" : "FAIL"}  content fits inside every stage`);
  if (overflowing.length) {
    console.log(`        ${overflowing.length} surface(s) overflow: ${overflowing.map((s) => `${s.overflowPx}px`).join(", ")}`);
  }
  console.log(`  ${stacked.length === 0 ? "PASS" : "FAIL"}  content is laid out in a row`);
  if (stacked.length) {
    console.log(`        ${stacked.length} surface(s) stacked: spread ${stacked.map((s) => `${s.rowSpreadPx}px`).join(", ")}`);
  }
}

socket.close();
