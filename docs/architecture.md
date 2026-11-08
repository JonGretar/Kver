# Architecture

## Overview

Kver is a native Rust PDF reader built with GPUI and GPUI Kit. Hayro is the
initial PDF backend; a concrete rendering or text-geometry gap would justify
reconsidering it. The downloadable app targets Apple silicon macOS.
Linux and Windows have separate delivery gates.

The reader keeps PDF processing behind an internal adapter and schedules
expensive work off the UI thread. Navigation state and page positions do not
depend on the PDF engine or physical keybindings.

See the [reader ownership decision](adr/0001-capability-owned-reader.md) for the
module trade-offs and the [glossary](../GLOSSARY.md) for reading-domain terms.

## Application boundaries

`src/main.rs` parses the command line and calls `app::run`. `src/app.rs`
registers OS open/reopen entry points before launch, initializes GPUI Kit and
shared state, and creates the initial reader window. Settings and history
receive the same resolved application paths.
`src/app/keymap.rs` resolves embedded common, platform-filtered, and preset
entries in that order before installing Actions and building native menus.
Standard is selected by default; until the separate preset tickets define new
keys, the embedded Standard and Vim entries preserve the existing reader keys.
The policy addresses a slot by product context and complete key sequence. A
later layer replaces that slot; an explicit contextual unbind masks the same
key inherited from its parent (including a global key) only in that context.
Other contexts retain the inherited binding.
Duplicate slots within a layer and one-key/multi-key prefix collisions in
overlapping contexts are diagnostics, not implicit GPUI ordering rules.
Reader, Sidebar, Contents, Settings, and About are stable product contexts;
the sidebar contains Contents, while Reader and the auxiliary windows do not
overlap. `src/key_context.rs` shares their GPUI names with the focused views.
Features still own their Actions and handlers. GPUI owns dispatch, pending
sequences, Escape cancellation, and focus invalidation. User file loading and
keymap reloads are not implemented. Line scrolling uses the reader's
`ScrollDown`/`ScrollUp` Actions; Down/J and Up/K are bound only while Reader
owns focus, rather than being interpreted by the view.
`src/assets.rs` combines Kit's default component assets with only the extra
reader icons used by zoom and fit controls; the full icon catalog is not embedded.
`src/app/windows.rs` creates document windows, cycles focus, and owns
the last-window policy; it does not share document or viewport state between readers.
Each window mounts `app::document_window::DocumentWindow` inside GPUI Kit's
`Root`, which owns component
overlays and applies the theme's interface scale. The window uses Kit's
`TitleBar` options so its native controls and dragging agree with the document
title. Kver follows the system's light/dark appearance for reader chrome
unless the app-wide preference selects Light or Dark; PDF pages retain their
document colors. `DocumentWindow` retains the system-appearance subscription
and owns the native file picker, Open Recent coordination, and
Open/Close/Next Window/Previous Window handlers. Those handlers are ancestors
of the reader's focused regions, so sidebar and palette input still reach the
correct window.

The window strongly owns one reader entity, with no reverse reference. The
reader remains the source of document identity, loading, and restoration;
window and OS open requests call that same load path. A file-picker result
checks the document generation rather than the raster generation, so a
replacement document invalidates it but an intervening zoom does not.
The native picker task belongs to the window lifetime. Deferred reader resize
work uses a weak handle so a queued frame does not retain a closed reader.

`DocumentWindow` composes the title bar, sidebar, reading toolbar, central
PDF surface, status bar, and full-window inline overlay layer.
`document_window/chrome.rs` renders value-like controls from the reader's
public state; it retains no separate document identity or viewport.
The window observes its reader and shared settings so chrome and command
availability follow changes. Sidebar resizing and chrome visibility changes
request a reader refit after the new layout has been measured.
The palette belongs to the window. Marks remain document-owned, but the
window mounts their popup alongside the palette so neither is positioned
relative to the smaller workspace below the title bar.

`src/reader.rs` implements `ReaderView`: the central PDF surface, document
identity and loading/restoration, navigation, selection, rendering, and
document-owned marks coordination. It does not retain the sidebar or its
panels, window chrome, the palette, or Recent dialogs. The former public
`Preview` name remains an alias for callers adopting the narrower interface.
`src/reader/commands.rs` declares document commands independently of physical
bindings. `src/reader/selection.rs` owns text selection and its page-text cache
as ordinary Rust state, without a separate entity or view back-reference.
`src/reader/links.rs` retains visible-page link metadata and one current
background request; replacing a document invalidates that request before its
reply can affect the new document.
`src/reader/navigation.rs` owns shared layout and navigation-coordinate rules.
`src/reader/rendering.rs` owns raster request selection, reply validation, and
visible bitmap retention. The reader coordinates navigation commands,
restoration, background execution, and presentation.
`src/reader/tests.rs` contains only the integration-suite declarations and
shared fixtures. The suites under `reader/tests/` group loading, reading modes,
saved positions, navigation, rendering, selection, Find, and marks behavior.
They can inspect private reader invariants without widening production
visibility; workflows that need chrome mount the real `DocumentWindow`.
Window-only composition, command routing, and focus tests remain beside
`DocumentWindow`.
`src/pdf.rs` isolates Hayro:
it returns page dimensions, page geometry, positioned text, document outline
entries and destinations, page-local link rectangles with backend-neutral
destinations, and premultiplied BGRA pixels, not Hayro types, to
the reader. The adapter currently reads and parses the document for each
raster, text, and visible-page link request. Reusing a parsed
document or prefetching pages should be driven by measurements, not a new
worker architecture upfront.
See the [rendering validation](rendering-validation.md) for the backend choice
and accepted limitations.

### Rendering and navigation

`PageLayout` borrows the reader's page dimensions, logical scale, reading mode,
and current page. It derives visible ranges, virtualized spacers, scroll bounds,
page-to-window transforms, fit scales, refit anchors, and positions to save.
It retains no separate mutable viewport and depends on neither GPUI nor the
PDF backend. Find, outline, and saved-position jumps retain their distinct
placement policies while using the same page-local-to-scroll conversion.
Same-document PDF links share the outline destination parser and navigation
path. Link rectangles retain PDF document coordinates and derive rotated,
cropped page geometry for hit targets. `/XYZ` positions use that geometry;
`/Fit` and `/FitH` switch to the corresponding fit modes. Visible link targets
are Tab-focusable and activate with Enter; unsupported destinations leave the
reader in place and report non-modal status.
`ReaderView` applies those results to its scroll handle, invalidates bitmaps
when scale changes, and coordinates selection and persistence.
Pure rules are tested under `src/reader/navigation/`; reader integration
tests exercise the real input, rendering, and history paths.

`PageRasters` retains GPUI image resources but has no entity, window, PDF path,
or background executor. The view supplies a borrowed `PageLayout` and measured
viewport, executes the chosen request through the PDF adapter, and returns its
reply with the current visible range. Each request has a unique ticket;
obsolete replies cannot install pixels or retire a newer request, even when
the page and scale match again. Document replacement, logical zoom, and display
density changes invalidate pixels, pending tickets, and old failures together.
An already-running backend call may finish after invalidation; it no longer
owns the current request slot.

Document loading and reading-mode save replies use the reader's document
generation, not a raster ticket. Moving between displays while a PDF opens
therefore cannot discard the document load. Raster scheduling, cache bounds,
and reply ordering have direct tests under `src/reader/rendering/`; composed
loading, zoom, density, and replacement tests live in
`src/reader/tests/rendering.rs`.

The reader loads page dimensions in background work. Continuous mode lays out
the document with spacers around visible pages; Single Page mode lays out only
the selected page and bounds the scroll region to that page. Each reader window
owns its live mode; the TOML opening default and optional PDF-specific override
are resolved when a document opens, without reconfiguring other open windows.
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
The app's initial zoom preference is captured for each document open (including
the first launch, file picker, Open Recent, and OS file-open); readers wait for
the initial asynchronous config load before loading their first PDF. Fit Width
and Fit Page use the same measured-viewport fitting as reader Actions. Later
preference edits do not refit open readers or replace their manual zoom.
The same open path restores an optional page-local position after page geometry
loads; each window still owns its live viewport. Positions use PDF points,
clamp within a changed page and reject missing page indices rather than
matching text across revisions. Explicit page turns persist the selected page
even if several short pages fit in one viewport and its scroll offset cannot
reach that page's top. An obsolete load cannot move a newer document.

### Settings and platform storage

`src/settings.rs` owns the stable `com` / `JonGretar` / `Kver` identity
and resolves config and local-data directories with `directories::ProjectDirs`.
Configuration lives at `config_dir()/config.toml`; reading history lives in
`data_local_dir()/reading.sqlite3`. Neither location depends on a PDF path or
the working directory. The config chapters match the Settings pages:
`[application]`, `[reader]`, and `[persistence]`. Appearance is
`[application].appearance` (default System);
initial zoom is `[reader].initial_zoom` (`100_percent`, `fit_width`, or `fit_page`,
default 100%); `[reader].reading_view` is `continuous` (default) or `page`.
Both reader defaults apply once per document open, not to existing readers.
`[application].quit_when_last_window_closes`, `[persistence].remember`, and
`[persistence].reopen_at_last_position` default to true. Disabling restoration
starts a new open at the beginning without erasing its previously saved place.
`[application].reopen_last_windows_on_startup` defaults to
false and is saved for future use; window restoration is not implemented.
Missing values use those defaults without writing a file until a choice is saved.
Legacy `[appearance]`, `[windows]`, and `[reading_history]` keys remain readable.
Explicit values in the new chapters take precedence. The next panel save moves
known legacy keys to their new chapters, preserving unrelated entries and comments.
The former `[application].theme_mode` key also remains readable and migrates to
`appearance` on save; an explicit `appearance` value takes precedence.

`src/settings.rs` validates the entire known `AppSettings` value before applying
external edits or panel changes; a `SettingsChange` updates one known field in
the latest valid TOML document. New preferences explicitly join this model
and the panel, rather than generating controls from arbitrary TOML keys.
`src/settings_ui.rs` owns the shared app-wide settings state and one Settings
window. Its navigation tree switches between Application (Theme and Windows),
Reader (Reading Experience), and Persistence (Reading History). Child nodes select
their parent page; grouping is visual, not an additional config nesting level.
The tree owns selection and keyboard navigation, and only the selected page's
controls are rendered. The panel retains the appearance reset control and the
confirmed clear-all history action.
Loading, periodic file checks, and saving TOML run off the UI thread. Valid
external changes update the shared model, theme, and open Settings controls;
invalid or partially written edits leave the last valid choice active and
show an error until correction. Panel choices commit only after the writer
re-reads a valid file, and saves are serialized through a temporary file in
the config directory. The writer retains unrelated TOML entries and comments
and refuses a file that changed while the replacement was prepared. A panel
choice cannot replace malformed config. Unavailable paths and read/write
errors leave the window usable and appear beside the setting.

`src/reading_history.rs` is the entry module for shared reading history.
Its private `store.rs` owns local PDF identity, record types, SQLite operations,
and schema migrations, without a GPUI dependency. Its private `worker.rs` owns
the single serialized request consumer, session state, deletion epoch, and
application-wide history/revision handles. Request methods enqueue immediately;
the returned futures only await replies, so dropping a reply does not cancel an
already submitted write. Schema and transaction tests live under
`reading_history/store/`; request-ordering and worker-lifecycle tests live under
`reading_history/worker/`. No panel has its own store or worker.
The former `recent` module path and existing store/import names remain re-exports
of this capability. A successful
page load records the canonical local path; failed and cancelled opens do not.
The `documents` table is the ownership boundary for per-PDF records: child tables
must reference its path with `ON DELETE CASCADE`, so removing a document
removes its saved reading data, not the PDF itself. The schema uses
`user_version` migrations and refuses newer versions rather than overwriting
them. The shared background worker serializes disk requests from all reader
windows. Its in-memory session index keeps new opens available across windows
when recording is off; saved records remain readable. The `reading_positions`
child table stores page indices and PDF-point offsets behind the same worker.
Re-enabling recording flushes current session paths and positions. Removing
one path or clearing all deletes the
SQLite parent rows and invalidates the matching session index. A generation
token captured when loading begins prevents an older in-flight open from
resurrecting data after an explicit deletion; a new open may record it again.
The `reading_modes` child table stores only explicit per-PDF choices. Absence
means the PDF follows the current TOML default on its next open. Mode reads and
writes use the same serialized worker as document records, with session-only
changes retained while history recording is off and flushed on re-enable.
The `local_marks` child table (schema version 5) stores lowercase letter,
page index, PDF-point offset, and an optional display name. The letter remains
the mark's identity and keyboard target; a name does not create an independent
bookmark. UI-only name editing preserves that name when the letter's location
is replaced, and removing the letter removes its name. Its shared worker caches
each PDF's complete mark set for session edits, reads previously saved marks
while recording is off, and flushes the resulting set (including deletions)
when recording resumes.
Mark writes are ordered with open, remove, and clear requests; deleting a parent
document cascades to its marks and clears the session copy.
`src/reader/marks.rs` owns local mark interaction in a `LocalMarks` entity for
one immutable document snapshot. It owns popup focus, letter selection, name
editing, history refreshes, and mark-operation replies. Its list is presentation
data from the shared worker, not a second per-PDF store. The reader supplies
the position to save and applies emitted jump destinations after checking the
source entity, document generation, and page bounds.

The marks surface uses Kit's base `Dialog` and `DialogPopup`, the same modal
behavior layer used by the styled Recent dialog. Kit owns backdrop dismissal,
focus trapping, and pointer occlusion; the document still owns whether the marks
surface exists. Removing a document or superseding the popup therefore removes
it without closing an unrelated dialog on the window's stack. Row deletion
returns focus to the persistent search field before its button disappears.

Replacing a document drops its marks owner and subscriptions. Closing or
reopening the popup cancels reads and invalidates editor replies; a late name
save cannot finish a new editor for the same letter. Refreshing the list does
not clear a pending save. Submitted writes finish independently of popup and
window lifetime and notify other windows to refresh from the worker, rather
than installing potentially obsolete mutation snapshots. Pending navigation
also checks the history deletion epoch. Lifecycle tests live beside the
feature; composed keyboard, naming, and document-replacement tests live in
`src/reader/tests/marks.rs`.

GPUI's multi-keystroke bindings hold `m` then a letter to set or replace,
`'` then a letter to jump, and `d m` then a letter to delete while the reader
is focused. The pending keymap sequence takes priority over ordinary one-key
reader shortcuts (such as `p` for Previous Page); no second letter handler
competes with the normal keymap. The command palette exposes the corresponding
operations and an Open marks popup; Command-Shift-M opens that popup directly.
The popup lists only set local letters, their page-local positions, and optional
names. Jumps reuse the saved-position coordinate transform, clamping offsets
within a changed page and rejecting missing pages.
No SQLite or config I/O runs on the UI thread. Open Recent searches both the
session index and database, including stored records beyond the first 100 shown.
It excludes the active reader's normalized PDF path before limiting results,
without removing that record for other windows or future opens. It does not
scan for missing PDFs on startup. Each recent row has a separate, accessible
Forget button; it confirms which PDF's saved data will be removed before
deleting the database record. Settings offers a separate confirmed clear-all
command; neither action deletes PDFs or changes TOML preferences. Open Recent
panels in other windows refresh after deletion. The recent-file
`Command` lives inside GPUI Kit's `Dialog`, so its modal backdrop, Escape
behavior, pointer hit testing, and focus restoration come from the window
overlay layer rather than a second hand-built reader overlay.

`src/reading_history/recent_picker.rs` owns the Open Recent dialog, query,
history-revision subscription, and search task. It receives the normalized
active path as an exclusion snapshot and emits an open request rather than
loading PDFs. `DocumentWindow` retains the picker and matches its pending
request to the reader's document generation. The reader emits replacement
and load-completion events without retaining a picker or referring to dialogs.
The window updates the active-path exclusion, cancels superseded requests,
and replies to the originating picker and request; a late completion cannot
dismiss a reopened dialog. Search, open, and removal
have independent busy state, so a search refresh cannot enable rows while an
open is pending. Confirmed removals complete and notify other windows even if
the originating picker closes. This retains the shared serialized history
worker and existing schema rather than introducing a separate picker store.
Composed Recent, focus, and cross-window history workflows are tested under
`src/app/document_window/tests/`; picker-only lifecycle tests remain beside
`recent_picker.rs`.

### Actions, palette, and menu

Open PDF, page turns, reading mode, zoom, and fit modes are GPUI Actions
handled by the active reader window. Toolbar controls, keybindings, and GPUI
Kit's command palette dispatch those same Actions; reader methods own the underlying
navigation and loading behavior. The document window owns the file picker;
its reader ignores obsolete asynchronous load results, so entry points share
error handling.
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

`src/app/document_window/command_palette.rs` owns the command palette's
catalogue, retained `CommandState`, focus trap, and overlay presentation.
`DocumentWindow` retains the palette and supplies a read-only availability
snapshot instead of a reader back-reference. The palette is a sibling of the
reader workspace, not its child. Window-local routing sends reader Actions
through GPUI's `FocusHandle::dispatch_action` to the existing reader handlers,
without moving focus or duplicating navigation policy. Actions already handled
inside the reader do not run again at the window. No application-global handler
chooses a reader on the palette's behalf.
Kit's `Command` owns search and selection;
`Root` handles the overlay's focus trap through Kit's `FocusTrapElement`.
The overlay has a dialog role and a rem-based maximum width so it remains
usable in narrow windows. A dismissal Action lets the host restore focus
without exposing itself to palette callbacks. Dismissing it restores the active sidebar panel's
focus when one is open, otherwise reader focus. A deferred palette dismissal
does not steal focus from a replacement Marks or Recent overlay. GPUI Kit
resolves shortcut hints from active keybindings. Bare reading keys apply only
while the reader has focus; text entry in the palette does not navigate the
PDF.

`DocumentWindow` owns one `ReaderSidebar` entity per window,
implemented in `src/app/document_window/sidebar.rs`. The host has no
`ReaderView` back-reference: resizing, panel selection, and focus transfer use
its own entity context. Its owner observes layout changes so PDF fit modes
follow the resized viewport. The host selects one panel and reserves a bounded
side of the window's reading area
(toolbar, document, and status bar), and transfers focus on open, switch, and
close. The toolbar's leading control, View menu, and command palette dispatch
the same `ToggleSidebar` Action, reopening the last-selected panel. Bottom,
equal-width `TabBar` tabs switch panels; the panel content owns the scroll
region above them. The macOS title bar stays across the full window to
preserve its native controls. `src/reader/find.rs` owns the Find query,
results, active selection, and batched search task in its own entity;
`src/reader/contents.rs` owns lazy outline loading, Contents focus, and GPUI
Kit's virtualized outline tree in a separate entity. Command-T dispatches
`ShowContents` from the active reader window even when Find has focus;
Command-F returns to Find. Kver's documents remain in
independent windows, not document tabs. The tree preserves PDF hierarchy,
supports collapse/expand with the arrow keys, and navigates when a usable row
is clicked or confirmed. Only the chevron toggles disclosure by pointer;
clicking the rest of a parent row navigates without folding it. Both panels
supply content to the same host; search and outline state remain separate.
The sidebar remains attached across document changes, preserving its width
and last panel. The window supplies both panels with document snapshots from
the reader's single document-generation owner. Contents cancels pending extraction and
clears its tree on replacement; it loads only when opened or when a new
document arrives while Contents is open. Its own activation Action and pointer
handlers emit generation-tagged destinations, and load completion transfers
focus only if its loading placeholder still has focus. Replacing a document
or query cancels Find's search task and invalidates
obsolete batches and queued navigation events. Closing the sidebar preserves
the query and results; replacing the document clears them. Find emits a
document-space match destination for the reader to apply and exposes the
active highlight without exposing mutable result state. The window retains
the subscription handles, checks document/query revisions, and routes
destinations to `ReaderView` for viewport placement. It supplies the selected
highlight as a read-only document-space snapshot; the reader clears it on
replacement and rejects snapshots for an old document. The surface does not
reach through the sidebar to read Find state. Find has no reader back-reference.
Search and outline lifecycle tests live beside their features; composed
navigation, focus, and sidebar-resize workflows live under
`src/app/document_window/tests/`. Reader geometry, raster bounds, and stale
highlight coverage remain under `src/reader/tests/`.

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

On macOS, `src/app/menu.rs` installs native Kver, File, Edit, Window, and View
menus at startup. Application-wide About, update, and Quit Actions are declared
in `src/app/commands.rs`. The former `app_menu` and `reader_windows` module
paths, along with `reader_sidebar`, remain available as re-exports, not
duplicate implementations.
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
on a background worker for visible pages. The reader's `Selection` state owns
glyph-boundary anchors, hit testing, drag lifetime, copied text, and page-local
highlight rectangles. `ReaderView` converts pointer coordinates, executes
background requests, projects highlights at the current zoom, and writes to
the clipboard. Zoom does not invalidate selection or page text.

Selection keeps visible page text plus the selected page so copying still
works after scrolling away in Continuous mode. One current text request runs
at a time; after completion, the next request uses the latest viewport.
Requests carry the reader's document generation, not its raster generation.
Replacing a document resets selection, retained text, failures, and the current
request; an old reply cannot install text or clear a replacement request.
Entering Single Page mode clears a selection on another page, and turning
pages in that mode clears the selection.
Pure selection and request-lifecycle tests live under
`src/reader/selection/`; pointer, copy, zoom, and replacement workflows live in
`src/reader/tests/selection.rs`. More general line grouping, rotated/cropped
text selection, and scans remain separate follow-ups.

## Future state

Navigation history, internal links, and independent named bookmarks remain
future work. The existing SQLite document record is the shared deletion
boundary for per-PDF positions, reading-mode overrides, and local marks.
Platform-specific menus, dialogs, and packaging belong at the application
boundary; PDF and navigation behavior should remain portable.
