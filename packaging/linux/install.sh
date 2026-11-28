#!/bin/sh
# Download the latest Linux release and hand it to its packaged installer.
set -eu

fail() {
    printf '%s\n' "Kver install: $*" >&2
    exit 1
}

[ "$#" -eq 0 ] || fail "This script takes no arguments. Choose a specific version on GitHub Releases."
[ "$(uname -sm)" = "Linux x86_64" ] || fail "The download installer supports Linux x86-64 only."
for tool in curl python3 sha256sum tar mktemp grep; do
    command -v "$tool" >/dev/null 2>&1 || fail "Required command not found: $tool"
done
python3 -c 'import sys; sys.exit(sys.version_info < (3, 9))' ||
    fail "Python 3.9 or newer is required to install Kver."

download() {
    curl -fsSL --proto '=https' --proto-redir '=https' --connect-timeout 15 "$@"
}

releases=https://github.com/JonGretar/Kver/releases
latest=$(download -o /dev/null -w '%{url_effective}' "$releases/latest") ||
    fail "Could not look up the latest stable release."
case "$latest" in
    "$releases/tag/"*) tag=${latest#"$releases/tag/"} ;;
    *) fail "GitHub did not return a Kver release tag." ;;
esac
case "$tag" in *[!v0-9.]*) fail "Unexpected release tag." ;; esac
printf '%s\n' "$tag" | grep -Ex 'v[0-9]+\.[0-9]+\.[0-9]+' >/dev/null ||
    fail "Unexpected release tag: $tag"

tmp=$(cd "${TMPDIR:-/tmp}" && pwd) || fail "Invalid temporary directory."
stage=$(mktemp -d "$tmp/kver-install.XXXXXXXX") ||
    fail "Could not create a temporary directory."
trap 'rm -rf -- "$stage"' 0
trap 'exit 1' HUP INT TERM
cd "$stage"

bundle="Kver-$tag-linux-x86_64"
base_url="$releases/download/$tag"
missing() {
    fail "Could not download the Linux archive and checksum for $tag. They may not be published yet. See $latest."
}

if ! status=$(download -o "$bundle.tar.gz.sha256" -w '%{http_code}' \
    "$base_url/$bundle.tar.gz.sha256" 2>checksum-download.log); then
    if [ "$status" != 404 ]; then
        cat checksum-download.log >&2
        missing
    fi
    # Older releases used a different filename. Never fall back to an older tag.
    bundle="Kver-$tag-experimental-linux-x86_64"
    download -o "$bundle.tar.gz.sha256" "$base_url/$bundle.tar.gz.sha256" || missing
fi
archive="$bundle.tar.gz"
download -o "$archive" "$base_url/$archive" || missing

# Hash only the archive we requested, not paths supplied by the checksum file.
actual=$(sha256sum "$archive") || fail "Could not hash the Linux archive."
[ "$actual" = "$(cat "$archive.sha256")" ] ||
    fail "Checksum verification failed for $archive. Nothing was installed."

tar -xzf "$archive" || fail "Could not extract $archive."
[ -f "$bundle/install.py" ] || fail "The archive does not contain the Kver installer."
python3 "$bundle/install.py" install
