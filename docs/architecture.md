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
reader icons used by fit controls; the full icon catalog is not embedded.
`src/reader_windows.rs` creates empty reader windows and cycles focus between
open windows; it does not share document or viewport state between readers.
Each window mounts its reader inside GPUI Kit's `Root`, which owns component
overlays and applies the theme's interface scale. The window uses Kit's
`TitleBar` options so its native controls and dragging agree with the document
title. Kver follows the system's light/dark appearance for the reader chrome;
the PDF page itself retains its document colors.
`src/reader.rs` owns the viewport, document identity, navigation state,
render scheduling, and window-local Actions. `src/pdf.rs` isolates Hayro:
it returns page dimensions, page geometry, positioned text, and premultiplied
BGRA pixels, not Hayro types, to the reader. The adapter currently reads and
parses the document for each raster and text request. Reusing a parsed
document or prefetching pages should be driven by measurements, not a new
worker architecture upfront.
See the [rendering validation](rendering-validation.md) for the backend choice
and accepted limitations.

### Rendering and navigation

The reader loads page dimensions in background work, then lays out the full
document with spacers around visible pages. It selects one raster job at a
time from the current viewport rather than queueing work that a scroll or
zoom might make obsolete. Old-document, old-scale, and offscreen results are
discarded. Only visible bitmaps are retained, within a 128 MiB budget;
offscreen pages remain placeholders. Rasterization accounts for display
density and passes pixels directly to GPUI without a PNG round trip.

The reader preserves a page-local position in PDF points across zoom and
viewport changes. Fixed zoom, fit width, and fit page use one scale for
continuous layout. Fit modes recompute from the current page and measured
scroll viewport on navigation or resize; manual zoom returns to fixed mode.
Scale changes invalidate obsolete bitmaps before new background renders
complete, without waiting on the UI thread.

### Actions, palette, and menu

Open PDF, page turns, zoom, and fit modes are GPUI Actions handled by the
active reader window. Toolbar controls, keybindings, and GPUI Kit's command
palette dispatch those same Actions; reader methods own the underlying
navigation and loading behavior. The reader owns the file picker and ignores
obsolete asynchronous load results, so entry points share error handling.
Document-dependent Actions are unavailable in an empty, loading, or error
window; each reader checks its own document state.
Kit's `TitleBar` holds the truncating filename and persistent Open PDF button;
the grouped `Toolbar` holds navigation, zoom, fit, and selection commands.
`StatusBar` shows page position and zoom without taking space from the PDF.
`Empty` shows the open, loading, or recoverable error state in the document
area. Control presentation uses Kit's semantic theme colors and component
states rather than a reader-specific palette.

The palette is an inline overlay owned by the reader. This keeps the reader
in its Action dispatch path rather than relying on a global handler that
could affect another window. Kit's `Command` owns search and selection;
`Root` handles the overlay's focus trap through Kit's `FocusTrapElement`.
The overlay has a dialog role and a rem-based maximum width so it remains
usable in narrow windows. Dismissing it restores reader focus. GPUI Kit
resolves shortcut hints from active keybindings. Bare reading keys apply only
while the reader has focus; text entry in the palette does not navigate the
PDF.

On macOS, `src/app_menu.rs` installs native Kver, File, Edit, and Window menus
at startup.
File → Open PDF dispatches the same window-local `OpenPdf` Action; the
application-wide `Quit` Action shuts down GPUI rather than just closing one
reader window. File → New Window creates an independent empty reader; Window →
Next Window and Previous Window cycle focus in either direction. These commands
use the same reader Actions as shortcuts and the palette; only the focused
window handles a window command. Closing a window does not quit while other
readers remain open. macOS menu setup stays separate from reader operations.

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

The reader currently has no persistence layer. SQLite is planned for
reading state and TOML for user-editable configuration, but neither has a
schema or implementation yet. Platform-specific menus, dialogs, and
packaging belong at the application boundary; the PDF and navigation
behavior should remain portable.
