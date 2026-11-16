# Experimental Windows x86-64 archive

This is an unsigned testing build, not a supported Windows release. Experimental
archives are published as GitHub Release assets. Build/package
verification does not establish clean-PC launch or native GUI usability.

## Requirements

- x86-64 Windows 10/11 is the initial candidate baseline, not certification of
  every Windows version or configuration. The inspected executable imports
  Windows' `icuuc.dll`, available starting with Windows 10 version 1703;
  earlier Windows versions cannot supply that requirement. See
  [Microsoft's ICU availability](https://learn.microsoft.com/en-us/windows/win32/intl/international-components-for-unicode--icu-).
  This is a dependency floor, not a tested minimum OS. Use a maintained Windows
  installation. ARM64 is not a native target.
- A working Direct3D 11 graphics environment and current vendor graphics drivers.
  Remote sessions, virtual GPUs and unusual DPI configurations need native testing.
- Windows supplies the system DLLs and Universal CRT. `PE-DEPENDENCIES.json`
  records the actual normal and delay-load imports for this executable.
  The inspected build imports `vcruntime140.dll`. Install/update the
  **Microsoft Visual C++ v14 x64 Redistributable** from
  <https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist>.
  Use a version at least as recent as the `msvc_build_tools_version` recorded
  in `PE-DEPENDENCIES.json`; Microsoft's latest x64 package is recommended.
  Future builds record their actual CRT imports in `vc_redist_imports`.
  The redistributable's supported OS requirements also apply. Do not download individual DLLs
  from DLL websites or copy DLLs out of a developer machine.

The archive contains no runtime DLL sidecars. GPUI's optional NVIDIA/AMD
driver-version probes are not a reason to redistribute vendor SDK DLLs.
Icons, reader assets and release shaders are embedded in the executable;
you do not need the source checkout or a particular working directory.
License notices and the dependency report travel with the executable.

## Extract and run

1. Open the project's [GitHub Releases](https://github.com/JonGretar/Kver/releases)
   and choose the version you intend to test. Download the experimental Windows
   ZIP and its `.zip.sha256` from **Assets**. No GitHub sign-in is needed.
   Experimental assets appear after the macOS release if their build succeeds.
   Keep the release URL to identify the version you tested.
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

The **Open files in** application preference defaults to reusing a reader;
explicit `--new-window` / `--reuse-window` options override it. Relative PDF
paths use the caller's working directory; each invocation accepts one PDF.
See the included `CLI.md` for the command contract. Windows instance forwarding
requires separate native acceptance; the package check only executes `--version`.
A successful CLI exit means the request was queued, not loaded or saved.
Answer any Save / Discard / Cancel prompt in the reader; Cancel must preserve
the original PDF and edits.
No installer, default-reader change or file-association registration is performed.

Known platform limitation: safe overwrite of an existing PDF is currently
unavailable on Windows. Save to a new filename instead; this archive does not
change that safeguard.

## Menu and keyboard

Open the title-bar **Kver** menu (a menu icon in narrow windows) for New Window,
Open/Recent, Save/Save As, Close Window, Settings, About and Quit.
**Settings → Keyboard** selects Standard or Vim and shows effective shortcuts;
menu and palette hints follow that selection and your overrides. Standard uses
Ctrl-O to open, Ctrl-F for Find and Ctrl-T for Contents. Vim uses `o`, `/`
and `g c` in the reader instead, and keeps Ctrl-O/Ctrl-I for back/forward
history and Ctrl-F for scrolling. Both use Ctrl-C to copy selected text and
Ctrl-Shift-P for the command palette. See the
[keyboard guide](https://github.com/JonGretar/Kver/blob/main/docs/keybindings.md)
for the full policy. There is no experimental Check for Updates menu command.

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

Save your work and quit all Kver windows/processes before changing versions;
otherwise the new executable can forward to the old running app. Extract the new ZIP
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

On a non-development PC, record each check as passed, failed or unverified:

- Clean extraction and launch from an unrelated directory; first-run security
  prompts and runtime availability without development tools installed.
- File picker and relative/absolute CLI opens, cold and with Kver running;
  default reuse, the New window preference and both CLI overrides. Check the
  actual destination window, not just the CLI exit status.
- Text, scanned, long and rotated/cropped PDFs; zoom/scroll; selection/copy
  into another app; Find and Contents when the PDF provides searchable text/outline.
- Standard/Vim keys and effective menu/palette hints; keyboard/pointer menu
  navigation, disabled Save, dismissal and return to reader/Find focus.
- Multiple document windows; close/reopen persistence; monitor/DPI changes.
- On disposable PDF copies, edit markup, save to a **new filename** and reopen
  it; check the known existing-file overwrite refusal leaves that file intact.
  Test Save / Discard / Cancel while replacing a dirty PDF, closing and quitting.
  Cancel must preserve the document and edits.

Record Windows version/build, installed x64 v14 runtime, GPU/driver, scaling
and security conditions separately in
[#183](https://github.com/JonGretar/Kver/issues/183). Unperformed checks remain
**unverified**, not inferred passes.

### Report a problem

[Open an issue](https://github.com/JonGretar/Kver/issues/new) with:

- `kver.exe --version`, workflow run URL/revision, ZIP filename and SHA-256;
  launch path and whether another version was running.
- Windows edition/version/build (`winver`), architecture, installed Microsoft
  x64 v14 runtime version, GPU/driver, monitor scaling and any VM or remote session.
  Say when a detail is unknown.
- Exact launch command or menu/keys, preset and relevant overrides, steps,
  expected/actual results and whether the problem repeats after restart.
- Exact security warning, missing-DLL or dialog error, terminal output and a
  screenshot if useful. Redact personal paths and private information.
- PDF type/page count and a public or minimal reproducer, if possible.
  Do not upload private PDFs, configuration or reading-history databases.
