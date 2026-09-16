"""Prints `Outlet`'s public accessors from the `ort` crate source.

`Outlet` has private `name` and `dtype` fields, so the shapes and names come
from accessor methods. Which ones exist is not guessable, and getting it wrong
costs a compile cycle each time.

Run: python scripts/probe-ort-outlet.py
"""

import os
import pathlib
import re
import sys

REGISTRY = pathlib.Path(os.environ["USERPROFILE"]) / ".cargo" / "registry" / "src"


def main():
    roots = list(REGISTRY.glob("*/ort-2.0.0-rc.13")) if REGISTRY.is_dir() else []
    if not roots:
        print("ort source not found")
        return 1
    root = roots[0]

    path = root / "src" / "value" / "type.rs"
    if not path.exists():
        print(f"missing: {path}")
        return 1

    text = path.read_text(encoding="utf-8", errors="replace")
    lines = text.splitlines()

    start = next(
        (i for i, line in enumerate(lines) if line.strip() == "impl Outlet {"), None
    )
    if start is None:
        print("no impl Outlet block")
        return 1

    # Walk to the matching close brace.
    depth = 0
    end = start
    for index in range(start, len(lines)):
        depth += lines[index].count("{") - lines[index].count("}")
        if depth == 0 and index > start:
            end = index
            break

    print(f"impl Outlet: lines {start + 1}..{end + 1} of {path.relative_to(root)}")
    print()
    print("public methods:")
    for index in range(start, end + 1):
        line = lines[index]
        match = re.match(r"\s*pub (?:const )?fn (\w+)\s*(<[^>]*>)?\s*\(([^)]*)\)\s*(->[^\{]*)", line)
        if match:
            generics = match.group(2) or ""
            returns = match.group(4).strip()
            print(f"  {index + 1:5}  fn {match.group(1)}{generics}({match.group(3).strip()}) {returns}")
    print()

    # The ValueType enum, which is what dtype carries.
    vtype = root / "src" / "value" / "type.rs"
    vtext = vtype.read_text(encoding="utf-8", errors="replace")
    print("ValueType, tensor-related:")
    match = re.search(r"pub enum ValueType\s*\{(.*?)\n\}", vtext, re.DOTALL)
    if match:
        for line in match.group(1).splitlines():
            stripped = line.strip()
            if stripped and not stripped.startswith("//"):
                print(f"  {stripped}")
    print()

    # The Session::inputs/outputs signatures.
    session = root / "src" / "session" / "mod.rs"
    stext = session.read_text(encoding="utf-8", errors="replace")
    print("Session accessors:")
    for match in re.finditer(r"pub fn (inputs|outputs)\s*\(([^)]*)\)\s*(->[^\{]*)", stext):
        print(f"  fn {match.group(1)}({match.group(2).strip()}) {match.group(3).strip()}")
    print()

    # And ::run, to get the argument shape right.
    print("Session::run:")
    for match in re.finditer(r"pub fn run\s*(<[^>]*>)\s*\(([^)]*)\)\s*(->[^\{]*)", stext):
        print(f"  fn run{match.group(1)}({match.group(2).strip()}) {match.group(3).strip()}")
    print()

    # Tensor::from_array and try_extract_tensor, already confirmed, reprinted
    # here so one script answers everything.
    print("Tensor::from_array:")
    create = root / "src" / "value" / "impl_tensor" / "create.rs"
    if create.exists():
        ctext = create.read_text(encoding="utf-8", errors="replace")
        for match in re.finditer(r"pub fn from_array\s*(<[^>]*>)?\s*\(([^)]*)\)\s*(->[^\{]*)", ctext):
            print(f"  fn from_array({match.group(2).strip()}) {match.group(3).strip()}")

    print()
    print("try_extract_tensor:")
    extract = root / "src" / "value" / "impl_tensor" / "extract.rs"
    if extract.exists():
        etext = extract.read_text(encoding="utf-8", errors="replace")
        for match in re.finditer(r"pub fn try_extract_tensor\s*(<[^>]*>)?\s*\(([^)]*)\)\s*(->[^\{]*)", etext):
            print(f"  fn try_extract_tensor({match.group(2).strip()}) {match.group(3).strip()}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
