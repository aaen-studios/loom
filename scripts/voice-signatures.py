"""Prints the exact signatures the microphone layer needs to call.

Everything here is read from the source rather than remembered: a wrong function
name or argument order is a compile error at best, and at worst a call that
compiles because the arguments happen to be assignable in the wrong order.
"""

import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent


def public_api(path, label):
    print(f"\n=== {label} ===")
    text = path.read_text(encoding="utf-8", errors="replace")
    print(f"({len(text.splitlines())} lines)")

    # Public data types and their fields.
    for match in re.finditer(
        r"^pub (struct|enum) (\w+)\s*\{", text, re.M
    ):
        kind, name = match.group(1), match.group(2)
        # Find the matching closing brace at column 0.
        start = text.index("{", match.end() - 1)
        depth = 0
        end = start
        for index in range(start, len(text)):
            if text[index] == "{":
                depth += 1
            elif text[index] == "}":
                depth -= 1
                if depth == 0:
                    end = index
                    break
        print(f"\n  pub {kind} {name} {{")
        for line in text[start + 1 : end].splitlines():
            stripped = line.strip()
            if stripped.startswith("pub ") or (
                kind == "enum" and re.match(r"^[A-Z]\w*\s*[{(,]?$", stripped)
            ):
                print(f"      {stripped[:100]}")
        print("  }")

    # Public functions, one line each with the whole argument list.
    print("\n  public functions:")
    for match in re.finditer(r"^\s*pub fn (\w+)", text, re.M):
        start = match.start()
        # Take up to the opening brace or semicolon.
        chunk = text[start : start + 600]
        depth = 0
        end = 0
        for index, char in enumerate(chunk):
            if char in "([":
                depth += 1
            elif char in ")]":
                depth -= 1
            elif char in "{;" and depth == 0 and index > 0:
                end = index
                break
        signature = " ".join(chunk[:end].split())
        print(f"    {signature[:150]}")


public_api(ROOT / "crates/loom-core/src/voice/audio.rs", "voice/audio.rs")
public_api(ROOT / "crates/loom-core/src/voice/whisper.rs", "voice/whisper.rs")

# The tts Paths, which say where the VAD model lives.
print("\n=== voice/tts.rs: Paths ===")
text = (ROOT / "crates/loom-core/src/voice/tts.rs").read_text(encoding="utf-8", errors="replace")
struct = re.search(r"pub struct Paths\s*\{(.*?)\n\}", text, re.S)
if struct:
    for line in struct.group(1).splitlines():
        if line.strip().startswith("pub "):
            print(f"    {line.strip()[:110]}")

# The error type, so the new module returns the right variant.
print("\n=== the Error enum ===")
lib = (ROOT / "crates/loom-core/src/lib.rs").read_text(encoding="utf-8", errors="replace")
enum = re.search(r"pub enum Error\s*\{(.*?)\n\}", lib, re.S)
if enum:
    for line in enum.group(1).splitlines():
        stripped = line.strip()
        if stripped and not stripped.startswith("//") and not stripped.startswith("#"):
            print(f"    {stripped[:100]}")

# Anything already named for capture, so a duplicate is visible.
print("\n=== existing microphone-ish names anywhere ===")
for path in sorted((ROOT / "crates").rglob("*.rs")):
    text = path.read_text(encoding="utf-8", errors="replace")
    for match in re.finditer(
        r"\b(fn|struct|enum|const)\s+(\w*(?:[Mm]ic|capture|Input|Recording)\w*)", text
    ):
        line = text[: match.start()].count("\n") + 1
        print(f"  {path.relative_to(ROOT)}:{line}: {match.group(1)} {match.group(2)}")
