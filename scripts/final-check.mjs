// The final verification, in one place.
//
// Three checks and a cleanup, because running them as separate shell commands has
// been unreliable all session and I want one unambiguous result.
import { execSync } from "node:child_process";
import { unlinkSync, existsSync } from "node:fs";

function run(label, command) {
  console.log(`\n########## ${label}`);
  try {
    const out = execSync(command, {
      stdio: ["ignore", "pipe", "pipe"],
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
    });
    console.log(out.trim() || "(no output)");
    console.log(`   -> exit 0`);
    return true;
  } catch (error) {
    const out = ((error.stdout ?? "") + (error.stderr ?? "")).trim();
    console.log(out.slice(-3000) || "(no output)");
    console.log(`   -> exit ${error.status}`);
    return false;
  }
}

const results = {
  workspace: run("cargo check --workspace", "cargo check --workspace"),
  frontend: run("bun run check", "bun run check"),
};

/* The scratch scripts. Every one was a one-off; none belongs in the repo. */
const scratch = [
  "scripts/plumb-documents.mjs",
  "scripts/finish-wire.mjs",
  "scripts/fix-wire-calls.mjs",
  "scripts/add-wire-tests.mjs",
  "scripts/fix-last-tests.mjs",
  "scripts/fix-three-tests.mjs",
  "scripts/finish-browser.mjs",
  "scripts/check-tests.mjs",
  "scripts/show-errors.mjs",
  "scripts/show-helpers.mjs",
  "scripts/show-region.mjs",
  "scripts/find-wire.mjs",
  "scripts/verify.mjs",
];
let removed = 0;
for (const path of scratch) {
  if (existsSync(path)) {
    unlinkSync(path);
    removed++;
  }
}
console.log(`\n########## cleanup`);
console.log(`   removed ${removed} scratch script(s)`);

console.log("\n########## summary");
for (const [name, ok] of Object.entries(results)) {
  console.log(`   ${ok ? "PASS" : "FAIL"}  ${name}`);
}
