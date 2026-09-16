// TEMPORARY: repairs the release workflow. Delete immediately after use.
//
// The workflow is rejected by GitHub with "Unrecognized named-value: 'secrets'"
// because `secrets` is not one of the contexts available in a step-level `if:`.
// An invalid file means GitHub cannot even read its `name:`, so it is listed
// under its path and every evaluation records a zero-job failure.
//
// The fix is to pass the values through `env` (which `secrets` *is* allowed in)
// and let the script decide whether there is anything to sign. That also
// removes the interpolation of secrets directly into a PowerShell script, where
// an unexpected quote could break it.
//
// Line endings are preserved; the file is checked byte-wise afterwards.
import { readFileSync, writeFileSync } from "node:fs";

const PATH = ".github/workflows/release.yml";
const original = readFileSync(PATH, "utf8");
const crlf = original.includes("\r\n");
let text = original.replace(/\r\n/g, "\n");

let failures = 0;

/** Replace `pattern` with `replacement`, asserting it matched exactly `expected` times. */
function swap(label, pattern, replacement, expected) {
  const found = text.match(pattern);
  const count = found ? found.length : 0;
  if (count !== expected) {
    console.error(`FAIL  ${label}: expected ${expected} match(es), found ${count}`);
    failures += 1;
    return;
  }
  text = text.replace(pattern, replacement);
  console.log(`ok    ${label}: ${count} replacement(s)`);
}

// 1. Both certificate steps: drop the illegal `if:` and pass the values via env.
swap(
  "cert steps: if: -> env: + in-script guard",
  new RegExp(
    "^        if: \\$\\{\\{ secrets\\.WINDOWS_CERT_BASE64 != '' \\}\\}\n" +
      "        shell: pwsh\n" +
      "        run: \\|\n",
    "gm",
  ),
  () =>
    "        # `secrets` is NOT available in a step-level `if:`: GitHub rejects the\n" +
    "        # whole workflow with \"Unrecognized named-value: 'secrets'\". Pass the\n" +
    "        # values through `env` instead and let the script decide, which also\n" +
    "        # keeps a stray quote in a secret from breaking the script.\n" +
    "        env:\n" +
    "          WINDOWS_CERT_BASE64: ${{ secrets.WINDOWS_CERT_BASE64 }}\n" +
    "          WINDOWS_CERT_PASSWORD: ${{ secrets.WINDOWS_CERT_PASSWORD }}\n" +
    "        shell: pwsh\n" +
    "        run: |\n" +
    "          if ([string]::IsNullOrWhiteSpace($env:WINDOWS_CERT_BASE64)) {\n" +
    '            Write-Host "No certificate configured - skipping signing."\n' +
    "            exit 0\n" +
    "          }\n",
  2,
);

// 2. Read the certificate from the environment rather than interpolating it.
swap(
  "cert: base64 read from env",
  /\[Convert\]::FromBase64String\("\$\{\{ secrets\.WINDOWS_CERT_BASE64 \}\}"\)/g,
  () => "[Convert]::FromBase64String($env:WINDOWS_CERT_BASE64)",
  2,
);

swap(
  "cert: password read from env",
  /\/p "\$\{\{ secrets\.WINDOWS_CERT_PASSWORD \}\}"/g,
  () => "/p $env:WINDOWS_CERT_PASSWORD",
  2,
);

// 3. Same treatment for the minisign key: it is only interpolated inside `run:`,
//    which GitHub permits, but reading it from `env` is consistent and safer.
swap(
  "minisign: collect step gains env",
  /^      - name: Collect artifacts \+ manifest\n        shell: pwsh\n/gm,
  () =>
    "      - name: Collect artifacts + manifest\n" +
    "        env:\n" +
    "          LOOM_MINISIGN_KEY: ${{ secrets.LOOM_MINISIGN_KEY }}\n" +
    "        shell: pwsh\n",
  1,
);

swap(
  "minisign: presence check reads env",
  /if \("\$\{\{ secrets\.LOOM_MINISIGN_KEY \}\}" -ne ""\) \{/g,
  () => "if (-not [string]::IsNullOrWhiteSpace($env:LOOM_MINISIGN_KEY)) {",
  1,
);

swap(
  "minisign: key written from env",
  /-Value "\$\{\{ secrets\.LOOM_MINISIGN_KEY \}\}"/g,
  () => "-Value $env:LOOM_MINISIGN_KEY",
  1,
);

if (failures > 0) {
  console.error(`\n${failures} step(s) failed - file NOT written, nothing changed.`);
  process.exit(1);
}

// The invariant that matters: no step-level `if:` may mention `secrets`.
const badIf = text
  .split("\n")
  .filter((line) => line.trim().startsWith("if:") && line.includes("secrets"));
if (badIf.length > 0) {
  console.error("\n`secrets` still appears in an if: expression:");
  for (const line of badIf) console.error(`  ${line.trim()}`);
  console.error("not writing - the workflow would still be rejected.");
  process.exit(1);
}

// `env:` must now carry all three secrets, or the guard would always skip.
for (const name of [
  "WINDOWS_CERT_BASE64",
  "WINDOWS_CERT_PASSWORD",
  "LOOM_MINISIGN_KEY",
]) {
  const declared = text.includes(`          ${name}: `);
  console.log(`${declared ? "ok   " : "FAIL "} ${name} declared in a step env:`);
  if (!declared) failures += 1;
}

if (failures > 0) process.exit(1);

writeFileSync(PATH, crlf ? text.replace(/\n/g, "\r\n") : text);
console.log(`\nwritten ${PATH} (${crlf ? "CRLF" : "LF"} preserved)`);
