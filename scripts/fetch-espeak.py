"""Puts a prebuilt espeak-ng beside the voice assets.

`espeak-rs-sys` compiles espeak-ng from source at build time, which needs
`libclang` *and* a full C toolchain with the platform's standard headers. On
Windows that means MSVC Build Tools — a large dependency for a build that only
ever calls four functions.

The `espeakng-loader` package on PyPI ships a prebuilt `espeakng.dll` and the
`espeak-ng-data` directory it needs. That removes the C toolchain, the bindgen
step, and the `espeak-rs` dependency together, and it matches how the reference
implementation locates these files.

Writes into `%USERPROFILE%\\.loom\\espeak\\`:
    espeakng.dll
    espeak-ng-data\\...

Stdlib only: no pip, no admin, no shell quoting.
"""

import json
import os
import pathlib
import sys
import urllib.request
import zipfile

DEST = pathlib.Path(os.environ["USERPROFILE"]) / ".loom" / "espeak"
API = "https://pypi.org/pypi/espeakng-loader/json"


def find_wheel():
    """The newest wheel, preferring a Windows build for this interpreter."""
    with urllib.request.urlopen(API, timeout=60) as response:
        meta = json.load(response)

    candidates = []
    for version, files in meta["releases"].items():
        for entry in files:
            name = entry["filename"]
            if entry.get("yanked") or not name.endswith(".whl"):
                continue
            # Prefer win_amd64; a pure-python wheel would carry no DLL.
            if "win_amd64" in name:
                candidates.append((2, version, name, entry["url"], entry["size"]))
            elif "py3-none-any" in name or "none-any" in name:
                candidates.append((1, version, name, entry["url"], entry["size"]))

    if not candidates:
        raise SystemExit("no usable espeakng-loader wheel found on PyPI")

    def key(entry):
        rank, version = entry[0], entry[1]
        parts = []
        for chunk in version.split("."):
            digits = "".join(c for c in chunk if c.isdigit())
            parts.append(int(digits) if digits else 0)
        return (rank, parts)

    candidates.sort(key=key)
    return candidates[-1]


def extract(wheel_path):
    DEST.mkdir(parents=True, exist_ok=True)
    found_dll = None
    found_data = None

    with zipfile.ZipFile(wheel_path) as archive:
        names = archive.namelist()
        print(f"  {len(names)} entries in the wheel")

        for name in names:
            lowered = name.lower()

            if lowered.endswith((".dll", ".so", ".dylib")) and (
                "espeak" in lowered or "sonic" in lowered
            ):
                target = DEST / pathlib.Path(name).name
                target.write_bytes(archive.read(name))
                print(f"  lib  {pathlib.Path(name).name}  ({target.stat().st_size:,} bytes)")
                if "espeak" in lowered and lowered.endswith(".dll"):
                    found_dll = target
                continue

            # The data directory is needed at run time; espeak-ng will not
            # initialise without it and reports that as a bare error code.
            if "espeak-ng-data/" in name and not name.endswith("/"):
                relative = name.split("espeak-ng-data/", 1)[1]
                target = DEST / "espeak-ng-data" / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(archive.read(name))
                found_data = DEST / "espeak-ng-data"

    if found_dll is None:
        raise SystemExit("no espeak-ng library in the wheel")
    if found_data is None:
        raise SystemExit("no espeak-ng-data directory in the wheel")

    print(f"  data {found_data}  ({sum(1 for _ in found_data.rglob('*'))} entries)")
    return found_dll


def main():
    existing = list(DEST.glob("*.dll")) if DEST.exists() else []
    if existing and (DEST / "espeak-ng-data").exists():
        for dll in existing:
            print(f"already present: {dll} ({dll.stat().st_size:,} bytes)")
        return 0

    rank, version, name, url, size = find_wheel()
    print(f"espeakng-loader : {version}")
    print(f"wheel           : {name} ({size:,} bytes)")
    print("downloading...")

    scratch = pathlib.Path(os.environ.get("TEMP", ".")) / name
    urllib.request.urlretrieve(url, scratch)

    dll = extract(scratch)
    scratch.unlink(missing_ok=True)

    print()
    print(f"wrote: {dll}")
    print(f"       {dll.parent / 'espeak-ng-data'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
