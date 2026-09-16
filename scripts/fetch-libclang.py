"""Puts libclang.dll in place without a 900 MB LLVM install.

**Retained for reference only.** Voice mode no longer needs this: the espeak
bindings were dropped in favour of hand-written FFI over a prebuilt library, so
nothing runs bindgen any more. See `crates/loom-core/src/voice/espeak.rs`.

It is kept because the technique is worth having when a crate does need
bindgen: the official LLVM release for Windows is ~900 MB of compiler the build
never touches, while PyPI's `libclang` wheel is ~26 MB and contains exactly the
one DLL.

Writes `libclang.dll` to `%USERPROFILE%\\.loom\\llvm\\bin\\`.

Stdlib only: no pip, no wheel, no shell quoting.
"""

import json
import os
import pathlib
import sys
import urllib.request
import zipfile

DEST = pathlib.Path(os.environ["USERPROFILE"]) / ".loom" / "llvm" / "bin"
API = "https://pypi.org/pypi/libclang/json"


def find_wheel():
    """The newest Windows 64-bit wheel, preferring our own interpreter version."""
    with urllib.request.urlopen(API, timeout=60) as response:
        meta = json.load(response)

    candidates = []
    for version, files in meta["releases"].items():
        for entry in files:
            name = entry["filename"]
            if entry.get("yanked"):
                continue
            if not name.endswith(".whl"):
                continue
            if "win_amd64" not in name:
                continue
            candidates.append((version, name, entry["url"], entry["size"]))

    if not candidates:
        raise SystemExit("no win_amd64 libclang wheel found on PyPI")

    # Version strings are dotted ints; sort numerically rather than lexically
    # so 18.1.8 does not beat 9.0.1 by string comparison.
    def key(entry):
        parts = []
        for chunk in entry[0].split("."):
            digits = "".join(c for c in chunk if c.isdigit())
            parts.append(int(digits) if digits else 0)
        return parts

    candidates.sort(key=key)
    return candidates[-1]


def extract(wheel_path):
    """Pulls libclang.dll out of the wheel, wherever it sits."""
    DEST.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(wheel_path) as archive:
        names = archive.namelist()
        matches = [n for n in names if n.lower().endswith("libclang.dll")]
        if not matches:
            # Show what is there so a layout change is diagnosable.
            print("no libclang.dll in the wheel. Contents:")
            for name in names[:40]:
                print(f"  {name}")
            raise SystemExit(1)

        # Prefer the canonical location over a stray copy.
        matches.sort(key=lambda n: (0 if "native" in n else 1, len(n)))
        chosen = matches[0]
        print(f"extracting {chosen}")
        with archive.open(chosen) as source:
            target = DEST / "libclang.dll"
            target.write_bytes(source.read())
    return DEST / "libclang.dll"


def main():
    target = DEST / "libclang.dll"
    if target.exists():
        print(f"already present: {target} ({target.stat().st_size:,} bytes)")
        return 0

    version, name, url, size = find_wheel()
    print(f"PyPI libclang  : {version}")
    print(f"wheel          : {name} ({size:,} bytes)")
    print(f"downloading...")

    scratch = pathlib.Path(os.environ.get("TEMP", ".")) / name
    urllib.request.urlretrieve(url, scratch)

    written = extract(scratch)
    scratch.unlink(missing_ok=True)

    print(f"wrote          : {written} ({written.stat().st_size:,} bytes)")
    print()
    print("Set this before building:")
    print(f"  set LIBCLANG_PATH={DEST}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
