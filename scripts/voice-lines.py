"""Counts the lines in voice mode's modules, for the report.

The shell keeps mangling inline multi-line Python, so this lives in a file.

Run: python scripts/voice-lines.py
"""

import pathlib

ROOT = pathlib.Path(__file__).parent.parent

RUST = ROOT / "crates/loom-core/src/voice"
EXAMPLES = ROOT / "crates/loom-core/examples"
FRONTEND = [
    ROOT / "src/lib/voice.ts",
    ROOT / "src/stores/voice.ts",
    ROOT / "src/components/SpeakButton.tsx",
    ROOT / "src/components/VoiceSettings.tsx",
    ROOT / "src-tauri/src/voice.rs",
]
SCRIPTS = sorted((ROOT / "scripts").glob("*.py"))


def count(path):
    try:
        return len(path.read_text(encoding="utf-8").splitlines())
    except OSError:
        return 0


def section(title, paths, keys=None):
    print(f"=== {title} ===")
    total = 0
    for path in sorted(paths):
        if keys and not any(k in path.name for k in keys):
            continue
        n = count(path)
        total += n
        print(f"  {path.name:22} {n:5}")
    print(f"  {'total':22} {total:5}")
    print()
    return total


def main():
    rust = section("voice modules (Rust)", RUST.glob("*.rs"))
    examples = section("examples", EXAMPLES.glob("*.rs"))
    frontend = section("frontend", FRONTEND)
    scripts = section("scripts", SCRIPTS)

    print(f"grand total: {rust + examples + frontend + scripts:,} lines")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
