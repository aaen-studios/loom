"""Prints cargo diagnostics, one per line, with the source location.

The shell's quoting has repeatedly mangled `grep` invocations here, so this
filters in Python where the arguments are just strings.

Run: python scripts/cargo-errors.py
     python scripts/cargo-errors.py --package loom-core --tests
"""

import argparse
import re
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", "-p", default="loom-core")
    parser.add_argument("--tests", action="store_true")
    parser.add_argument("--all", action="store_true", help="the whole workspace")
    parser.add_argument("--full", action="store_true", help="raw output")
    args = parser.parse_args()

    command = ["cargo", "build" if not args.tests else "test", "--message-format=short"]
    if args.all:
        command.append("--workspace")
    else:
        command += ["-p", args.package]
    if args.tests:
        command.append("--no-run")

    result = subprocess.run(command, capture_output=True, text=True)
    output = result.stdout + result.stderr

    if args.full:
        print(output)
        return result.returncode

    errors = []
    warnings = []
    for line in output.splitlines():
        stripped = line.strip()
        if re.match(r"^.*error(\[|:)", stripped):
            errors.append(stripped)
        elif re.match(r"^.*warning:", stripped) and "generated" not in stripped:
            warnings.append(stripped)

    # Deduplicate while keeping order: cargo repeats across crates.
    def unique(items):
        seen = set()
        out = []
        for item in items:
            if item not in seen:
                seen.add(item)
                out.append(item)
        return out

    errors = unique(errors)
    warnings = unique(warnings)

    if warnings:
        print(f"--- {len(warnings)} warning(s) ---")
        for line in warnings[:20]:
            print(f"  {line}")
        print()

    if errors:
        print(f"--- {len(errors)} error(s) ---")
        for line in errors:
            print(f"  {line}")
        print()
        print("Run with --full for complete output.")
    else:
        print("no errors")

    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
