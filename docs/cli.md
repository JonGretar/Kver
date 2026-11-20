# Command-line usage

Run `kver some.pdf` to open a PDF. If Kver is already running, the command
hands the request to that process and exits. Otherwise it launches Kver.
Relative paths are resolved from your terminal's working directory, not
from the running app's directory.

## Make `kver` available on macOS

After installing `Kver.app` in `/Applications`, create a symlink in a
directory on your `PATH`. For example:

```sh
mkdir -p ~/.local/bin
ln -s /Applications/Kver.app/Contents/MacOS/kver ~/.local/bin/kver
```

Ensure `~/.local/bin` is on your shell's `PATH`. The symlink continues to
point to the installed app after updates. If you installed Kver elsewhere,
adjust the source path. This requires a version with CLI handoff support;
older running versions must be quit before using it.

## Choose a window

```sh
kver some.pdf                 # Existing window, unless configured otherwise
kver --new-window some.pdf    # Override the preference
kver --reuse-window some.pdf  # Use the most recently active reader
kver 'https://example.org/download?id=42'  # No .pdf suffix required
kver                         # Activate a reader, or open an empty window
kver --new-window            # Open an empty window
```

The application-wide **Open files in** preference defaults to **Existing
window**. CLI and Finder requests reuse the most recently active **document**
window, even when Settings or About is in front. Open PDF / Command-O and
Open Recent reuse the reader where you invoked the command. If no reader
is available, Kver creates one.

Choose **New window** to open files in separate readers from all of these
entry points. A default file request can fill the empty launch window.
Explicit CLI options override the preference.

Replacing a PDF follows the normal Save / Discard / Cancel safeguards.
The CLI exits when the request is queued, not when the PDF has loaded or
you have answered an unsaved-changes prompt. PDF loading errors appear in
the reader window; a successful CLI exit only confirms the handoff.

Change the preference in **Settings → Application → Windows → Open files in**,
or set it in `config.toml`:

```toml
[application]
open_files = "reuse" # Default; use "new_window" for separate readers
```

On macOS the file is at
`~/Library/Application Support/com.JonGretar.Kver/config.toml`.
The running app picks up valid edits automatically. New Window commands
always create a window. Navigation inside PDFs and links to other PDFs
keep their existing behavior.

## Paths and help

Quote paths containing spaces. Use `--` for filenames starting with `-`:

```sh
kver "My documents/book.pdf"
kver -- -book.pdf
kver --help
kver --version
```

Each invocation accepts at most one PDF path or HTTP(S) URL. Web PDFs open in
memory (up to 256 MiB) without a download file or reading-history entry.
Choose **Save** or **Save As** to keep a local PDF. Closing an unedited web PDF
discards it; if you annotated it, Kver asks whether to save your changes.

Help and version commands
do not launch or contact the app. Invalid arguments and handoff failures
return a nonzero exit status.
