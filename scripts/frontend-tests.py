"""Prints the frontend test files and what they cover, plus the test runner setup.

Written as a file because inline `python -c` quoting is unreliable in this shell
and the resulting syntax errors look like failures of the code under test.
"""

import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent

print("=== frontend test files ===")
total = 0
for path in sorted((ROOT / "src").rglob("*.test.ts*")):
    text = path.read_text(encoding="utf-8", errors="replace")
    count = len(re.findall(r"\b(?:it|test)\(", text))
    total += count
    print(f"  {path.relative_to(ROOT)}: {count} tests, {len(text.splitlines())} lines")

print(f"\n  total: {total} tests")

print("\n=== does anything test the voice stores or components ===")
for path in sorted((ROOT / "src").rglob("*.test.ts*")):
    text = path.read_text(encoding="utf-8", errors="replace")
    if re.search(r"voice|microphone|dictation|speak", text, re.I):
        print(f"  {path.relative_to(ROOT)}")
        for line_no, line in enumerate(text.splitlines(), 1):
            if re.search(r"voice|microphone|dictation|speak", line, re.I):
                print(f"    {line_no}: {line.strip()[:100]}")

print("\n=== vitest config ===")
config = ROOT / "vitest.config.ts"
print(config.read_text(encoding="utf-8") if config.exists() else "missing")
