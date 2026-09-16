"""Lists the ONNX Runtime releases, to pick one matching the `ort` crate.

`ort` 2.0.0-rc.13 wraps ONNX Runtime **1.28**, and the version on this machine
is **1.22.0**. Loading and running works; cleaning up aborts the process with
STATUS_STACK_BUFFER_OVERRUN, which is what ONNX Runtime raises on an
unrecoverable internal error. So the two need to be brought into line.

Run: python scripts/check-ort-versions.py
"""

import json
import re
import sys
import urllib.request

API = "https://api.github.com/repos/microsoft/onnxruntime/releases?per_page=30"


def main():
    request = urllib.request.Request(API, headers={"User-Agent": "loom-voice"})
    with urllib.request.urlopen(request, timeout=60) as response:
        releases = json.load(response)

    print("tag        published     windows x64 asset")
    candidates = []
    for release in releases:
        tag = release.get("tag_name", "")
        if not tag.startswith("v1."):
            continue
        published = (release.get("published_at") or "")[:10]
        prerelease = release.get("prerelease", False)

        asset = None
        for entry in release.get("assets", []):
            if entry["name"] == "onnxruntime-win-x64-{}.zip".format(tag.lstrip("v")):
                asset = entry
                break

        mark = "pre" if prerelease else "   "
        present = "yes" if asset else "no "
        size = f"{asset['size'] / 1e6:.0f} MB" if asset else ""
        print(f"{mark} {tag:<9} {published}  {present}  {size}")

        if asset and not prerelease:
            candidates.append((tag, asset["browser_download_url"]))

    print()
    # What ort rc.13 was built against.
    lock = None
    try:
        import os
        import pathlib

        registry = pathlib.Path(os.environ["USERPROFILE"]) / ".cargo/registry/src"
        for path in registry.glob("*/ort-sys-*/Cargo.toml"):
            text = path.read_text(encoding="utf-8", errors="replace")
            match = re.search(r'^version\s*=\s*"([^"]+)"', text, re.MULTILINE)
            if match and "2.0.0-rc" in str(path):
                lock = match.group(1)
    except Exception:  # noqa: BLE001
        pass

    print("the `ort` crate 2.0.0-rc.13 documents wrapping ONNX Runtime 1.28.")
    if candidates:
        print()
        print("newest stable with a Windows x64 zip:")
        print(f"  {candidates[0][0]}  {candidates[0][1]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
