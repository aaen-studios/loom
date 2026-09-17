// Guards against store selectors that build a fresh array/object literal.
//
// Zustand hands selectors to useSyncExternalStore, which decides whether state
// changed by comparing snapshots with Object.is. A selector that returns a new
// `[]` or `{}` on every call therefore reports a change on every single read:
// React re-renders, the render reads again, and the component spins until React
// throws "Maximum update depth exceeded".
//
// The fix is always the same: hoist the fallback to a module-level constant, or
// select a primitive. Both `?? NO_TODOS` and `todos.length > 0` are fine.
//
// Only selectors passed to a hook are inspected. Hooks whose argument is not a
// selector (useEffect, useMemo, ...) are skipped, so imperative code inside them
// does not trip the check. Add `audit-ok` in the call to silence one on purpose.
//
// Usage:
//   node scripts/audit-selectors.mjs            # scan src/
//   node scripts/audit-selectors.mjs --self-test
import { readFileSync, readdirSync } from "node:fs";

const SKIP_DIRS = new Set(["node_modules", "dist", "build", ".git", "target", "coverage"]);

// Hooks that take something other than a store selector as their first argument.
const NOT_A_SELECTOR = new Set([
  "useCallback",
  "useDeferredValue",
  "useEffect",
  "useId",
  "useImperativeHandle",
  "useLayoutEffect",
  "useMemo",
  "useReducer",
  "useRef",
  "useState",
  "useSyncExternalStore",
  "useTransition",
]);

const HOOK_CALL = /\b(use[A-Z]\w*)\s*\(/g;

// An empty fallback literal: `?? []`, `: []`, `?? {}`, `: {}`.
const FRESH_LITERAL = /\?\?\s*(?:\[\]|\{\})|:\s*(?:\[\]|\{\})\s*(?:[),;]|$)/;

function walk(dir) {
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (!SKIP_DIRS.has(entry.name)) out.push(...walk(`${dir}/${entry.name}`));
    } else if (/\.(ts|tsx)$/.test(entry.name)) {
      out.push(`${dir}/${entry.name}`);
    }
  }
  return out;
}

/** The balanced parenthesis group that starts at `openIdx`, and where it ends. */
function sliceCall(src, openIdx) {
  let depth = 0;
  for (let i = openIdx; i < src.length; i += 1) {
    const ch = src[i];
    if (ch === "(") depth += 1;
    else if (ch === ")") {
      depth -= 1;
      if (depth === 0) return { text: src.slice(openIdx, i + 1), end: i + 1 };
    }
  }
  return { text: src.slice(openIdx), end: src.length };
}

/** Selectors in `src` that return a fresh empty literal. */
export function scanSource(src, file = "<source>") {
  const findings = [];
  HOOK_CALL.lastIndex = 0;
  let match;
  while ((match = HOOK_CALL.exec(src)) !== null) {
    const hook = match[1];
    if (NOT_A_SELECTOR.has(hook)) continue;
    const { text, end } = sliceCall(src, match.index + match[0].length - 1);
    // The marker may sit on the opening line or on the closing one, so widen the
    // window to the end of whichever line the call finishes on.
    const lineEnd = src.indexOf("\n", end);
    const context = src.slice(match.index, lineEnd === -1 ? src.length : lineEnd);
    if (context.includes("audit-ok")) continue;
    const literal = text.match(FRESH_LITERAL);
    if (!literal) continue;
    findings.push({
      file,
      line: src.slice(0, match.index).split("\n").length,
      hook,
      literal: literal[0].replace(/\s+/g, " ").trim(),
    });
  }
  return findings;
}

const SELF_TEST_CASES = [
  {
    name: "fresh array fallback is flagged",
    expect: 1,
    src: `const todos = useChat((state) =>
  state.activeId ? (state.todos[state.activeId] ?? []) : [],
);`,
  },
  {
    name: "hoisted constant is allowed",
    expect: 0,
    src: `const NO_TODOS: Todo[] = [];
const todos = useChat((state) =>
  state.activeId ? state.todos[state.activeId] ?? NO_TODOS : NO_TODOS,
);`,
  },
  {
    name: "selecting a primitive is allowed",
    expect: 0,
    src: `const hasTodos = useChat((state) =>
  state.activeId ? (state.todos[state.activeId]?.length ?? 0) > 0 : false,
);`,
  },
  {
    name: "useEffect body is not a selector",
    expect: 0,
    src: `useEffect(() => {
  setItems([]);
}, []);`,
  },
  {
    name: "useMemo is not a store selector",
    expect: 0,
    src: `const list = useMemo(() => map.get(id) ?? [], [map, id]);`,
  },
  {
    name: "audit-ok silences one on purpose",
    expect: 0,
    src: `const rows = useThing((state) => state.rows ?? []); // audit-ok`,
  },
];

function selfTest() {
  let failed = 0;
  for (const testCase of SELF_TEST_CASES) {
    const found = scanSource(testCase.src).length;
    const ok = found === testCase.expect;
    if (!ok) failed += 1;
    console.log(`${ok ? "ok  " : "FAIL"} ${testCase.name} (expected ${testCase.expect}, got ${found})`);
  }
  if (failed > 0) {
    console.log(`\n${failed} self-test(s) failed.`);
    process.exitCode = 1;
  } else {
    console.log(`\nAll ${SELF_TEST_CASES.length} self-tests passed.`);
  }
}

const target = process.argv[2];
if (target === "--self-test") {
  selfTest();
} else {
  const root = target ?? "src";
  const findings = walk(root).flatMap((file) => scanSource(readFileSync(file, "utf8"), file));
  for (const f of findings) {
    console.log(`${f.file}:${f.line}: ${f.hook}(...) returns a fresh literal \`${f.literal}\``);
  }
  if (findings.length === 0) {
    console.log("Selectors clean: no fresh array/object literals in store selectors.");
  } else {
    console.log(`\n${findings.length} selector(s) returning a fresh literal.`);
    console.log("Hoist the fallback to a module constant, or select a primitive.");
    process.exitCode = 1;
  }
}
