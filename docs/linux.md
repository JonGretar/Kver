# Linux x86-64 archive

The Linux release targets x86-64 with an Ubuntu 24.04 / glibc 2.39 baseline.
Other distributions and desktop configurations may work but are not all
verified. No Rust development environment is needed. Updates are manual.

## Download or install

For a user-level install of the latest release's Linux
archive, use the [download installer](../README.md#linux-x86-64). It needs
`curl`, Python 3.9+, `sha256sum`, and `tar`. If that release's Linux archive
or checksum is not available yet, it stops rather than installing an older
release. Quit Kver before running the installer to update an existing copy.
See [removal instructions](#install-a-desktop-launcher-optional) below.
Arch users can use the [separate package recipe](../packaging/arch/README.md).

To run from a portable copy or choose a specific version, open
[GitHub Releases](https://github.com/JonGretar/Kver/releases) and choose
the version you want. Download its Linux tarball and
matching checksum from **Assets**. These are public downloads; no GitHub
sign-in is needed. Linux assets appear after the macOS release if
their build succeeds.

Keep the release URL. `BUILD-INFO.json` inside the tarball records its source commit.
Package/loader checks do not establish native desktop usability. See
[#183](https://github.com/JonGretar/Kver/issues/183) for per-OS evidence and gaps.

## Extract and run

Download the versioned `Kver-vX.Y.Z-linux-x86_64.tar.gz` and
matching `.tar.gz.sha256` from the same release. In the download directory,
substitute its version. For older releases, use the exact filename from
GitHub, which includes `-experimental-`:

```sh
sha256sum -c Kver-vX.Y.Z-linux-x86_64.tar.gz.sha256
tar -xzf Kver-vX.Y.Z-linux-x86_64.tar.gz
cd Kver-vX.Y.Z-linux-x86_64
sha256sum -c SHA256SUMS
./kver --version
./kver /absolute/path/to/document.pdf
```

The checksum detects corruption, not publisher authenticity. Use only a
trusted build. See `CLI.md` beside the executable for the opening/forwarding
options. Launching by absolute executable path works from any directory.
Reader icons, app artwork and both keybinding presets are embedded.
`resources/kver.png` is supplied for desktop integration, not loaded
relative to the working directory. Nothing registers a default PDF reader.

## Read and open more files

Use **Open PDF** or the title-bar **Kver** menu (a menu icon in narrow windows).
The menu includes New Window, Open Recent, Save/Save As, Settings, About and
Quit. **Settings → Keyboard** selects Standard or Vim and shows effective
shortcuts; menu and palette hints follow that selection and your overrides.
Standard uses Ctrl-O to open, Ctrl-F for Find and Ctrl-T for Contents.
Vim uses `o`, `/` and `g c` in the reader instead, and keeps Ctrl-O/Ctrl-I
for back/forward history and Ctrl-F for scrolling. Both use Ctrl-C to copy
selected text and Ctrl-Shift-P for the command palette.
The [keyboard guide](https://github.com/JonGretar/Kver/blob/main/docs/keybindings.md)
has the full policy.

The **Open files in** application preference defaults to an existing reader.
For explicit choices, use `./kver --new-window /path/to/other.pdf` or
`./kver --reuse-window /path/to/other.pdf`. Each invocation accepts one PDF;
quote paths containing spaces. Opens forward to the running per-user app,
including opens from the installed launcher. A successful CLI exit means
queued, not loaded or saved: answer any Save / Discard / Cancel prompt in
the reader. Cancel must leave the original PDF and edits intact.

## Runtime requirements and limits

The build baseline is **Ubuntu 24.04, x86-64 GNU/Linux**, not musl/Alpine.
`BUILD-INFO.json` identifies the version, source revision, compiler and build
glibc. `ELF-DEPENDENCIES.json` records actual ELF imports, required symbol
versions, resolved linkage and dynamic-loader probes. Consult that report
before attempting another distribution; building on Ubuntu does not establish
an older glibc baseline. The initial archive requires glibc 2.39; older
distributions are not promised to work.

Use a graphical desktop with X11 or Wayland, system fonts/fontconfig configuration,
xkbcommon (including X11 support), and the linked libraries listed in the
report. The package does not bundle system libraries or graphics drivers.
On Ubuntu, the packaging environment supplies `libfontconfig1`,
`libxkbcommon0`, `libxkbcommon-x11-0`, `libwayland-client0`, `libvulkan1`,
`libegl1` and their package dependencies. These are not a certification of a
minimal runtime installation.

GPUI's WGPU backend dynamically loads Vulkan and can use an EGL/GLES fallback.
A working GPU driver and Vulkan ICD or suitable EGL implementation are needed;
the CI Mesa software-rendering libraries are not proof of desktop performance
or GPU support. Wayland also loads its client library dynamically. Loader
probes fail packaging if these libraries are missing, but do not test a
compositor, graphics context or driver-specific transitive libraries.

File dialogs require a session D-Bus and `xdg-desktop-portal` with the backend
appropriate to your desktop. Portal service availability, font coverage,
driver-specific dependencies and native desktop compatibility remain
unverified by packaging. A successful `--version` does not initialize a GUI.

## Install a desktop launcher (optional)

With **Python 3.9 or newer**, run from the extracted directory, without `sudo`:

```sh
python3 install.py install
```

This copies the package to `~/.local/opt/kver` and installs `kver.desktop` and
the Kver icon under `${XDG_DATA_HOME:-$HOME/.local/share}`. Open **Kver** from
your desktop's application menu (a desktop refresh or re-login may be needed).
The launcher accepts one PDF per invocation and delegates opening and window
preferences to the existing CLI. It does not change your default PDF reader,
edit `mimeapps.list`, or implement another IPC service.

The installer also links `~/.local/bin/kver` to the installed binary. If
`~/.local/bin` is on your `PATH`, use `kver some.pdf` from a terminal. Otherwise,
the installer prints a PATH hint without editing your shell configuration.
An existing unrelated command is left alone; you can still invoke
`"$HOME/.local/opt/kver/kver"` directly. See `CLI.md` for options. Python is not
needed to run the installed app.

To update or return to an older build, quit Kver, extract that archive, check
its checksums, and run its `python3 install.py install` again. Keep the original
archive if you want to return to it. Do not run installers concurrently.

To remove the installed copy and desktop integration:

```sh
python3 "$HOME/.local/opt/kver/install.py" uninstall
```

You can also run `python3 install.py uninstall` from an extracted archive if
the installed script is missing. Use the same `HOME` and `XDG_DATA_HOME` as
installation. Missing owned files are tolerated; reinstall repairs them.
Documents, configuration, reading history and unrelated files are preserved.
Only inventory-owned files with recognized contents are replaced or removed.
The CLI symlink is removed only if it still points to this installation's binary.
If a file was modified, move it aside and retry; if the ownership manifest is
missing, the script refuses to adopt or delete existing contents. Symlinked
destination files or parent directories are rejected rather than followed.
Unknown files inside the installation directory remain there after removal.
Permission/storage errors return a nonzero status; fix the reported problem
and retry. Never delete your data directories to fix an installation.

Removal does not delete downloaded/extracted copies. Those remain portable,
independent copies that you can remove separately.

## Update a portable copy

Save your work and quit **all** Kver windows/processes before switching builds;
otherwise the new executable can forward to the old running app. Extract and
verify the new tarball in a separate directory, then run that copy and check
its version. Keep the previous archive if you want to return to it. For an
installed copy, use the replacement procedure above. Neither procedure updates
automatically or resets configuration and reading history.

## Native testing

On a non-development desktop, record each check as passed, failed or unverified:

- Clean extraction and launch from an unrelated directory; first-run security
  prompts; installed desktop launcher and window grouping, if installed.
- File picker, relative/absolute CLI paths and launcher opens, both cold and
  with Kver running; default reuse, the New window preference and both CLI
  overrides. Check the actual destination window, not just the CLI exit status.
- Text, scanned, long and rotated/cropped PDFs; zoom/scroll; selection/copy
  into another app; Find and Contents when the PDF provides searchable text/outline.
- Standard/Vim keys and effective menu/palette hints; menu keyboard/pointer
  navigation, disabled Save, dismissal and return to reader/Find focus.
- Multiple document windows; close/reopen persistence; monitor/scaling changes.
- On disposable PDF copies, edit markup and Save/Save As, reopen the result,
  then test Save / Discard / Cancel while replacing a dirty PDF, closing and
  quitting. Cancel must preserve the document and edits.

Record X11 and Wayland results separately, including the desktop/compositor,
portal backend and GPU driver. CI, software rendering and loader success
are not native acceptance. Leave unavailable combinations unverified.

### Report a problem

[Open an issue](https://github.com/JonGretar/Kver/issues/new) with:

- `kver --version`, workflow run URL/revision, archive filename and SHA-256;
  extracted or installed copy, and whether another version was running.
- Distribution/version, architecture and glibc (`ldd --version`); X11/Wayland,
  desktop/compositor, portal/backend, GPU/driver, monitor scaling and any VM
  or remote session. Say when a detail is unknown.
- Exact launch command or menu/keys, preset and relevant overrides, steps,
  expected/actual results and whether the problem repeats after restart.
- Terminal output, exact security/dialog errors and a screenshot if useful.
  Redact personal paths and private information.
- PDF type/page count and a public or minimal reproducer, if possible.
  Do not upload private PDFs, configuration or reading-history databases.
