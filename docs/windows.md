# Experimental Windows x86-64 archive

This is an unsigned testing build, not a supported Windows release. CI artifacts
expire after 14 days and are not published to GitHub Releases. Build/package
verification does not establish clean-PC launch or native GUI usability.

## Requirements

- x86-64 Windows 10/11 is the initial candidate baseline, not certification of
  every Windows version or configuration. ARM64 is not a native target.
- A working Direct3D 11 graphics environment and current vendor graphics drivers.
  Remote sessions, virtual GPUs and unusual DPI configurations need native testing.
- Windows supplies the system DLLs and Universal CRT. `PE-DEPENDENCIES.json`
  records the actual normal and delay-load imports for this executable.
  If `vc_redist_imports` lists DLLs, install/update the **Microsoft Visual C++
  v14 x64 Redistributable** from
  <https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist>.
  Its supported OS requirements also apply. Do not download individual DLLs
  from DLL websites or copy DLLs out of a developer machine.

The archive contains no runtime DLL sidecars. GPUI's optional NVIDIA/AMD
driver-version probes are not a reason to redistribute vendor SDK DLLs.
Icons, reader assets and release shaders are embedded in the executable;
you do not need the source checkout or a particular working directory.
License notices and the dependency report travel with the executable.

## Extract and run

1. Obtain the experimental artifact from a trusted run of the project's
   **Experimental Windows archive** GitHub Actions workflow. Extract the
   GitHub artifact wrapper to find the versioned ZIP and its `.zip.sha256`.
2. In PowerShell, run `Get-FileHash .\Kver-vVERSION-experimental-windows-x86_64.zip
   -Algorithm SHA256` and compare the hash with the adjacent `.sha256` file.
   A matching hash detects corruption; an unsigned checksum from the same
   download does not authenticate the publisher.
3. Extract the **entire versioned ZIP** to a new user-writable folder. Do not
   run from inside the ZIP. Keep the included license material and guidance.
   `SHA256SUMS` lists SHA-256 hashes for the files inside this ZIP.
4. Open `kver.exe`, then use **Open PDF**, or invoke it by its full path:

   ```powershell
   & 'C:\Users\you\Apps\Kver\kver.exe' 'C:\Documents\book.pdf'
   & 'C:\Users\you\Apps\Kver\kver.exe' --new-window 'C:\Documents\other.pdf'
   & 'C:\Users\you\Apps\Kver\kver.exe' --version
   ```

The existing CLI defaults to reusing a reader; explicit options override the
application preference. Relative PDF paths use the caller's working directory.
See the included `CLI.md` for the command contract. Windows instance forwarding
requires separate native acceptance; the package check only executes `--version`.
No installer, default-reader change or file-association registration is performed.

Known platform limitation: safe overwrite of an existing PDF is currently
unavailable on Windows. Save to a new filename instead; this archive does not
change that safeguard.

## Unsigned downloads and policy

Windows SmartScreen, Defender, browser download protection or organizational
application-control policy may warn about or block this unsigned executable.
There is no promise of warning-free launch. Check the source, workflow revision
and checksum before deciding whether to run it. If an ordinary Windows dialog
offers a per-file approval, use it only if you trust this build and your policy
permits it. A managed policy may not offer an override; ask your administrator.
Do not disable Defender, SmartScreen or system security globally. Report the
exact warning and Windows version; do not send private documents.

## Update or remove

Quit all Kver windows/processes before changing versions. Extract the new ZIP
into a separate folder, verify it, and launch that copy. Update any personal
shortcut manually. There is no Windows self-updater. Keep the previous folder
if you need to return to that version; do not overwrite a running executable.

To remove the experimental app, quit it and delete its extracted folder and
your own shortcuts. Your PDFs are separate. Reading data and configuration live
in your Windows user profile, not next to the executable; removing the archive
folder does not reset them.

## Native acceptance is separate

The package job verifies AMD64 PE identity/version/icon, framework manifest,
imports, licenses, ZIP contents and checksums, and executes the extracted
`--version` from an unrelated directory on a Windows development runner.
That runner has development runtimes and is **not a clean PC**.

Before calling a build natively usable, record Windows version, installed
runtime, GPU/driver, scaling and security conditions separately in the issue:
clean non-development extraction/launch; first-run warnings; file picker and CLI
opens/instance forwarding; text, scanned, long and rotated/cropped PDFs; zoom
and scrolling; selection/copy; Find/Contents; keyboard/menu behavior; multiple
windows; persistence; DPI changes; saving and unsaved-work safeguards.
Unperformed checks remain **unverified**, not inferred passes.
