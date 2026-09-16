// Temporary helper: print a line range of a file with line numbers.
import fs from "node:fs";

const [file, start, end] = process.argv.slice(2);
const lines = fs.readFileSync(file, "utf8").split("\n");
const from = Math.max(1, Number(start) || 1);
const to = Math.min(lines.length, Number(end) || lines.length);
const out = [];
for (let i = from; i <= to; i += 1) out.push(`${i}: ${lines[i - 1]}`);
console.log(`--- ${file} ${from}-${to} of ${lines.length} ---`);
console.log(out.join("\n"));
