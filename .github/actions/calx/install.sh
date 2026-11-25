#!/bin/sh
# Installs the Calx loom is pinned to, on Linux or macOS, into ~/.calx or
# CALX_INSTALL, which mustn't be there yet:
#
#     sh .github/actions/calx/install.sh .calx-version
#
# The pin's first line is the version, and its other lines the SHA-256 of each
# system's archive. The archive comes from loom's copy of Calx's releases
# (aaen-studios/loom-calx), or else from Calx's own while it still has it, and
# is installed only if its SHA-256 is the pin's. The toolchain it builds with
# comes the same way, through `calx update --toolchain`, which checks it against
# the SHA-256 the archive names. CI (action.yml) and the Dockerfile run it.
set -eu

pin=${1:-.calx-version}
[ -f "$pin" ] || { echo "calx install: no $pin" >&2; exit 1; }
version=$(sed -n 1p "$pin" | tr -d '\r')
case "$(uname -s) $(uname -m)" in
    "Linux x86_64") platform=linux-x64 ;;
    "Linux aarch64" | "Linux arm64") platform=linux-arm64 ;;
    "Darwin arm64") platform=macos-arm64 ;;
    *) echo "calx install: no Calx release for $(uname -s) $(uname -m)" >&2; exit 1 ;;
esac
file="calx-$version-$platform.tar.gz"
want=$(awk -v f="$file" '$2 == f { print $1 }' "$pin")
[ -n "$want" ] || { echo "calx install: $pin has no SHA-256 for $file" >&2; exit 1; }
case "$version" in
    *-nightly.*) tag=nightly ;;
    *) tag="v$version" ;;
esac
if command -v sha256sum > /dev/null 2>&1; then
    sha256() { sha256sum "$1" | cut -d' ' -f1; }
else
    sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
fi
root=${CALX_INSTALL:-$HOME/.calx}
if [ -e "$root" ]; then
    echo "calx install: $root is there already; this installs only where nothing is" >&2
    exit 1
fi

temp=$(mktemp -d)
trap 'rm -rf "$temp"' EXIT
from=""
for releases in https://github.com/aaen-studios/loom-calx/releases https://github.com/ellipog/calx/releases; do
    if curl -fsSL --retry 5 --retry-all-errors --retry-delay 3 -A loom-ci -o "$temp/$file" "$releases/download/$tag/$file"; then
        got=$(sha256 "$temp/$file")
        if [ "$got" = "$want" ]; then
            from=$releases
            break
        fi
        echo "calx install: $releases/download/$tag/$file has SHA-256 $got, not the pin's $want" >&2
    else
        echo "calx install: $releases has no $tag/$file" >&2
    fi
done
[ -n "$from" ] || { echo "calx install: can't download $file with SHA-256 $want" >&2; exit 1; }
echo "Calx $version from $from, SHA-256 checked"

mkdir -p "$temp/unpacked" "$(dirname "$root")"
tar -xzf "$temp/$file" -C "$temp/unpacked"
[ -x "$temp/unpacked/calx/bin/calx" ] || { echo "calx install: $file has no calx/bin/calx" >&2; exit 1; }
mv "$temp/unpacked/calx" "$root"

# The toolchain, from the same place, checked against the SHA-256 in toolchain.txt.
tries=0
until CALX_RELEASES="$from" CALX_NO_UPDATE_CHECK=1 "$root/bin/calx" update --toolchain; do
    tries=$((tries + 1))
    [ "$tries" -lt 4 ] || { echo "calx install: can't install the toolchain" >&2; exit 1; }
    sleep $((tries * 5))
done

"$root/bin/calx" version | grep -qF " $version " || { echo "calx install: $root/bin/calx isn't Calx $version" >&2; exit 1; }
"$root/bin/calx" version
