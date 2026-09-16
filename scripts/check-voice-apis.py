"""Verifies every external API constant and signature the voice modules use.

Two things are checked, both of which are silently wrong if guessed:

1. **espeak-ng's C API**, read from the header vendored inside `espeak-rs-sys`.
   An incorrect `phoneme_mode` does not fail — it produces audio-like noise.
   `espeakINITIALIZE_PHONEME_IPA` was already caught this way: it is 0x0002, not
   the 0x0001 that seemed obvious.

2. **`ort`'s Rust API**, read from the crate source in the cargo registry.
   `Tensor::from_array`, `Session::run` and `try_extract_tensor` all changed
   between release candidates, and a wrong guess is a compile error at best.

Run: python scripts/check-voice-apis.py
"""

import os
import pathlib
import re
import sys

REGISTRY = pathlib.Path(os.environ["USERPROFILE"]) / ".cargo" / "registry" / "src"


def registry_glob(pattern):
    if not REGISTRY.is_dir():
        return []
    return sorted(REGISTRY.glob(pattern))


def show_espeak():
    headers = registry_glob(
        "*/espeak-rs-sys-*/espeak-ng/src/include/espeak-ng/speak_lib.h"
    )
    if not headers:
        print("espeak-ng header: NOT FOUND (build voice-espeak once to vendor it)")
        return

    text = headers[0].read_text(encoding="utf-8", errors="replace")
    print(f"espeak-ng header: {headers[0]}")
    print()

    # The audio output enum, in full, because the members are implicit and the
    # ordinal position is the value.
    match = re.search(
        r"typedef\s+enum\s*\{(.*?)\}\s*espeak_AUDIO_OUTPUT", text, re.DOTALL
    )
    print("espeak_AUDIO_OUTPUT:")
    if match:
        ordinal = 0
        for raw in match.group(1).splitlines():
            line = raw.strip()
            if not line or line.startswith("/") or line.startswith("*"):
                continue
            line = line.rstrip(",")
            code = re.sub(r"/\*.*?\*/", "", line).strip()
            if not code:
                continue
            if "=" in code:
                name, value = code.split("=", 1)
                name, value = name.strip(), value.strip()
                try:
                    ordinal = int(value, 0)
                except ValueError:
                    pass
                print(f"  {name:<34} = {value}")
            else:
                print(f"  {name:<34} = {ordinal}")
            ordinal += 1
    else:
        print("  (enum block not found)")
    print()

    # The defines the FFI hardcodes.
    print("constants:")
    for name in (
        "espeakCHARS_UTF8",
        "espeakINITIALIZE_DONT_EXIT",
        "espeakINITIALIZE_PHONEME_IPA",
        "espeakPHONEMES_IPA",
    ):
        found = re.search(
            r"^\s*#define\s+" + re.escape(name) + r"\s+([^\r\n/]+)", text, re.MULTILINE
        )
        value = found.group(1).strip() if found else "(not found)"
        print(f"  {name:<34} {value}")

    found = re.search(
        r"typedef\s+enum\s*\{(.*?)\}\s*espeak_ERROR", text, re.DOTALL
    )
    if found:
        for raw in found.group(1).splitlines():
            line = raw.strip().rstrip(",")
            if line.startswith("EE_OK"):
                print(f"  {'espeak_ERROR_EE_OK':<34} {line.split('=')[-1].strip()}")
    print()

    print("prototypes:")
    for name in ("espeak_Initialize", "espeak_SetVoiceByName", "espeak_TextToPhonemes"):
        pattern = re.compile(
            r"^[^\r\n{}]*\b" + re.escape(name) + r"\s*\([^;]*\);", re.MULTILINE
        )
        match = pattern.search(text)
        signature = re.sub(r"\s+", " ", match.group(0)).strip() if match else "(not found)"
        # A comment block can swallow the match; note it rather than print it.
        if signature.startswith("/*") or len(signature) > 200:
            signature = "(matched a comment; see the header)"
        print(f"  {signature}")
    print()


def show_ort():
    sources = registry_glob("*/ort-2.0.0-rc.13/src")
    if not sources:
        print("ort source: NOT FOUND in the registry")
        return

    root = sources[0]
    print(f"ort source: {root}")
    print()

    def search(label, pattern, where="**/*.rs", limit=3, context=0):
        print(f"{label}:")
        hits = 0
        for path in sorted(root.glob(where)):
            try:
                text = path.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            for match in re.finditer(pattern, text, re.MULTILINE):
                line = text[: match.start()].count("\n") + 1
                snippet = match.group(0)
                if context:
                    start = max(0, match.start() - context)
                    snippet = text[start : match.end() + context]
                snippet = re.sub(r"\n\s*", " ", snippet).strip()
                print(f"  {path.relative_to(root)}:{line}")
                print(f"    {snippet[:300]}")
                hits += 1
                if hits >= limit:
                    break
            if hits >= limit:
                break
        if not hits:
            print("  (not found)")
        print()

    search(
        "Tensor::from_array",
        r"pub fn from_array[^\n]*",
    )
    search(
        "Session::run",
        r"pub fn run\b[^\n]*",
    )
    search(
        "try_extract_tensor",
        r"pub fn try_extract_tensor[^\n]*",
    )
    search(
        "init_from",
        r"pub fn init_from[^\n]*",
    )
    search(
        "commit_from_file",
        r"pub fn commit_from_file[^\n]*",
    )
    search(
        "session inputs field",
        r"pub inputs:\s*[^\n]*",
    )


def main():
    show_espeak()
    print("=" * 70)
    print()
    show_ort()
    return 0


if __name__ == "__main__":
    sys.exit(main())
