#!/bin/sh
# Native Ubuntu build and verified, non-publishing Linux archive.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$root"
[ "$(uname -s)" = Linux ] && [ "$(uname -m)" = x86_64 ] || {
    echo "Run on native x86-64 Ubuntu Linux (not a cross-build)." >&2
    exit 1
}
. /etc/os-release
[ "$ID" = ubuntu ] && [ "$VERSION_ID" = 24.04 ] || {
    echo "The Linux build baseline is Ubuntu 24.04." >&2
    exit 1
}
for tool in cargo rustc python3 readelf ldd cargo-about; do
    command -v "$tool" >/dev/null
done
[ "$(rustc +stable -vV | sed -n 's/^host: //p')" = x86_64-unknown-linux-gnu ]
[ "$(cargo-about --version)" = "cargo-about 0.9.3" ]
cargo +stable build --locked --release --target x86_64-unknown-linux-gnu --bin kver
release="${CARGO_TARGET_DIR:-target}/x86_64-unknown-linux-gnu/release"
cargo +stable about generate --locked --fail -c packaging/linux/about.toml \
    -o "$release/THIRD-PARTY-LICENSES.html" packaging/common/licenses.hbs
python3 packaging/linux/package.py --release-dir "$release" \
    --licenses "$release/THIRD-PARTY-LICENSES.html"
