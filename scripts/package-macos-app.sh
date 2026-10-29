#!/bin/sh
# Build and inspect an unsigned, versioned Apple silicon app archive without publishing.
set -eu

if [ "$(uname -s)" != Darwin ] || [ "$(uname -m)" != arm64 ]; then
    echo "Packaging requires an Apple silicon Mac." >&2
    exit 1
fi

cd "$(dirname "$0")/.."
version=$(python3 scripts/release-metadata.py --version)
sh scripts/build-macos-app.sh

release_dir="${CARGO_TARGET_DIR:-target}/release"
app="$release_dir/Kver.app"
contents="$app/Contents"
archive="$release_dir/Kver-v${version}-macos-arm64.zip"
test "$(lipo -archs "$contents/MacOS/kver")" = arm64
plutil -lint "$contents/Info.plist"
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$contents/Info.plist")" = "$version"
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$contents/Info.plist")" = "$version"
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$contents/Info.plist")" = kver
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIconFile' "$contents/Info.plist")" = kver.icns
test -s "$contents/Resources/kver.icns"

# Stage a single app at the archive root. Do not reuse an archive from an earlier build.
rm -f "$archive"
ditto -c -k --sequesterRsrc --keepParent "$app" "$archive"
stage=$(mktemp -d "${TMPDIR:-/tmp}/kver-archive.XXXXXX")
trap 'rm -rf "$stage"' EXIT HUP INT TERM
ditto -x -k "$archive" "$stage"
test -x "$stage/Kver.app/Contents/MacOS/kver"
test "$(lipo -archs "$stage/Kver.app/Contents/MacOS/kver")" = arm64
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$stage/Kver.app/Contents/Info.plist")" = "$version"
test -s "$stage/Kver.app/Contents/Resources/kver.icns"
cmp "$contents/MacOS/kver" "$stage/Kver.app/Contents/MacOS/kver"
cmp "$contents/Resources/kver.icns" "$stage/Kver.app/Contents/Resources/kver.icns"
echo "Verified unsigned app archive: $archive"
