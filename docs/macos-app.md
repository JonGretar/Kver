# Build an unsigned macOS app locally

This packages the **current continuous reader prototype**, not a downloadable
release or a completed MVP. The bundle contains the optimized `kver` binary and
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

The [macOS CI job](../.github/workflows/ci.yml) builds the same bundle and checks
the plist, executable, and icon. On a local Apple silicon Mac, the build
completed, `open` launched the bundled executable with the sample PDF, and
macOS reported an on-screen window. The local user confirmed the app worked
and the icon appeared. CI's structural checks do not replace this visual check
or verify behavior on other Macs.

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
