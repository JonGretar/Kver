# Releasing Kver

For installation, see the [README](../README.md#install); for local build
basics, see the [development guide](development.md).
This is the maintainer runbook and the procedure behind `/release`.
[Release automation](../.github/workflows/release.yml) and the scripts under
[`packaging/`](../packaging/) define how artifacts are built and published.
Record results in the release PR, not in this document.

## Choose the version and scope

Check the latest published stable [release](https://github.com/JonGretar/Kver/releases)
and its tag, then review `Unreleased` and the changes since that tag. If no
version was supplied, recommend one and explain why. In the current `0.x`
series, propose a patch for fixes and a minor bump for substantial new
reader-facing features or compatibility changes; discuss deliberate breaking
changes explicitly. Check that the proposed tag is unused. Wait for version
confirmation before preparing it and explicit publication approval before
landing the release change or pushing a tag. A request to publish a specific
version covers the release change, publication, and the Arch recipe update.

## Prepare the release change

1. Start from the current default branch. Set the Kver version in `Cargo.toml`
   and `Cargo.lock`. Consolidate `Unreleased` into one dated
   `## [X.Y.Z] - YYYY-MM-DD` chapter and leave an empty `Unreleased` section.
   Describe the final reader-facing behavior, not intermediate changes to new
   features. Call out known limitations and keep past chapters intact.
2. Run `python3 packaging/common/release-metadata.py --tag vX.Y.Z`. Review the
   same command with `--notes`; the workflow uses that output for GitHub.
3. If dependencies or packaging changed, check the affected target's notice
   generation before tagging. Use its checked-in policy and locked dependency
   graph. A successful compile does not prove the archive can be packaged.
   Keep notice generation mandatory; review any new license before accepting
   it and verify its required notices are included.
4. Build the Apple silicon candidate and run the native checks below. Record
   what passed and what could not be checked on the exact candidate.

## Build a candidate

On an Apple silicon Mac:

```sh
sh packaging/macos/package.sh
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
sh packaging/macos/package.sh
```

The builder fetches the pinned, checksum-verified Sparkle SDK and bundles it.
Never distribute a candidate using a throwaway test key as a production app.
Sparkle signing is independent of Apple Developer ID signing/notarization.

## Update signing setup

On a trusted Mac, generate the production key:

```sh
sdk=$(sh packaging/macos/fetch-sparkle.sh)
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
release. Keep prereleases and non-macOS artifacts out of this
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

## Land and publish

1. With publication authorized, review and land the release change using the
   [normal landing gates](agents/workflow.md#feature-workflow), including
   the currently applicable CI checks. If the candidate changes, repeat the
   checks affected by the change.
2. Fetch the publication remote and verify that the landed release commit
   matches the tested candidate tree. Validate the version again there and
   check that the tag does not exist remotely. Create the tag at that exact
   commit, not at whatever `main` points to later. After verifying that `origin`
   is the publication remote, substitute the version and commit below:

   ```sh
   jj tag set vX.Y.Z -r VERIFIED_RELEASE_COMMIT
   jj git push --remote origin --tag vX.Y.Z --dry-run
   jj git push --remote origin --tag vX.Y.Z
   ```

   A `vX.Y.Z` tag push publishes; ordinary pushes and PRs do not. Do not use
   a production tag to test CI or move it to recover a failure.
3. Confirm that the release workflow is running for the expected tag and
   commit. The Linux build runs alongside macOS. After the macOS job publishes
   the release and signed Sparkle feed, Linux can attach its verified archive
   and checksum, and the Windows job starts.

## Verify publication and update Arch

| Deliverable | Definition and release check |
| --- | --- |
| Apple silicon macOS ZIP and Sparkle feed | [Release workflow](../.github/workflows/release.yml). Confirm the intended release is latest and its notes match the dated chapter. Download the ZIP and `appcast.xml`, verify both with `packaging/macos/verify-update-signatures.swift` and the production public key, and check the public latest feed points to that ZIP. Repeat the launch and reading smoke check with a browser download on Apple silicon; record actual Gatekeeper behavior separately from CLI-download checks. |
| Linux x86-64 archive | [Linux archive workflow](../.github/workflows/linux-archive.yml) and [Linux guide](linux.md). Check the published archive and checksum, including the contents and notices from clean extraction. |
| Windows x86-64 archive | [Windows archive workflow](../.github/workflows/windows-archive.yml) and [Windows guide](windows.md). Check the published archive and checksum, including the contents and notices from clean extraction. |
| Arch Linux x86-64 recipe | [Arch bump instructions](../packaging/arch/README.md#bump-to-a-new-release). After tagging, update `PKGBUILD` and `.SRCINFO`, verifying the source checksum against the tagged tarball. Land the recipe update using the normal gates. Record whether `makepkg` and the installed version were actually checked. This step is part of the release; it is not a GitHub release asset or a prerequisite for macOS publication. |

Report each deliverable separately, with links to checks and any unverified
steps. Linux x86-64 is a regular download with an Ubuntu 24.04 / glibc 2.39
baseline; Windows remains experimental. State the platform and runtime baseline
and distinguish packaging checks from native desktop checks. Report an actual
update install separately from signature and feed verification.

If archive publication fails, leave working assets and the tag alone. Rerun
transient failures. After landing a license-policy fix, manually run the
affected archive workflow from `main` with `release_tag` set to the existing
tag; it rebuilds the tagged source using the current notice policy. Download
and verify recovered assets before reporting success. Non-macOS assets
never enter the Sparkle feed. If recovery or the Arch update is blocked,
report what is already live and what remains rather than calling the release
complete.

## Add a distribution format

Keep version metadata and release notes shared; keep format-specific build,
verification, and recovery in the platform packaging code. For a format
attached to GitHub Releases, its build can run independently, but attaching
assets must wait for publication. Add one row to the deliverables above and
define the target architecture, runtime baseline, license notices, checksum,
clean-install or extraction check,
and how to recover from a partial upload. For example, a `.deb` would need an
install/remove check on the intended distro; a script shipped in an existing
archive belongs in that archive's verification, including preservation of user
data on uninstall. A distro recipe can stay a post-tag step like Arch. Add the
new format to the relevant install guide, not to the skill.

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
