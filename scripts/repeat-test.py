"""Runs one test repeatedly, to separate a flake from a real failure.

A test that fails once in a parallel suite run and passes in a serial one is
either genuinely timing-dependent or was racing a rebuild, in which case the
binary being run was not the code under test. Those have opposite
implications, so the distinction is worth establishing by repetition.

# Two bugs this script had, both of which produced confident nonsense

1. **It treated "0 passed, 607 filtered out" as a failure.** That means the
   filter matched nothing, so the test never ran — yet it was reported as four
   failures out of four. A tool that cannot tell "the test failed" from "the
   test did not run" is worse than no tool, because its output looks like
   evidence.

2. **It trusted a hand-typed test name.** The name came from a panic message and
   was pasted in, and the filter matched nothing every single time. So the names
   are now read from `cargo test --list` rather than typed, and a name that
   matches nothing is reported as such along with the closest real names.
"""

import re
import subprocess
import sys

# Without a name, the default is the test that has been flaking.
DEFAULT = "a_command_in_flight_times_out_without_being_killed"

# Quotes are stripped from the needle, and that is not cosmetic.
#
# This shell passes a quoted argument with its quote characters *attached*, so
# `repeat-test.py "a_command_in_flight"` arrives as the literal string
# `"a_command_in_flight"` — quotes included. That can never be a substring of a
# Rust test path, so the filter matched nothing and the script reported "no test
# matches". Three runs of a working test produced no evidence at all, and the
# output looked like a verdict rather than a broken invocation.
needle = sys.argv[1] if len(sys.argv) > 1 else DEFAULT
needle = needle.strip().strip("\"'").strip()
runs = int(sys.argv[2]) if len(sys.argv) > 2 else 5


def cargo(args: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["cargo", *args],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )


# --- find the real name, rather than trusting the one that was typed ---------

listing = cargo(["test", "-p", "loom-core", "--lib", "--", "--list"])
if listing.returncode != 0:
    print("could not list the tests:")
    print((listing.stdout + listing.stderr)[-800:])
    sys.exit(2)

# `--list` prints `path::to::test: test` one per line.
names = [
    match.group(1).strip()
    for match in re.finditer(r"^(.*): test$", listing.stdout, re.M)
]
if not names:
    print(f"the listing produced no test names ({len(listing.stdout)} chars)")
    print(listing.stdout[:400])
    sys.exit(2)

matches = [name for name in names if needle.lower() in name.lower()]
if not matches:
    print(f"no test matches {needle!r} among {len(names)} tests")
    # Show what exists, so the next attempt is informed rather than another guess.
    words = [word for word in re.split(r"[^a-z]+", needle.lower()) if len(word) > 4]
    near = [
        name
        for name in names
        if any(word in name.lower() for word in words)
    ][:10]
    if near:
        print("closest names:")
        for name in near:
            print(f"  {name}")
    sys.exit(2)

print(f"running {len(matches)} test(s) {runs} time(s):")
for name in matches:
    print(f"  {name}")
print()

passed = 0
failed = 0
never_ran = 0

for run in range(1, runs + 1):
    # `--exact` with a name taken from the listing cannot match nothing.
    result = cargo(
        ["test", "-p", "loom-core", "--lib"]
        + matches
        + ["--", "--exact"]
    )
    combined = result.stdout + result.stderr

    if "error[" in combined or re.search(r"^error: could not compile", combined, re.M):
        print(f"  run {run}: DID NOT COMPILE")
        for line in combined.splitlines():
            if re.match(r"\s*error", line):
                print(f"      {line.strip()[:110]}")
        never_ran += 1
        continue

    summary = re.search(r"test result: (\w+)\. (\d+) passed; (\d+) failed", combined)
    if summary is None:
        print(f"  run {run}: no summary line — inconclusive")
        never_ran += 1
        continue

    ran_passed = int(summary.group(2))
    ran_failed = int(summary.group(3))

    if ran_passed + ran_failed == 0:
        print(f"  run {run}: DID NOT RUN (the filter matched no test)")
        never_ran += 1
        continue

    if ran_failed == 0:
        passed += 1
        print(f"  run {run}: passed")
    else:
        failed += 1
        print(f"  run {run}: FAILED")
        lines = combined.splitlines()
        for index, line in enumerate(lines):
            if "panicked at" in line:
                print(f"      {line.strip()[:110]}")
                # The assertion text is the useful part, on the next line.
                for extra in lines[index + 1 : index + 3]:
                    if extra.strip() and not extra.startswith("note:"):
                        print(f"      {extra.strip()[:110]}")
                break

print()
if never_ran and not passed and not failed:
    print("the tests never ran, so this says nothing about the code")
    sys.exit(2)

print(f"{passed} passed, {failed} failed, {never_ran} did not run, of {runs}")
if passed == runs and failed == 0:
    print("consistent across every run, so an earlier single failure was not reproducible")
elif passed and failed:
    print("IT IS FLAKY: it both passes and fails, so the failure is real and intermittent")
elif failed:
    print("IT IS BROKEN: it fails every time")
