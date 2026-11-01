# Build an unsigned macOS app locally

This packages the **macOS MVP** locally. The first released download,
v0.1.0, is for Apple silicon.
The bundle contains the optimized `kver` binary and
the existing macOS icon; it has no Developer ID signature or notarization
(the macOS linker may apply an ad-hoc signature). Build on the Mac where you
intend to run it (Apple silicon and Intel builds are architecture-specific).

## Build

Install Rust 1.92 or newer and Xcode with its Metal Toolchain component.
From a checkout of this repository:

```sh
sh scripts/build-macos-app.sh
```

The script runs `cargo +stable build --release --locked --bin kver` and creates
`target/release/Kver.app`. Re-running replaces the bundle with the new binary
and icon. If `CARGO_TARGET_DIR` is set, the bundle and release binary are instead
under `$CARGO_TARGET_DIR/release/`. The app has no bundled PDF and no PDF file
association.

For a versioned Apple silicon archive, run `sh scripts/package-macos-app.sh`
on an Apple silicon Mac. It builds the same app, checks the bundle version
against `Cargo.toml` and the executable's `arm64` architecture, then extracts
and checks `target/release/Kver-vX.Y.Z-macos-arm64.zip`. It does not upload
or publish anything. The tag-only release job runs this same packaging path;
routine macOS PR CI is paused while the repository is private.

## Download an unsigned release

Download `Kver-v0.1.0-macos-arm64.zip` from
the project's [GitHub Releases](https://github.com/JonGretar/Kver/releases).
Use an Apple silicon Mac; this is not an Intel or supported Linux build.
Extract the archive and move `Kver.app` to Applications, or run it directly
from the extracted folder. Open it and choose a PDF with **Open PDF**.

Kver is **not Developer ID signed or notarized**. macOS may block a downloaded
copy with a security warning. Check that you obtained the archive from the
project's release page before overriding the warning. If you trust it,
control-click `Kver.app` in Finder and choose **Open**, or, after a blocked
launch, go to **System Settings → Privacy & Security → Open Anyway** and
confirm. Do not disable Gatekeeper globally. The release workflow checks the
archive's structure and architecture; it does not establish the identity of
a signed developer or guarantee that a document is safe.

## Release trigger (maintainers)

The [release workflow](../.github/workflows/release.yml) runs **only** when a
`vX.Y.Z` tag is pushed. A normal push to `main` and PR CI cannot publish a
release. Do not push a release tag without explicit authorization for that
release. Before an authorized tag, move release notes from `Unreleased` into
`## [X.Y.Z] - YYYY-MM-DD`, leave a new `Unreleased` section, and set the
`Cargo.toml` package version to `X.Y.Z`. Publish the tag only after the
release commit has been reviewed. The job rejects any tag that does not
match the package version and exactly one valid dated changelog heading
before it builds or publishes. It then builds and inspects the archive
with `scripts/package-macos-app.sh` and creates the GitHub Release.
This does not replace the real-Mac window and reading check in issue #18.
No experimental Linux artifact is attached: the current Linux build depends
on host GUI and software-rendering libraries, and portable packaging/runtime
instructions have not been validated. Linux retains its separate delivery gate.

## Open and check it

From the repository root, launch without a PDF path and choose a document
using **Open PDF** in the window:

```sh
open -n -a "$PWD/target/release/Kver.app"
```

If using `CARGO_TARGET_DIR`, substitute its release bundle path. Verify that
the file picker lets you choose `examples/readme-sample.pdf`, the sample renders,
the Kver icon appears in Finder/Dock, and left/right or `p`/`n` changes pages
and `−`/`+` changes zoom. Choose another PDF with **Open PDF**; invalid and
unreadable documents should show an error and leave the window usable. For
path-based opening, pass an **absolute** path:

```sh
open -n -a "$PWD/target/release/Kver.app" --args "$PWD/examples/readme-sample.pdf"
```

`open` returning successfully
only means macOS accepted the launch request; check the visible window as well.

Double-clicking `Kver.app` in Finder opens the empty reader window; use
**Open PDF** to choose a document. PDF file association in Finder is not yet supported.

To inspect the package independently:

```sh
plutil -lint target/release/Kver.app/Contents/Info.plist
file target/release/Kver.app/Contents/MacOS/kver
ls -l target/release/Kver.app/Contents/Resources/kver.icns
```

The [tag-only release job](../.github/workflows/release.yml) builds the same
archive and checks its plist, executable architecture, and icon; routine macOS
PR CI is paused while the repository is private. On a local Apple silicon Mac,
an earlier bundle build completed, `open` launched the bundled executable with
the sample PDF, and macOS reported an on-screen window. The local user
confirmed the app worked and the icon appeared. The v0.1.0 local-candidate
and downloaded-app checks are recorded in the
[MVP validation evidence](mvp-reader-validation.md); they do not verify
behavior on other Macs.

## Troubleshooting and macOS security

- If `xcrun --find metal` fails or the build reports a missing Metal toolchain,
  run `xcodebuild -downloadComponent MetalToolchain`, then retry the build.
  Use `rustup update stable` if Rust is too old. The first release build can
  take considerably longer than a debug build.
- If no window opens, run
  `target/release/Kver.app/Contents/MacOS/kver /absolute/path/to/file.pdf`
  directly from Terminal to see PDF loading or window errors. This
  tests the bundled executable but not Launch Services.
- A local build usually has no quarantine warning. A copied or downloaded
  unsigned bundle may be blocked by Gatekeeper, with wording varying by macOS
  version. Only if you trust the source, control-click `Kver.app` in Finder and
  choose **Open**, or use **System Settings → Privacy & Security → Open Anyway**
  after a blocked launch, then confirm the prompt. Do not disable Gatekeeper
  globally. Neither signing nor notarization is part of this build.
- If the icon does not update after rebuilding, inspect `Info.plist` and
  `Contents/Resources/kver.icns` above; Finder/Dock may cache an older icon.
