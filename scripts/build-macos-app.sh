#!/bin/sh
# Build a local, ad-hoc-signed macOS bundle. Run from any directory on a Mac.
set -eu

if [ "$(uname -s)" != "Darwin" ]; then
    echo "This bundle must be built on macOS." >&2
    exit 1
fi

cd "$(dirname "$0")/.."
version=$(awk '
    /^\[package\]$/ { in_package = 1; next }
    /^\[/ { in_package = 0 }
    in_package && /^version[[:space:]]*=/ {
        gsub(/^[^"]*"|"[^"]*$/, "")
        print
        exit
    }
' Cargo.toml)
if [ -z "$version" ]; then
    echo "Could not read package version from Cargo.toml." >&2
    exit 1
fi

# Local builds omit the updater unless a public verification key is supplied.
# Validate before building/downloading; never consume a private signing key here.
sparkle_dir=
if [ -n "${KVER_UPDATE_PUBLIC_KEY:-}" ]; then
    python3 scripts/update-metadata.py
    sparkle_dir=$(sh scripts/fetch-sparkle.sh)
elif [ -n "${KVER_UPDATE_FEED_URL:-}" ]; then
    echo "KVER_UPDATE_FEED_URL requires KVER_UPDATE_PUBLIC_KEY." >&2
    exit 1
fi

if command -v corgi >/dev/null 2>&1; then
    corgi build --release --bin kver --target-dir "${CARGO_TARGET_DIR:-target}"
else
    cargo +stable build --release --locked --bin kver
fi

release_dir="${CARGO_TARGET_DIR:-target}/release"
bundle="$release_dir/Kver.app"
stage=$(mktemp -d "$release_dir/.Kver.app.XXXXXX")
trap 'rm -rf "$stage"' EXIT HUP INT TERM
mkdir -p "$stage/Contents/MacOS" "$stage/Contents/Resources"
install -m 755 "$release_dir/kver" "$stage/Contents/MacOS/kver"
install -m 644 assets/icons/kver.icns "$stage/Contents/Resources/kver.icns"
cat > "$stage/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleDisplayName</key><string>Kver</string>
    <key>CFBundleExecutable</key><string>kver</string>
    <key>CFBundleIdentifier</key><string>io.github.jongretar.kver</string>
    <key>CFBundleIconFile</key><string>kver.icns</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>Kver</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key><string>PDF document</string>
            <key>CFBundleTypeRole</key><string>Viewer</string>
            <key>LSHandlerRank</key><string>Alternate</string>
            <key>LSItemContentTypes</key>
            <array><string>com.adobe.pdf</string></array>
        </dict>
    </array>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
</dict>
</plist>
EOF

if [ -n "$sparkle_dir" ]; then
    mkdir -p "$stage/Contents/Frameworks"
    # ditto preserves Sparkle's framework and helper symlinks and signatures.
    ditto "$sparkle_dir/Sparkle.framework" "$stage/Contents/Frameworks/Sparkle.framework"
    install -m 644 "$sparkle_dir/LICENSE" "$stage/Contents/Resources/Sparkle-LICENSE"
    python3 scripts/update-metadata.py "$stage/Contents/Info.plist"
fi

plutil -lint "$stage/Contents/Info.plist"
# Replace the linker's executable-only signature after all bundle files exist.
# Ad-hoc signing seals the bundle; it does not provide Developer ID trust.
codesign --force --sign - "$stage"
codesign --verify --deep --strict --verbose=2 "$stage"
rm -rf "$bundle"
mv "$stage" "$bundle"
trap - EXIT HUP INT TERM
echo "Built ad-hoc-signed bundle (not notarized): $bundle"
