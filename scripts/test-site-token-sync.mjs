// TEMPORARY: proves the token sync actually fails on each way it can break.
//
// The sync script claims to fail loudly. That claim is only worth anything if
// it is tested, because the failure it guards against — a region silently
// becoming empty — produces a site that builds fine and renders unstyled.
//
// Each case copies src/styles.css, breaks exactly one thing, runs the script,
// and restores the original. The file's bytes are compared at the end, so a
// run cannot leave the source modified.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const SOURCE = join(root, "src", "styles.css");
const SCRIPT = join(root, "scripts", "sync-site-tokens.mjs");
const original = readFileSync(SOURCE, "utf8");
const originalBytes = Buffer.from(original);

/**
 * Runs the sync and returns its exit code plus the combined output.
 *
 * Note the plain `stdio: "pipe"`: passing an explicit three-element pipe array
 * stops execFileSync from populating `error.stdout`/`error.stderr`, which is
 * how the first version of this harness silently compared against an empty
 * string and reported six bogus failures.
 */
function runSync(args = []) {
  try {
    const stdout = execFileSync("node", [SCRIPT, ...args], {
      encoding: "utf8",
      stdio: "pipe",
    });
    return { code: 0, output: stdout };
  } catch (error) {
    return {
      code: typeof error.status === "number" ? error.status : 1,
      output: `${error.stdout ?? ""}${error.stderr ?? ""}`,
    };
  }
}

/**
 * Cases are matched on a distinctive *substring*, not a regex with
 * alternation: `/a.*b|c.*d/s` applies the `s` flag only to the second
 * alternative, so `.*` quietly refuses to cross a newline in the first.
 * Substrings cannot have that bug.
 */
const cases = [
  {
    label: "a start marker is deleted",
    break: (text) => text.replace("/* loom-site:tokens:start */\n", ""),
    expect: 'region "tokens"',
    alsoExpect: "0 start marker",
  },
  {
    label: "an end marker is deleted",
    break: (text) => text.replace("/* loom-site:controls:end */\n", ""),
    expect: 'region "controls"',
    alsoExpect: "0 end marker",
  },
  {
    label: "a region is emptied between its markers",
    break: (text) => {
      const start = text.indexOf("/* loom-site:utilities:start */");
      const end = text.indexOf("/* loom-site:utilities:end */");
      const afterStart = start + "/* loom-site:utilities:start */".length;
      return text.slice(0, afterStart) + "\n" + text.slice(end);
    },
    expect: 'region "utilities"',
    alsoExpect: "is empty",
  },
  {
    label: "markers are inverted (end before start)",
    break: (text) => {
      // A placeholder is required: renaming start->end and then end->start
      // would rewrite both, since the second pass matches what the first wrote.
      const SENTINEL = "/* loom-site:motion:INVERTED */";
      return text
        .replace("/* loom-site:motion:start */", SENTINEL)
        .replace("/* loom-site:motion:end */", "/* loom-site:motion:start */")
        .replace(SENTINEL, "/* loom-site:motion:end */");
    },
    expect: 'region "motion"',
    alsoExpect: "comes at or after",
  },
  {
    label: "a region name is misspelled",
    break: (text) =>
      text.replace("/* loom-site:surfaces:start */", "/* loom-site:surface:start */"),
    expect: "surface",
    alsoExpect: "0 start marker",
  },
  {
    label: "an unknown extra region is added",
    break: (text) =>
      text.replace(
        "/* loom-site:tokens:end */",
        "/* loom-site:colours:start */\n/* loom-site:colours:end */\n/* loom-site:tokens:end */",
      ),
    // Adding the markers *inside* another region trips the nesting check first,
    // which is the correct rejection: either error means the file is not the
    // shape the script expects.
    expect: 'contains a marker for "colours"',
    alsoExpect: undefined,
  },
  {
    label: "all markers are removed",
    break: (text) => text.replace(/\/\* loom-site:[a-z]+:(start|end) \*\/\n/g, ""),
    expect: "no loom-site:*:start markers",
    alsoExpect: undefined,
  },
];

let passed = 0;
let failed = 0;

for (const testCase of cases) {
  const broken = testCase.break(original);
  if (broken === original) {
    console.log(`FAIL  ${testCase.label}\n        the harness edit was a no-op`);
    failed += 1;
    continue;
  }

  writeFileSync(SOURCE, broken);
  const result = runSync();
  // Restore first, then verify: the sync script never writes to the source, so
  // comparing before the restore would always report "not restored" and mask
  // the actual result of the case.
  writeFileSync(SOURCE, original);
  const restored = readFileSync(SOURCE, "utf8") === original;

  const failedClosed = result.code !== 0;
  const sawMessage = result.output.includes(testCase.expect);
  const sawDetail =
    testCase.alsoExpect === undefined || result.output.includes(testCase.alsoExpect);
  const ok = failedClosed && sawMessage && sawDetail && restored;

  // The first meaningful line, so the failure is readable in the log.
  const headline =
    result.output
      .split("\n")
      .map((line) => line.trim())
      .find((line) => line.startsWith("sync-site-tokens:")) ?? "(no message)";

  console.log(
    `${ok ? "PASS" : "FAIL"}  ${testCase.label}\n` +
      `        exit ${result.code}${failedClosed ? "" : "  << DID NOT FAIL"}` +
      `${restored ? "" : "  << SOURCE NOT RESTORED"}\n` +
      `        ${headline.slice(0, 118)}`,
  );
  ok ? (passed += 1) : (failed += 1);
}

// The file must be byte-identical afterwards, or running this would be a leak.
const untouched = readFileSync(SOURCE).equals(originalBytes);

console.log("");
console.log(`source restored byte-for-byte: ${untouched ? "yes" : "NO — CHECK src/styles.css"}`);
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0 || !untouched) process.exit(1);

// And the unbroken source must still pass cleanly, so the harness cannot be
// "passing" because the script fails on everything.
const clean = runSync(["--check"]);
console.log(`\nunbroken source, --check: exit ${clean.code}`);
console.log(clean.output.trim());
if (clean.code !== 0) process.exit(1);

const write = runSync([]);
console.log(`unbroken source, write: exit ${write.code}`);
console.log(write.output.trim().split("\n")[0]);
