#!/bin/sh
# Installs the Calx loom is pinned to, on Linux or macOS, into ~/.calx or
# CALX_INSTALL, which mustn't be there yet:
#
#     sh .github/actions/calx/install.sh .calx-version
#
# The pin's first line is the version, and its other lines the SHA-256 of each
# system's archive. The archive comes from Calx's release of that version,
# vVERSION, which Calx keeps for every build, or else from its rolling nightly
# release, which had nightlies from before those, and is installed only if its
# SHA-256 is the pin's. The toolchain it builds with comes from the same
# releases and is installed only if its SHA-256 is the one the archive's
# toolchain.txt names. CI (action.yml) and the Dockerfile run it.
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
releases=https://github.com/ellipog/calx/releases
case "$version" in
    *-nightly.*) tags="v$version nightly" ;;
    *) tags="v$version" ;;
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

# fetch NAME SHA256: downloads NAME from the first of the releases that has it
# with that SHA-256, into $temp.
fetch() {
    for tag in $tags; do
        url="$releases/download/$tag/$1"
        if curl -fsSL --retry 5 --retry-all-errors --retry-delay 3 -A loom-ci -o "$temp/$1" "$url"; then
            got=$(sha256 "$temp/$1")
            if [ "$got" = "$2" ]; then
                echo "$1 from $url, SHA-256 checked"
                return 0
            fi
            echo "calx install: $url has SHA-256 $got, not $2" >&2
        else
            echo "calx install: no $url" >&2
        fi
    done
    echo "calx install: can't download $1 with SHA-256 $2" >&2
    exit 1
}

fetch "$file" "$want"
mkdir -p "$temp/unpacked" "$(dirname "$root")"
tar -xzf "$temp/$file" -C "$temp/unpacked"
[ -x "$temp/unpacked/calx/bin/calx" ] || { echo "calx install: $file has no calx/bin/calx" >&2; exit 1; }

# The toolchain, which the archive's toolchain.txt names: its file, then its SHA-256.
note="$temp/unpacked/calx/toolchain.txt"
if [ -f "$note" ]; then
    tc_file=$(sed -n 1p "$note" | tr -d '\r')
    tc_sum=$(sed -n 2p "$note" | tr -d '\r')
    fetch "$tc_file" "$tc_sum"
    tar -xJf "$temp/$tc_file" -C "$temp/unpacked/calx"
    rm -f -- "$temp/$tc_file"
    [ -x "$temp/unpacked/calx/toolchain/bin/clang" ] || { echo "calx install: $tc_file has no toolchain/bin/clang" >&2; exit 1; }
fi
mv "$temp/unpacked/calx" "$root"

"$root/bin/calx" version | grep -qF " $version " || { echo "calx install: $root/bin/calx isn't Calx $version" >&2; exit 1; }
"$root/bin/calx" version
