"""Reads the `ort` crate's source to find the right API for session outlets.

Written because `input.input_type()` does not exist on `&Outlet` and guessing
again would waste another compile. The crate source is in the cargo registry
either way, so the answer is a text search away.

Run: python scripts/probe-ort-api.py
"""

import os
import pathlib
import re
import sys

REGISTRY = pathlib.Path(os.environ["USERPROFILE"]) / ".cargo" / "registry" / "src"


def find_ort():
    if not REGISTRY.is_dir():
        return None
    for entry in REGISTRY.glob("*/ort-2.0.0-rc.13"):
        return entry
    return None


def show(root, label, pattern, limit=3, window=1500):
    print(f"=== {label} ===")
    hits = 0
    for path in sorted(root.glob("**/*.rs")):
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for match in re.finditer(pattern, text):
            line = text[: match.start()].count("\n") + 1
            print(f"  {path.relative_to(root)}:{line}")
            snippet = text[match.start() : match.start() + window]
            for chunk in snippet.splitlines()[:40]:
                print(f"    {chunk}")
            print()
            hits += 1
            if hits >= limit:
                break
        if hits >= limit:
            break
    if not hits:
        print("  (not found)")
    print()


def main():
    root = find_ort()
    if root is None:
        print("ort source not found in the cargo registry")
        return 1

    print(f"ort source: {root}")
    print()

    show(root, "struct Outlet", r"pub struct Outlet\b")
    show(root, "impl Outlet", r"impl(?:<[^>]*>)?\s+Outlet")
    show(root, "Outlet accessors", r"pub fn (name|dtype|value_type|input_type|output_type)\b", limit=8, window=300)

    print("=== every public method taking &self on Outlet ===")
    for path in sorted(root.glob("**/*.rs")):
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for block in re.finditer(r"impl(?:<[^>]*>)?\s+Outlet[^\{]*\{", text):
            start = block.end()
            # Walk to the matching close brace, crudely.
            depth = 1
            index = start
            while index < len(text) and depth > 0:
                if text[index] == "{":
                    depth += 1
                elif text[index] == "}":
                    depth -= 1
                index += 1
            body = text[start:index]
            for method in re.finditer(r"pub fn (\w+)\(([^)]*)\)", body):
                print(f"  pub fn {method.group(1)}({method.group(2).strip()})")
    print()

    # The value type, which is what carries the shape.
    show(root, "ValueType", r"pub enum ValueType\b", limit=1, window=1200)
    show(root, "ValueType tensor helper", r"pub fn tensor_type\b", limit=2, window=400)

    return 0


if __name__ == "__main__":
    sys.exit(main())
