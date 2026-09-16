"""Downloads ONNX Runtime for local voice-mode development.

Voice mode loads `onnxruntime.dll` dynamically rather than linking it, so the
DLL has to exist on the machine. This puts it under `%USERPROFILE%\\.loom\\ort\\`,
which is where `voice::tts::Paths` looks.

# The version matters

The `ort` crate documents which ONNX Runtime it wraps, and a mismatch is not
benign: 2.0.0-rc.13 wraps **1.28**, and running it against 1.22.0 loaded and
inferred fine but aborted the process during teardown with
`STATUS_STACK_BUFFER_OVERRUN` — which is what ORT raises on an unrecoverable
internal error. So the version here is pinned to match the crate, and the
already-present check includes it, so bumping one fetches the other.

Python rather than `cmd` because the download URL contains `?` and `&`, and
passing those through `cmd.exe`'s parser produced silent zero-byte downloads.

Stdlib only.
"""

import os
import pathlib
import shutil
import sys
import urllib.request
import zipfile

# Must match what the `ort` crate wraps. See the module note above.
VERSION = "1.28.2"
DEST = pathlib.Path(os.environ["USERPROFILE"]) / ".loom" / "ort"


def library_name():
    if sys.platform == "win32":
        return "onnxruntime.dll"
    if sys.platform == "darwin":
        return "libonnxruntime.dylib"
    return "libonnxruntime.so"


def archive_name():
    if sys.platform == "win32":
        return f"onnxruntime-win-x64-{VERSION}.zip"
    if sys.platform == "darwin":
        # Apple silicon is the common case for a Mac dev machine.
        return f"onnxruntime-osx-arm64-{VERSION}.tgz"
    return f"onnxruntime-linux-x64-{VERSION}.tgz"


def already_present():
    """Finds the library for *this* version, flat or in a versioned directory.

    Version-aware on purpose: the `ort` crate wraps a specific ONNX Runtime, and
    a mismatched pair loads but aborts on teardown. Checking only for the file
    name would keep an older runtime in place after a version bump.
    """
    name = library_name()
    if (DEST / name).exists():
        # The flat copy is only trustworthy if its source directory matches.
        marker = DEST / f".version-{VERSION}"
        if marker.exists():
            return DEST / name

    expected = DEST / f"onnxruntime-{platform_tag()}-{VERSION}"
    nested = expected / "lib" / name
    if nested.exists():
        return nested

    return None


def platform_tag():
    if sys.platform == "win32":
        return "win-x64"
    if sys.platform == "darwin":
        return "osx-arm64"
    return "linux-x64"


def extract(archive, into):
    into.mkdir(parents=True, exist_ok=True)
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as bundle:
            bundle.extractall(into)
    else:
        # tarfile handles .tgz and is standard library.
        import tarfile

        with tarfile.open(archive) as bundle:
            bundle.extractall(into)


def main():
    found = already_present()
    if found:
        print(f"already present: {found} ({found.stat().st_size:,} bytes)")
        print(f"version: {VERSION}")
        return 0

    # A stale copy from a different version would shadow the new one, since the
    # loader looks for the flat name first.
    stale = DEST / library_name()
    if stale.exists():
        print(f"replacing a mismatched runtime at {stale}")
        stale.unlink()

    name = archive_name()
    url = (
        f"https://github.com/microsoft/onnxruntime/releases/download/"
        f"v{VERSION}/{name}"
    )

    print(f"ONNX Runtime : {VERSION}")
    print(f"archive      : {name}")
    print(f"url          : {url}")
    print("downloading...")

    DEST.mkdir(parents=True, exist_ok=True)
    scratch = pathlib.Path(os.environ.get("TEMP", ".")) / name

    with urllib.request.urlopen(url, timeout=300) as response:
        total = int(response.headers.get("Content-Length") or 0)
        written = 0
        with open(scratch, "wb") as out:
            while True:
                chunk = response.read(1 << 20)
                if not chunk:
                    break
                out.write(chunk)
                written += len(chunk)
                if total:
                    percent = written * 100 // total
                    print(f"\r  {written / 1e6:6.1f} / {total / 1e6:.1f} MB ({percent}%)", end="")
    print()

    print("extracting...")
    extract(scratch, DEST)
    scratch.unlink(missing_ok=True)

    found = already_present()
    if not found:
        print("extraction produced no library.")
        for path in sorted(DEST.rglob("*")):
            if path.is_file():
                print(f"  {path.relative_to(DEST)}")
        return 1

    # Flatten if it landed in a nested directory, so the path is predictable.
    flat = DEST / library_name()
    if found != flat:
        shutil.copy2(found, flat)
        print(f"copied to {flat}")

    print(f"wrote: {flat} ({flat.stat().st_size:,} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
