# Build an unsigned macOS app locally

This packages the **macOS MVP** locally. The first released download,
v0.1.0, is for Apple silicon.
The bundle contains the optimized `kver` binary and
the existing macOS icon; it has no Developer ID signature or notarization
(the build script ad-hoc signs the completed bundle). Build on the Mac where you
intend to run it (Apple silicon and Intel builds are architecture-specific).

## Build

Install Rust 1.92 or newer and Xcode with its Metal Toolchain component.
Install Corgi (`cargo install corgi-build`) to use its shared build cache;
the script falls back to Cargo if Corgi is not installed.
From a checkout of this repository:

```sh
sh scripts/build-macos-app.sh
```

The script builds the release binary with Corgi when available, or with
`cargo +stable build --release --locked --bin kver` otherwise, and creates
`target/release/Kver.app`. Re-running replaces the bundle with the new binary
and icon. If `CARGO_TARGET_DIR` is set, both tools place the bundle and
release binary under `$CARGO_TARGET_DIR/release/`. The app has no bundled PDF.
It declares PDF viewer support so macOS can offer it in Finder's **Open With** menu.

For a versioned Apple silicon archive, run `sh scripts/package-macos-app.sh`
on an Apple silicon Mac. It builds the same app, checks the bundle version
against `Cargo.toml` and the executable's `arm64` architecture, then extracts
and checks `target/release/Kver-vX.Y.Z-macos-arm64.zip`, including strict
signature verification before and after archiving. It does not upload
or publish anything. The tag-only release job runs this same packaging path;
run it before tagging a release to catch packaging failures. Ordinary macOS PR
CI builds the GUI binary but does not assemble or inspect an app archive.

## Recheck native quit protection after dependency upgrades

**Required when changing GPUI Kit, its GPUI/macOS backend, `objc2`, or Kver's
native quit adapter.** Record the check result in the upgrade PR; a passing
headless test suite alone does not establish native termination safety.

`src/app/quit_guard/macos.rs` supplies Cocoa's `applicationShouldTerminate:`
before GPUI's non-cancellable shutdown callback. It keeps GPUI's delegate object
and instance-variable layout, using an ivar-free runtime subclass for the veto.
It does not fork or patch GPUI. Startup rejects an unexpected delegate class,
layout, or a framework-owned veto so an upgrade cannot silently bypass review.

Before accepting a dependency upgrade:

1. Inspect the pinned backend's delegate creation, termination, and shutdown
   callbacks. Confirm that the adapter still preserves delegate identity,
   ivars, inherited methods, and optional-selector discovery. If GPUI gains a
   supported pre-quit veto, reconsider and preferably remove this adapter.
2. Run `corgi test --lib app::quit_guard` on macOS, then the ordinary library
   tests and strict Clippy checks. The runtime tests check subclass layout,
   inherited behavior, and one-shot approval invalidation.
3. In a real Kver window, create an unsaved highlight and choose **Quit** from
   the Dock. Cancel must leave the app, selection, and edits usable. Repeat
   Quit while the dialog is open; it must not stack duplicate decisions.
4. Repeat native Quit with **Save**. A writable original must save without a
   filename prompt. With a readonly original, Save must offer another location;
   cancelling the picker or failing the save must not quit or lose edits.
   Verify the resulting PDF before testing **Discard** in a separate run.
5. Exercise two windows of one dirty PDF plus an unrelated document. Quit must
   resolve each dirty session once, regardless of which window is active.
6. Recheck Command-Q, the Quit menu, last-window close, Finder file delivery,
   Dock reopen, menus, and update checking. The adapter must not break GPUI's
   other delegate callbacks.

Normal native Quit requests are covered by this guard. Force Quit, process
killing, crashes, and power failure cannot offer Save/Discard/Cancel; there is
no persistent recovery of unsaved annotations.

## Download an unsigned release

Download the current `Kver-vX.Y.Z-macos-arm64.zip` from
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
archive's structure, architecture, and ad-hoc signature; it does not establish the identity of
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
with `scripts/package-macos-app.sh` and creates the GitHub Release with that
version's changelog chapter in its notes. It also requires the update signing
configuration below and attaches a signed `appcast.xml`. The release is marked
as GitHub's latest release so the stable feed URL advances with the download.
This does not replace the real-Mac window and reading check in issue #18.
No experimental Linux artifact is attached: the current Linux build depends
on host GUI and software-rendering libraries, and portable packaging/runtime
instructions have not been validated. Linux retains its separate delivery gate.

## Configure updates (maintainers)

Update-enabled macOS builds use [Sparkle](https://sparkle-project.org/).
Kver → **Check for Updates…** checks the latest stable release; Sparkle offers
optional background checks and asks before installation/relaunch. It handles
update verification and quarantine during installation. The initial browser
download still has the unsigned-app security prompts described above.
Versions without Sparkle cannot acquire it automatically: readers must manually
download the first update-enabled release once.

### Signing-key setup

Generate a production signing key on a trusted Mac:

```sh
sdk=$(sh scripts/fetch-sparkle.sh)
"$sdk/bin/generate_keys" --account Kver
```

This stores the private key in your login Keychain and prints the public key.
Set the public base64 value as the repository Actions **variable**
`KVER_UPDATE_PUBLIC_KEY`. Set the matching private base64 value as the Actions
**secret** `SPARKLE_PRIVATE_KEY`, using GitHub's secure secret-entry UI. Keychain
Access can reveal the password of the Sparkle signing-key item for account
`Kver`. Do not paste the private value into chat, command arguments, repository
files, or logs. Back it up securely: unsigned-by-Apple apps cannot rely on
Developer ID trust to recover from a lost Sparkle key. Keep the same key across
releases unless following Sparkle's documented key-rotation procedure.

The release job refuses to build/publish without this configuration. It passes
the private key to `generate_appcast` over standard input, not a file or command
argument, then independently verifies the archive and feed against the public
key using macOS CryptoKit. Mismatched keys stop publication even though
`generate_appcast` itself only warns about them.
No Apple developer membership is required for Sparkle signatures;
they do not provide Apple notarization.

### Build an update-enabled app locally

Set only the **public** key when building:

```sh
export KVER_UPDATE_PUBLIC_KEY='your-base64-public-key'
sh scripts/package-macos-app.sh
```

The script fetches the pinned, checksum-verified Sparkle SDK into ignored
`target/sparkle/`, bundles its framework/helpers, and includes the public key and
feed URL in the sealed app. A normal build without the variable omits Sparkle
and disables **Check for Updates…**. Never distribute an archive built with
a throwaway test key as a production release.

The default public feed is:

```text
https://github.com/JonGretar/Kver/releases/latest/download/appcast.xml
```

Local builds can override it with `KVER_UPDATE_FEED_URL`, an HTTPS URL without
credentials. This does not change the production workflow's feed.
GitHub hosts both the feed and versioned ZIPs; neither requires authentication
now that the repository is public. The feed will return 404 until the first
update-enabled release is published. Keep prereleases and experimental
platform builds out of this stable release path.

### Check an upgrade before shipping

Use two disposable app bundles with increasing `CFBundleVersion` values and
the same public key. Sign their completed archives and the test feed with
Sparkle's `generate_appcast`, using a separate test key/feed rather than
publishing an unauthorized release.

Check that:

1. A real reader window opens a PDF while update checking/downloading runs.
2. **Check for Updates…** offers the newer version, while an equal version
   reports that the app is up to date.
3. Installing and relaunching replaces the intended app and launches the
   new version without another quarantine workaround.
4. A modified signed feed, corrupted archive, offline server, and cancelled
   update leave the installed version usable.
5. `codesign --verify --deep --strict` succeeds on the installed app.

Kver does not restore open windows after restarting. Reopen a PDF from Open
Recent; if Reopen at Last Position is enabled, it resumes where you left off.
Before shipping, also check the production HTTPS feed/download URLs and an
upgrade from the first update-enabled release. A local feed test cannot prove
GitHub publication or the behavior of another Mac.

## Open and check it

From the repository root, launch without a PDF path and choose a document
using **Open PDF** in the window:

```sh
open -n -a "$PWD/target/release/Kver.app"
```

If using `CARGO_TARGET_DIR`, substitute its release bundle path. Verify that
the file picker lets you choose `examples/readme-sample.pdf`, the sample renders,
the Kver icon appears in Finder/Dock, and Next/Previous Page changes pages
and `−`/`+` changes zoom. Choose another PDF with **Open PDF**; invalid and
unreadable documents should show an error and leave the window usable. For
path-based opening, pass an **absolute** path:

```sh
open -n -a "$PWD/target/release/Kver.app" --args "$PWD/examples/readme-sample.pdf"
```

`open` returning successfully
only means macOS accepted the launch request; check the visible window as well.

Double-clicking `Kver.app` in Finder opens the empty reader window; use
**Open PDF** to choose a document. The picker filters for PDF files.

### Open PDFs from Finder

Move the newly built `Kver.app` to Applications and launch it once. In Finder,
control-click a PDF and choose **Open With → Kver** (use **Other…** to select
the app if it is not listed). Each PDF opens in its own reader window; the
initial empty window is reused when available. This also works while Kver is
already running.

To make Kver the default for PDFs, select a PDF in Finder, choose **Get Info**
(Command-I), expand **Open with**, select **Kver**, and click **Change All…**.
Kver does not change your default automatically. Older downloaded releases
do not include this integration; rebuild the app to use it.

To exercise Finder-style file delivery from Terminal, without `--args`:

```sh
open -a "$PWD/target/release/Kver.app" "$PWD/examples/readme-sample.pdf"
```

Check both a fresh launch and delivery while the app is running, including
multiple PDFs and filenames containing spaces or non-ASCII characters.

To inspect the package independently:

```sh
plutil -lint target/release/Kver.app/Contents/Info.plist
file target/release/Kver.app/Contents/MacOS/kver
ls -l target/release/Kver.app/Contents/Resources/kver.icns
codesign --verify --deep --strict --verbose=2 target/release/Kver.app
```

The [tag-only release job](../.github/workflows/release.yml) builds the same
archive and checks its plist, executable architecture, and icon. Routine macOS
PR CI checks the non-updating local bundle. On a local Apple silicon Mac,
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
  globally. Ad-hoc signing protects bundle integrity but does not establish
  developer identity; Developer ID signing and notarization are not part of this build.
- The original v0.1.0 download has an incomplete bundle signature and can show
  “Kver is damaged.” It contains only the linker's executable signature, not
  a signature for the completed app. Rebuild with the corrected script rather
  than disabling macOS security checks. Successful `codesign` verification
  checks integrity, not Gatekeeper approval.
- If the icon does not update after rebuilding, inspect `Info.plist` and
  `Contents/Resources/kver.icns` above; Finder/Dock may cache an older icon.
