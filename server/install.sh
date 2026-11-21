#!/bin/sh
# Installs loom on a Linux server. It only fetches the newest release of the
# `loom` program for this machine and checks it against the release's
# manifest; `loom install` (server/setup.calx) does the rest: the loom user,
# the service, and Caddy or Tailscale in front.
#
#   curl -fsSL https://github.com/aaen-studios/loom/releases/latest/download/install.sh | sudo sh
#   curl -fsSL …/install.sh | sudo sh -s -- --domain loom.example.com
#
# Every argument goes on to `loom install`. LOOM_RELEASES names another place
# to fetch from, such as a release of a fork.
set -eu

base="${LOOM_RELEASES:-https://github.com/aaen-studios/loom/releases/latest/download}"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64 | Linux-amd64) kind=linux-server ;;
  *)
    echo "loom: there's no release of loom for $(uname -s) on $(uname -m) yet" >&2
    exit 1
    ;;
esac

if [ "$(id -u)" != 0 ]; then
  echo "loom: run this as root, with sudo, to make the loom user and the service" >&2
  exit 1
fi

for tool in curl sha256sum; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "loom: this needs $tool" >&2
    exit 1
  fi
done

tmp="$(mktemp -d)"
trap 'rm -rf -- "$tmp"' EXIT

curl -fsSL "$base/manifest.json" -o "$tmp/manifest.json"
entry="$(sed -n "s/.*\"$kind\":{\([^}]*\)}.*/\1/p" "$tmp/manifest.json")"
url="$(printf '%s' "$entry" | sed -n 's/.*"url":"\([^"]*\)".*/\1/p')"
want="$(printf '%s' "$entry" | sed -n 's/.*"sha256":"\([0-9a-f]*\)".*/\1/p')"
if [ -z "$url" ] || [ -z "$want" ]; then
  echo "loom: the release names no program for $kind" >&2
  exit 1
fi

curl -fsSL "$url" -o "$tmp/loom"
got="$(sha256sum "$tmp/loom" | cut -d ' ' -f 1)"
if [ "$got" != "$want" ]; then
  echo "loom: the program downloaded isn't the one the release names, so it isn't used" >&2
  exit 1
fi
chmod 755 "$tmp/loom"

"$tmp/loom" install "$@"
