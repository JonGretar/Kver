# Architecture

## Overview

Kver is a native Rust PDF reader built with GPUI and GPUI Kit. Hayro is the
initial PDF backend; a concrete rendering or text-geometry gap would justify
reconsidering it. The downloadable app targets Apple silicon macOS.
Linux and Windows have separate delivery gates.

The reader keeps PDF processing behind an internal adapter and schedules
expensive work off the UI thread. Navigation state and page positions do not
depend on the PDF engine or physical keybindings.

## Application boundaries

`src/main.rs` initializes GPUI Kit, keybindings, and the initial reader window.
`src/assets.rs` combines Kit's default component assets with only the extra
reader icons used by zoom and fit controls; the full icon catalog is not embedded.
`src/reader_windows.rs` creates empty reader windows, cycles focus, and owns
the last-window policy; it does not share document or viewport state between readers.
Each window mounts its reader inside GPUI Kit's `Root`, which owns component
overlays and applies the theme's interface scale. The window uses Kit's
`TitleBar` options so its native controls and dragging agree with the document
title. Kver follows the system's light/dark appearance for reader chrome
unless the app-wide preference selects Light or Dark; PDF pages retain their
document colors.
`src/reader.rs` owns the viewport, document identity, navigation state,
render scheduling, and window-local Actions. `src/pdf.rs` isolates Hayro:
it returns page dimensions, page geometry, positioned text, document outline
entries and destinations, and premultiplied BGRA pixels, not Hayro types, to
the reader. The adapter currently reads and
parses the document for each raster and text request. Reusing a parsed
document or prefetching pages should be driven by measurements, not a new
worker architecture upfront.
See the [rendering validation](rendering-validation.md) for the backend choice
and accepted limitations.

### Rendering and navigation

The reader loads page dimensions in background work. Continuous mode lays out
the document with spacers around visible pages; Single Page mode lays out only
the selected page and bounds the scroll region to that page. Each reader window
owns its mode; this slice does not persist a mode or set a configurable default.
Wheel and trackpad input never turn pages in Single Page mode; page turns are
explicit Actions. Page-local PDF-point positions anchor mode switches, zoom,
and jumps, clamped to the destination page's viewport. The reader selects one
raster job at a time from the current viewport rather than queueing work that
a scroll or zoom might make obsolete. Old-document, old-scale, and offscreen
results are discarded. Only visible bitmaps are retained, within a 128 MiB
budget; offscreen pages remain placeholders. Rasterization accounts for
display density and passes pixels directly to GPUI without a PNG round trip.

The reader preserves a page-local position in PDF points across zoom and
viewport changes. Fixed zoom, fit width, and fit page use one scale for
continuous layout. Fit modes recompute from the current page and measured
scroll viewport on navigation or resize; manual zoom returns to fixed mode.
Scale changes invalidate obsolete bitmaps before new background renders
complete, without waiting on the UI thread.

### Settings and platform storage

`src/settings.rs` owns the stable `com` / `JonGretar` / `Kver` identity
and resolves config and local-data directories with `directories::ProjectDirs`.
Configuration lives at `config_dir()/config.toml`; reading history lives in
`data_local_dir()/reading.sqlite3`. Neither location depends on a PDF path or
the working directory. Appearance is `theme_mode` in `[appearance]` (default
System); `quit_when_last_window_closes` in `[windows]` defaults to true. A missing
file or table uses those defaults without writing a file until a choice is saved.

`src/settings.rs` validates the entire known `AppSettings` value before applying
external edits or panel changes; a `SettingsChange` updates one known field in
the latest valid TOML document. New preferences explicitly join this model
and the panel, rather than generating controls from arbitrary TOML keys.
`src/settings_ui.rs` owns the shared app-wide settings state and one Settings
window. Its General page contains a labeled appearance dropdown, a reset
control when appearance differs from System, and the last-window switch.
Loading, periodic file checks, and saving TOML run off the UI thread. Valid
external changes update the shared model, theme, and open Settings controls;
invalid or partially written edits leave the last valid choice active and
show an error until correction. Panel choices commit only after the writer
re-reads a valid file, and saves are serialized through a temporary file in
the config directory. The writer retains unrelated TOML entries and comments
and refuses a file that changed while the replacement was prepared. A panel
choice cannot replace malformed config. Unavailable paths and read/write
errors leave the window usable and appear beside the setting.

`src/recent.rs` owns local PDF identity and the SQLite schema. A successful
page load records the canonical local path; failed and cancelled opens do not.
The `documents` table is the parent for future per-PDF records: child tables
must reference its path with `ON DELETE CASCADE`, so removing a document
removes its saved reading data, not the PDF itself. The schema uses
`user_version` migrations and refuses newer versions rather than overwriting
them. A shared background worker processes SQLite requests in the order reader
windows enqueue them, so forgetting a PDF cannot be undone by an earlier,
still-pending recording; neither operation blocks reader input. Open Recent queries
the database on every search, including records beyond the first 100 shown.
It excludes the active reader's normalized PDF path before limiting results,
without removing that record for other windows or future opens. It does not
scan for missing PDFs on startup. Each recent row has a separate, accessible
Forget button; it confirms which PDF's saved data will be removed before
deleting the database record. The PDF file remains untouched. The recent-file
`Command` lives inside GPUI Kit's `Dialog`, so its modal backdrop, Escape
behavior, pointer hit testing, and focus restoration come from the window
overlay layer rather than a second hand-built reader overlay.

### Actions, palette, and menu

Open PDF, page turns, reading mode, zoom, and fit modes are GPUI Actions
handled by the active reader window. Toolbar controls, keybindings, and GPUI
Kit's command palette dispatch those same Actions; reader methods own the underlying
navigation and loading behavior. The reader owns the file picker and ignores
obsolete asynchronous load results, so entry points share error handling.
The native picker uses `rfd` for its PDF extension filter because GPUI's
`PathPromptOptions` does not expose file-type filtering. Its asynchronous API
keeps the picker from blocking GPUI's event loop; cancellation leaves the
current document unchanged. Unlike GPUI's picker, `rfd` does not distinguish
picker failure from cancellation.
The app shell registers OS file-open events before launch and queues local
file paths through `async-channel` for processing on GPUI's foreground task.
`url` decodes file URLs without corrupting escaped or non-ASCII filenames.
File-open requests reuse the initial reader while empty, otherwise create
independent readers. The macOS bundle declares the standard `com.adobe.pdf`
type as a viewer with alternate handler rank, without taking over the user's
default PDF application. Linux and Windows file associations remain separate
packaging work.
Document-dependent Actions are unavailable in an empty, loading, or error
window; each reader checks its own document state.
Kit's `TitleBar` holds the truncating filename and persistent Open PDF button;
the grouped `Toolbar` holds navigation, current reading mode, zoom, fit, and
selection commands.
`StatusBar` shows page position and zoom without taking space from the PDF.
`Empty` shows the open, loading, or recoverable error state in the document
area. Control presentation uses Kit's semantic theme colors and component
states rather than a reader-specific palette.

The command palette is an inline overlay owned by the reader. This keeps the reader
in its Action dispatch path rather than relying on a global handler that
could affect another window. Kit's `Command` owns search and selection;
`Root` handles the overlay's focus trap through Kit's `FocusTrapElement`.
The overlay has a dialog role and a rem-based maximum width so it remains
usable in narrow windows. Dismissing it restores the active sidebar panel's
focus when one is open, otherwise reader focus. GPUI Kit
resolves shortcut hints from active keybindings. Bare reading keys apply only
while the reader has focus; text entry in the palette does not navigate the
PDF.

The reader owns one `ReaderSidebar` per window in `src/reader_sidebar.rs`. It
selects one panel, reserves a bounded side of the window's reading area
(toolbar, document, and status bar), and transfers focus on open, switch, and
close. The toolbar's leading control, View menu, and command palette dispatch
the same `ToggleSidebar` Action, reopening the last-selected panel. Bottom,
equal-width `TabBar` tabs switch panels; the panel content owns the scroll
region above them. The macOS title bar stays across the full window to
preserve its native controls. `src/find_panel.rs` owns the Find query;
`src/contents_panel.rs` owns Contents focus and GPUI Kit's virtualized
outline tree. Command-T dispatches `ShowContents` from the active reader window
even when Find has focus; Command-F returns to Find. Kver's documents remain in
independent windows, not document tabs. The tree preserves PDF hierarchy,
supports collapse/expand with the arrow keys, and navigates when a usable row
is clicked or confirmed. Only the chevron toggles disclosure by pointer;
clicking the rest of a parent row navigates without folding it. Both panels
supply content to the same host; search and outline state remain separate.
The sidebar remains attached to the reader
across document changes, while its outline state resets immediately and
background extraction results carry the reader's document generation. The
Find panel is still an interim surface, not a search result.

Hayro 0.8 exposes the PDF catalog through `xref.root_id()` and `xref.get`,
and individual page dictionaries through `page.raw()`. The PDF adapter
traverses the catalog's linked outline hierarchy and resolves direct,
legacy-named, and name-tree destinations using those lower-level objects.
It bounds malformed cycles and depth, retains non-navigable headings,
and converts supported `/Fit`, `/FitB`, `/FitH`, `/FitBH`, and `/XYZ`
destinations into page-local positions using the target page's crop and
rotation transform. Unsupported destination modes cannot navigate; headings
with children remain expandable. Outline parsing starts on a background worker
only when Contents first opens for a document. The reader applies destinations
through its existing page/zoom/scroll model.

On macOS, `src/app_menu.rs` installs native Kver, File, Edit, Window, and View menus
at startup.
File → Open PDF dispatches the reader's `OpenPdf` Action when a reader exists;
with no windows the global fallback first opens a reader. The app-wide
`NewWindow` Action opens a reader even with no focused window, while Window →
Next Window and Previous Window cycle focus. The app-wide `Quit` Action exits
regardless of the last-window choice. Closing a non-last window never quits.
On macOS, with the preference off, the app stays available via its menu and
GPUI's Dock reopen callback; a Finder file-open request opens a reader if
the launch window has closed. On Linux and Windows the last close always quits
until an equivalent windowless activation path exists, preventing an inaccessible
process. Menu setup stays separate from reader operations.

### macOS updates

`src/updater.rs` retains one Sparkle 2 controller through the application-wide
`CheckForUpdates` Action. It loads only `Contents/Frameworks/Sparkle.framework`
from a configured app bundle, using `objc2` and Foundation at this boundary.
Ordinary unbundled executables and local bundles built without a public key
do not load Sparkle or check the network; their update menu item is disabled.
The reader and PDF adapter have no updater dependency.

Sparkle owns optional background-check consent and scheduling, download
progress, verification, installation, and relaunch prompts. Kver requires
signed feeds and Ed25519 archive verification before extraction, and disables
automatic installation: the reader approves installing an update. Sparkle's
preferences are its source of truth, not a second Kver settings store.
The framework is version- and checksum-pinned during packaging, rather than
downloaded at application runtime or required to compile the Rust binary.

The tag-only release workflow signs the ZIP and feed with a GitHub Actions
secret and attaches both to the same GitHub Release. The default feed is
`https://github.com/JonGretar/Kver/releases/latest/download/appcast.xml`;
each enclosure points to its version-specific release archive. Releases are
stable-only, full Apple silicon archives, without delta updates or a custom
update server. Sparkle signing is independent of Apple Developer ID signing
and notarization; the initial unsigned download still requires macOS approval.
See [macOS packaging](macos-app.md#configure-updates-maintainers) for key setup
and the first-release bootstrap requirement.

## Document coordinates

Reader navigation anchors use page-local PDF points, not screen pixels.
`src/coordinates.rs` provides transforms between PDF document coordinates,
cropped/rotated page-local coordinates, and the viewport; the rendered bitmap
is only a display representation. The PDF adapter collects positioned glyphs
on a background worker for visible pages. The reader keeps one-line selection
anchors in document coordinates and projects its transient highlight through
the page transform at the current zoom, independently of the raster cache.
Changing documents invalidates pending text work and clears selection. More
general line grouping, rotated/cropped text selection, and scans are separate
follow-ups.

## Future state

Reading history and appearance settings persist. Positions, marks, and
per-document view overrides are separate future work; the SQLite document
record is their shared deletion boundary. Platform-specific menus, dialogs,
and packaging belong at the application boundary; PDF and navigation behavior
should remain portable.
