# Experimental Linux x86-64 archive

This is a testing artifact, not a supported Linux release or a claim of
compatibility with every distribution. No Rust development environment is
needed to use it. Updates are manual; no updater is included.

## Extract and run

Download the versioned `Kver-vX.Y.Z-experimental-linux-x86_64.tar.gz` and
matching `.tar.gz.sha256` from the same CI artifact. In the download directory,
substitute its version:

```sh
sha256sum -c Kver-vX.Y.Z-experimental-linux-x86_64.tar.gz.sha256
tar -xzf Kver-vX.Y.Z-experimental-linux-x86_64.tar.gz
cd Kver-vX.Y.Z-experimental-linux-x86_64
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
edit `mimeapps.list`, add a shell command to `PATH`, or implement another IPC
service. From a terminal, use `"$HOME/.local/opt/kver/kver"` with the options
in `CLI.md`. Python is not needed to run the installed app.

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
If a file was modified, move it aside and retry; if the ownership manifest is
missing, the script refuses to adopt or delete existing contents. Symlinked
destination files or parent directories are rejected rather than followed.
Unknown files inside the installation directory remain there after removal.
Permission/storage errors return a nonzero status; fix the reported problem
and retry. Never delete your data directories to fix an installation.

Removal does not delete downloaded/extracted copies. Those remain portable,
independent copies that you can remove separately.

## Native testing

Report the archive checksum, version, distribution, X11/Wayland session,
desktop/portal backend and GPU/driver when testing. Archive verification is
separate from native acceptance. On a non-development desktop, check launch,
file picker and CLI opens/forwarding; text, scanned, long and rotated/cropped
PDFs; zoom/scrolling; selection/copy; Find/Contents; keyboard/menu interaction;
multiple windows; persistence; DPI changes; and save/unsaved-work safeguards.
Treat any unavailable checks as unverified, not inferred passes.
