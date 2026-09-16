"""Runs the project's typecheck and prints the diagnostics, one per line.

Inline `python -c` with multi-line source keeps getting mangled by the shell's
quoting, so this lives in a file where the arguments are just strings.

Run: python scripts/typecheck.py
"""

import subprocess
import sys

result = subprocess.run(
    ["bun", "run", "typecheck"],
    capture_output=True,
    text=True,
    shell=True,
)

output = (result.stdout or "") + (result.stderr or "")
lines = [line.rstrip() for line in output.splitlines() if line.strip()]

if not lines:
    print("CLEAN")
    sys.exit(0)

# tsc reports "file(line,col): error TSxxxx: message"; group by file so a long
# list is readable.
errors = [line for line in lines if "error TS" in line]
other = [line for line in lines if "error TS" not in line]

print(f"{len(errors)} error(s)\n")
for line in errors:
    print(f"  {line}")

if other:
    print("\nother output:")
    for line in other[:20]:
        print(f"  {line}")

sys.exit(1 if errors else 0)
