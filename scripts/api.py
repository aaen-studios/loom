"""Prints the exact public API of one file, so a call site matches it.

Written as a file because inline `python -c` with nested quotes arrives mangled
in this shell: the syntax error then says nothing about the command that caused
it, and the retries look like flaky tools rather than a broken invocation.
"""

import re
import sys
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent

targets = sys.argv[1:] or [
    "crates/loom-core/src/voice/audio.rs",
    "crates/loom-core/src/voice/config.rs",
]

for name in targets:
    path = ROOT / name
    if not path.exists():
        print(f"MISSING: {name}")
        continue

    text = path.read_text(encoding="utf-8", errors="replace")
    print(f"\n=== {name} ({len(text.splitlines())} lines) ===")

    print("  functions:")
    for match in re.finditer(r"pub fn ([A-Za-z0-9_]+)", text):
        line_no = text[: match.start()].count("\n") + 1
        print(f"    {match.group(1)}  (line {line_no})")

    consts = re.findall(r"pub const ([A-Z0-9_]+)", text)
    if consts:
        print(f"  consts: {consts}")

    # The signature, one line, for each function above.
    print("  signatures:")
    for match in re.finditer(r"^\s*pub fn [A-Za-z0-9_]+", text, re.M):
        chunk = text[match.start() : match.start() + 400]
        depth = 0
        end = len(chunk)
        for index, char in enumerate(chunk):
            if char in "([":
                depth += 1
            elif char in ")]":
                depth -= 1
            elif char in "{;" and depth == 0 and index > 0:
                end = index
                break
        print(f"    {' '.join(chunk[:end].split())[:120]}")

    # Every struct, with its fields, so a field name is never guessed.
    for match in re.finditer(r"pub struct ([A-Za-z0-9_]+)", text):
        name_match = match.group(1)
        brace = text.find("{", match.end())
        if brace == -1:
            continue
        depth = 0
        close = brace
        for index in range(brace, len(text)):
            if text[index] == "{":
                depth += 1
            elif text[index] == "}":
                depth -= 1
                if depth == 0:
                    close = index
                    break
        print(f"  struct {name_match}:")
        for line in text[brace + 1 : close].splitlines():
            stripped = line.strip()
            if stripped.startswith("pub "):
                print(f"    {stripped[:110]}")
