"""Prints the espeak-ng constants and signatures the FFI depends on.

`crates/loom-core/src/voice/espeak.rs` declares four C functions and five
constants by hand, because `espeak-rs-sys` cannot build without a full C
toolchain. Hand-written declarations are exactly the sort of thing that is
silently wrong — an incorrect `espeakINITIALIZE_PHONEME_IPA` produces audio-like
noise rather than an error — so they are read from the vendored header and
checked rather than remembered.

The header is shipped inside the `espeak-rs-sys` crate source, which is already
in the cargo registry cache whether or not the crate builds.

Run: python scripts/check-espeak-constants.py
"""

import pathlib
import os
import re
import sys

REGISTRY = pathlib.Path(os.environ["USERPROFILE"]) / ".cargo" / "registry" / "src"

WANTED_CONSTANTS = [
    "espeak_AUDIO_OUTPUT_RETRIEVAL",
    "espeakCHARS_UTF8",
    "espeakINITIALIZE_DONT_EXIT",
    "espeakINITIALIZE_PHONEME_IPA",
    "espeak_ERROR_EE_OK",
    "espeakPHONEMES_IPA",
]

WANTED_FUNCTIONS = [
    "espeak_Initialize",
    "espeak_SetVoiceByName",
    "espeak_TextToPhonemes",
]


def find_header():
    """The vendored `speak_lib.h` inside the espeak-rs-sys crate."""
    if not REGISTRY.is_dir():
        return None
    for candidate in REGISTRY.glob("*/espeak-rs-sys-*/espeak-ng/src/include/espeak-ng/speak_lib.h"):
        return candidate
    return None


def find_constants(text):
    """Every `#define NAME` line for a name we care about, with numeric value."""
    found = {}
    for name in WANTED_CONSTANTS:
        # Constants are sometimes defined in terms of others, so keep the raw
        # expression as well as any literal value.
        pattern = re.compile(
            r"^\s*#define\s+" + re.escape(name) + r"\s+([^\r\n/]+)", re.MULTILINE
        )
        match = pattern.search(text)
        if match:
            found[name] = match.group(1).strip()
    return found


def find_enums(text):
    """Enum members, which espeak-ng uses for the character and error codes."""
    found = {}
    # enum { ... } blocks, one member per line.
    for block in re.findall(r"enum\s*(?:\w+\s*)?\{(.*?)\}", text, re.DOTALL):
        for line in block.splitlines():
            line = line.strip().rstrip(",")
            if not line or line.startswith("//") or line.startswith("/*"):
                continue
            if "=" in line:
                key, value = line.split("=", 1)
                found[key.strip()] = value.strip()
            else:
                found[line] = "(implicit)"
    return found


def find_functions(text):
    """Prototypes for the functions we call."""
    found = {}
    for name in WANTED_FUNCTIONS:
        pattern = re.compile(
            r"^[^\r\n{}]*\b" + re.escape(name) + r"\s*\([^;]*\);", re.MULTILINE
        )
        match = pattern.search(text)
        if match:
            found[name] = re.sub(r"\s+", " ", match.group(0)).strip()
    return found


def main():
    header = find_header()
    if header is None:
        print("speak_lib.h not found in the cargo registry.")
        print("Run `cargo build -p loom-core --features voice-espeak` once to")
        print("vendor espeak-rs-sys, or point REGISTRY at your cargo cache.")
        return 1

    print(f"header: {header}")
    size = header.stat().st_size
    print(f"size  : {size:,} bytes")
    print()

    text = header.read_text(encoding="utf-8", errors="replace")

    print("--- #define constants ---")
    defines = find_constants(text)
    for name in WANTED_CONSTANTS:
        value = defines.get(name)
        print(f"  {name:<38} {value if value else '(not found)'}")
    print()

    print("--- enum members ---")
    enums = find_enums(text)
    for name in WANTED_CONSTANTS:
        if name in defines:
            continue
        value = enums.get(name)
        print(f"  {name:<38} {value if value else '(not found)'}")
    # The ones that matter but are not in our list.
    for probe in ("AUDIO_OUTPUT_RETRIEVAL", "espeakCHARS_UTF8", "EE_OK", "espeakINITIALIZE_DONT_EXIT"):
        value = enums.get(probe)
        if value:
            print(f"  {probe:<38} {value}  (enum)")
    print()

    print("--- function prototypes ---")
    functions = find_functions(text)
    for name in WANTED_FUNCTIONS:
        prototype = functions.get(name)
        print(f"  {name}:")
        print(f"    {prototype if prototype else '(not found)'}")
    print()

    # A literal value for PHONEME_IPA is what the IPA mode depends on; if it is
    # expressed in terms of another constant, say so rather than guessing.
    ipa = defines.get("espeakINITIALIZE_PHONEME_IPA")
    print("VERDICT")
    if ipa:
        print(f"  espeakINITIALIZE_PHONEME_IPA = {ipa}")
    else:
        print("  espeakINITIALIZE_PHONEME_IPA is not a plain #define —")
        print("  check the enum block above before trusting the value in espeak.rs.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
