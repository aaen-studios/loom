"""Runs the test suite and prints only what matters: failures and their detail.

The full `cargo test` output is thousands of lines of passing-test names, and the
interesting part — which test failed and why — is buried in the middle. The
shell also mangles multi-line Python passed inline, so this lives in a file.

Run: python scripts/check-tests.py
     python scripts/check-tests.py --package loom
     python scripts/check-tests.py --full
"""

import argparse
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", "-p", default="loom-core")
    parser.add_argument("--full", action="store_true", help="print everything")
    parser.add_argument("--lib", action="store_true", help="library tests only")
    args = parser.parse_args()

    command = ["cargo", "test", "-p", args.package]
    if args.lib:
        command.append("--lib")

    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    output = (result.stdout or "") + (result.stderr or "")

    if args.full:
        print(output)
        return result.returncode

    lines = output.splitlines()

    summary = [line for line in lines if "test result:" in line]
    failures = [line for line in lines if line.strip().startswith("---- ")]
    panics = [line for line in lines if "panicked at" in line]
    errors = [line for line in lines if re.match(r"\s*error(\[|:)", line)]

    print("summary:")
    for line in summary:
        print(f"  {line.strip()}")
    if not summary:
        print("  (none — the build may have failed)")

    if errors:
        print()
        print(f"{len(errors)} compile error(s):")
        for line in errors[:15]:
            print(f"  {line.strip()}")

    if failures or panics:
        print()
        print(f"{len(failures)} failing test(s):")
        seen = set()
        for line in failures:
            name = line.strip().strip("-").strip()
            if name and name not in seen:
                seen.add(name)
                print(f"  {name}")

        print()
        print("detail:")
        for line in panics:
            print(f"  {line.strip()}")

    # The failure block, verbatim, which is where the actual assertion lives.
    marker = output.find("failures:")
    if marker >= 0 and (failures or panics):
        print()
        print("first failure block:")
        block = output[marker : marker + 2500]
        for line in block.splitlines()[:45]:
            print(f"  {line}")

    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
