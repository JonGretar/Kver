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

`src/main.rs` initializes GPUI Kit, keybindings, and the reader window.
`src/reader.rs` owns the viewport, document identity, navigation state,
render scheduling, and window-local Actions. `src/pdf.rs` isolates Hayro:
it returns page dimensions, page geometry, and premultiplied BGRA pixels,
not Hayro types, to the reader. The adapter currently reads and parses the
document for each raster request. Reusing a parsed document or prefetching
pages should be driven by measurements, not a new worker architecture upfront.
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

The palette is an inline overlay owned by the reader. This keeps the reader
in its Action dispatch path rather than relying on a global handler that
could affect another window. GPUI Kit resolves shortcut hints from active
keybindings. Bare reading keys apply only while the reader has focus; text
entry in the palette does not navigate the PDF.

On macOS, `src/app_menu.rs` installs native Kver and File menus at startup.
File → Open PDF dispatches the same window-local `OpenPdf` Action; the
application-wide `Quit` Action shuts down GPUI rather than just closing one
reader window. macOS menu setup stays separate from reader operations.

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
