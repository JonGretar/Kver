# macOS packaging and releases

For installation and local build basics, see the [README](../README.md).
This guide is for maintainers. Scripts and the
[release workflow](../.github/workflows/release.yml) define the automated
checks; results belong in the release PR, not in this document.

## Build a candidate

On an Apple silicon Mac:

```sh
sh scripts/package-macos-app.sh
```

This builds an optimized app, verifies its version, architecture, resources,
and ad-hoc signature, then extracts and verifies the ZIP. It publishes
nothing. Outputs are under `${CARGO_TARGET_DIR:-target}/release/`.
Ordinary PR CI does not establish that a downloadable app works.

Without `KVER_UPDATE_PUBLIC_KEY`, local bundles omit Sparkle and disable
Check for Updates. For an update-enabled candidate, supply the production
**public** key:

```sh
export KVER_UPDATE_PUBLIC_KEY='your-base64-public-key'
sh scripts/package-macos-app.sh
```

The builder fetches the pinned, checksum-verified Sparkle SDK and bundles it.
Never distribute a candidate using a throwaway test key as a production app.
Sparkle signing is independent of Apple Developer ID signing/notarization.

## Update signing setup

On a trusted Mac, generate the production key:

```sh
sdk=$(sh scripts/fetch-sparkle.sh)
"$sdk/bin/generate_keys" --account Kver
```

Sparkle stores the private key in the login Keychain and prints the public
key. Configure these through GitHub's secure repository settings:

- Actions variable `KVER_UPDATE_PUBLIC_KEY`: public base64 key.
- Actions secret `SPARKLE_PRIVATE_KEY`: matching private base64 key.

Keychain Access can reveal the signing-key item's password for account
`Kver`. Never put the private value in chat, repository files, command
arguments, or logs. Back it up securely; unsigned-by-Apple apps cannot rely
on Developer ID trust to recover a lost Sparkle key. Keep the same key
between releases unless following Sparkle's documented rotation procedure.

The workflow passes the private key over standard input and independently
verifies the signed archive/feed against the public key.

The stable feed is
`https://github.com/JonGretar/Kver/releases/latest/download/appcast.xml`.
It can return 404 until the first update-enabled release is published.
Older apps without Sparkle require one manual download of an update-enabled
release. Keep prereleases and experimental platform artifacts out of this
stable path.

## Validate before publishing

Use a real Mac window, not merely a successful `open` command:

1. Extract the candidate and launch it without a PDF argument. Check its
   version, Finder/Dock icon, file picker, and invalid-PDF recovery.
2. Read ordinary text, image-only, long, and rotated/cropped PDFs. Exercise
   navigation, scrolling, zoom, fit modes, and resizing during background work.
3. Check Finder Open With on a fresh launch and while running, including
   multiple files and names containing spaces/non-ASCII characters.
4. Exercise annotation editing, Save/Save As, external changes, and
   Save/Discard/Cancel with shared-document windows. Verify saved PDFs in
   another reader; headless checks do not establish cross-reader fidelity.
5. Record the candidate revision, machine/macOS, documents, checks actually
   performed, and remaining limitations in the release PR.

For update changes, use two disposable bundles with increasing
`CFBundleVersion` values and the same separate test key. Generate a signed
test feed with Sparkle's `generate_appcast`; local builds accept
`KVER_UPDATE_FEED_URL` as a credential-free HTTPS override.
Check newer/equal versions, continued reading during downloads, installation
and relaunch, cancellation, offline failure, tampered feeds, and corrupted
archives. Failures must leave the installed app usable.
Run `codesign --verify --deep --strict` on the installed app.
Kver does not restore open windows after relaunch; use Open Recent.
A local test does not establish production HTTPS publication or another
Mac's behavior. Check the production feed and an upgrade from the first
update-enabled release before relying on that path.

## Publish only with release authorization

1. Set the package version in `Cargo.toml`; move changelog entries from
   `Unreleased` into exactly one `## [X.Y.Z] - YYYY-MM-DD` heading and leave
   a new `Unreleased` section.
2. Run `python3 scripts/release-metadata.py --tag vX.Y.Z` and the candidate
   packaging/native checks above. Have the release change reviewed and landed.
3. Only with explicit authorization for that release, create and push its
   `vX.Y.Z` tag. A tag push triggers publication; ordinary pushes and PRs do not.
4. Verify the GitHub Release, ZIP, signed `appcast.xml`, and production URLs.
   Download through a browser on an Apple silicon Mac and repeat the launch/
   reading smoke check. Record Gatekeeper prompts and the override used;
   a CLI download may not carry quarantine.

The tag-only workflow validates metadata and signing configuration, packages
the app, signs/verifies updates, and marks the release latest. Do not use a
production tag to test CI.

## Native quit checks after dependency upgrades

Required when changing GPUI Kit, its GPUI/macOS backend, `objc2`, or Kver's
[native quit adapter](../src/app/quit_guard/macos.rs). Headless tests alone
cannot establish native termination safety. Record results in the upgrade PR.

1. Inspect the pinned backend's delegate creation and termination/shutdown
   callbacks. The adapter must preserve delegate identity, ivars, inherited
   methods, and optional-selector discovery. Prefer a supported framework veto
   if one becomes available.
2. Run `corgi test --lib app::quit_guard`, the ordinary library tests, and
   strict Clippy checks on macOS, following the root command policy.
3. Make an unsaved highlight and choose Dock Quit. Cancel must leave edits,
   selection, and input usable. Repeating Quit while deciding must not stack
   dialogs.
4. Repeat with Save: a writable original saves without a filename prompt;
   a readonly original offers another location. Picker cancellation or write
   failure must preserve edits and cancel quitting. Verify the saved PDF;
   test Discard separately.
5. Quit with two windows of one dirty PDF and an unrelated document. Resolve
   each dirty session once, independent of the active window.
6. Recheck Command-Q, the Quit menu, last-window close, Finder file delivery,
   Dock reopen, menus, and update checking.

Force Quit, process killing, crashes, and power loss cannot prompt; unsaved
annotations have no persistent recovery.
