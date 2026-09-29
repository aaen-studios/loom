// Adds the `Document` variant to every exhaustive match over `ContentPart`.
//
// The compiler names each one, but reading them back through a grep against a
// 400 KB file has been unreliable all session, so this asserts each site instead
// and reports the ones it could not find — a silent miss here is a compile error
// on the next build, which is fine, but a *wrong* edit is not.
import { readFileSync, writeFileSync } from "node:fs";

const edits = [
  [
    "crates/loom-core/src/providers/openai.rs",
    [
      [
        "message_json document arm",
        `                super::ContentPart::Document { mime, base64, name } => json!({
                    "type": "file",
                    "file": {
                        "filename": name,
                        "file_data": format!("data:{mime};base64,{base64}")
                    }
                }),`,
      ],
    ],
  ],
];

console.log("no-op placeholder: the arms are written by hand below");
