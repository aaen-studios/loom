import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { GENERATED_UI_LANGUAGE } from "./generatedUi";

/**
 * The class vocabulary is a contract between the prompt guide (Rust) and the
 * shadow-root stylesheet. The model can only write what the guide names, and it
 * only looks right if the sheet still defines it — so check both.
 */
const ROOT = fileURLToPath(new URL("../../", import.meta.url));
const GUIDE = readFileSync(`${ROOT}crates/loom-core/src/ui_guide.rs`, "utf8");
const SHEET = readFileSync(`${ROOT}src/generatedUi.css`, "utf8");

const CLASSES = [
  "card",
  "btn",
  "btn-ghost",
  "chip",
  "grid",
  "row",
  "stat",
  "bar",
  "callout",
  "kv",
  "muted",
  "faint",
  "mono",
  "accent",
  "scroll",
];

describe("generated UI guide", () => {
  it("documents the fence the frontend listens for", () => {
    expect(GUIDE).toContain(`\`\`\`${GENERATED_UI_LANGUAGE}`);
  });

  it("documents the markdown document fence", () => {
    expect(GUIDE).toContain("```markdown");
  });

  it("names every house class in both the guide and the sheet", () => {
    for (const name of CLASSES) {
      expect(GUIDE, `guide is missing .${name}`).toContain(`.${name}`);
      expect(SHEET, `sheet is missing .${name}`).toContain(`.${name}`);
    }
  });
});
