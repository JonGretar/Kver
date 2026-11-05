#!/bin/sh
# Fetch a pinned, checksum-verified SDK into ignored build output.
set -eu
cd "$(dirname "$0")/.."
version=2.10.1
sha256=1a856d2a9edb8e628de88121d7be04cf0e2167f8641650b78fca00b14244cf27
destination="${CARGO_TARGET_DIR:-target}/sparkle/$version"
if [ -f "$destination/.verified-$sha256" ]; then
    printf '%s\n' "$destination"
    exit 0
fi
mkdir -p "$(dirname "$destination")"
stage=$(mktemp -d "$(dirname "$destination")/.sparkle.XXXXXX")
trap 'rm -rf "$stage"' EXIT HUP INT TERM
curl --fail --location --retry 3 --proto '=https' --tlsv1.2 \
    "https://github.com/sparkle-project/Sparkle/releases/download/$version/Sparkle-$version.tar.xz" \
    -o "$stage/sdk.tar.xz"
printf '%s  %s\n' "$sha256" "$stage/sdk.tar.xz" | shasum -a 256 -c - >&2
mkdir "$stage/sdk"
tar -xf "$stage/sdk.tar.xz" -C "$stage/sdk"
test -x "$stage/sdk/bin/generate_appcast"
test -d "$stage/sdk/Sparkle.framework"
touch "$stage/sdk/.verified-$sha256"
rm -rf "$destination"
mv "$stage/sdk" "$destination"
printf '%s\n' "$destination"
