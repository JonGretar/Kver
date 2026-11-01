# Architecture

## Overview

Kver is a native PDF reader in Rust using GPUI. Its continuous-scroll
prototype renders with Hayro, the chosen initial backend. Revisit the choice
if a concrete rendering or text-geometry limitation warrants it.
macOS is the first delivery target; Linux and Windows are separate targets.

The application separates PDF processing, document navigation, rendering orchestration, and UI components.

The primary architectural goal is to support a responsive reading experience without tightly coupling the application to a particular PDF engine.

## Stack

| Component             | Technology |
| --------------------- | ---------- |
| Language              | Rust       |
| UI framework          | GPUI       |
| PDF backend           | Hayro      |
| Planned persistence   | SQLite     |
| Planned configuration | TOML       |

### GPUI and rendering

The prototype uses `gpui-pre` with its matching platform crate. The optional
`screenshot` feature enables headless test support; normal GUI builds omit it.
`tests/visual.rs` runs on the process main thread for macOS Metal and checks
the reader's asynchronous rendering. It does not verify pointer hit-testing.

The prototype loads page dimensions once in background work and lays out the
whole document with spacer elements around the visible pages. It rasterizes one
visible page at a time on the background executor, selecting the next job from
the *current* viewport rather than filling a queue that can become obsolete.
Results from an old zoom or an offscreen page are discarded. Only visible
bitmaps are retained, subject to a 128 MiB budget; offscreen pages remain
placeholders. The UI receives premultiplied BGRA pixels directly rather than
encoding/decoding PNG. Zoom preserves the page-local reading position in PDF
points. Fixed zoom and fit-width/fit-page share one scale for continuous layout;
fit modes recalculate from the current page's dimensions and the measured scroll
viewport on navigation and window resize. Manual zoom exits fit mode. A scale
change invalidates obsolete bitmaps before background rendering, without waiting
on the UI thread. The adapter currently reparses the PDF for each raster request; a
parsed-document worker and prefetch are deferred until measurements justify them.

## Boundaries

The GPUI viewport handles interaction and display; reader actions remain
separate from physical keybindings. Background work obtains page geometry and
rasters from the PDF adapter. The UI receives backend-independent page
geometry and pixels rather than Hayro types. Preserve document coordinates
for future marks, text overlays, and reading positions.

### First reader action and palette (#34)

GPUI Kit 0.8.1 uses the same `gpui-pre` 0.3.10 snapshot as Kver. The
`screenshot` feature enables Kit's matching test support; the application
initializes Kit and registers its assets once before opening the reader.
For now, the reader owns one searchable `CommandState` while its palette is
visible, and renders `Command` inside its own action dispatch path. Its Open
PDF item carries the actual `OpenPdf` Action: Kit resolves the active binding
hint and dispatches the same Action as the toolbar and shortcut. The reader
owns the existing file-picker and document-loading flow, so entry points
cannot diverge on errors or stale asynchronous loads.

The palette is an inline overlay instead of a new dialog/window: it keeps the
reader as the Action handler's ancestor, without introducing a global handler
that could target the wrong window. This slice does not reorganize navigation
or add an app-wide palette registry; later reader Actions can extend the same
palette as their operations become available. The root key handler yields
unhandled input (and all keys while the palette is open) so search text and
Kit's Enter/Escape handling reach the focused palette. Headless tests exercise
these routes and rendering; they do not prove the native macOS picker or focus.

### Reading Actions (#35)

Page turns, zoom, and fit modes now enter through window-local GPUI Actions;
the existing reader methods remain the single source of navigation and scaling
behavior. Toolbar controls dispatch those Actions, while the palette renders
Kit `CommandItem`s carrying the same Actions and resolves their shortcut hints
from the active keymap. Bare reading keys are scoped to the reader only while
the palette is closed, so typing search terms does not turn pages or zoom.
Modified shortcuts remain active in the palette and supply its resolved hints.
Each reader checks its own loaded pages before handling a document Action;
loading, empty, and error windows disable the corresponding controls and
palette rows, leaving other windows untouched.

Apple silicon real-window check: Jón Grétar confirmed that the
`issue-35-reader-actions` branch worked as expected when asked to check
toolbar/keyboard page, zoom, and fit parity; palette search, shortcut hints,
typing without invoking reader commands, and focus after confirmation or
Escape; and unavailable reading commands in empty and error windows. The
specific Mac model and macOS version were not reported. Headless Metal tests
alone do not establish native focus and input behavior.

### macOS application menu (#36)

`app_menu` installs GPUI's native Kver and File menus only in the macOS
application startup path. The File menu holds the same `OpenPdf` Action as
the reader control, shortcut, and palette. GPUI dispatches menu actions into
the active window's focused element; the reader's existing window-local
handler owns the picker, error reporting, and obsolete-load invalidation.
The global `Quit` Action calls GPUI's application shutdown instead of
closing only the focused reader window. Command-O and Command-Q are
registered in the keymap before the menu is constructed so GPUI supplies
the menu's native shortcut labels. This does not add window switching or
platform-specific code to reader operations.

Headless menu tests inspect the installed Actions and route them through
GPUI's active-window dispatch, but its file picker cancels without a native
dialog. The native menu display, shortcut, and focus behavior require a
separate visible Apple silicon Mac check.

## PDF Backend

### Current and future needs

The backend boundary currently covers page dimensions, geometry, and rasterization.
Later reader features will also need:

- Positioned text extraction
- Link extraction
- Document outlines

Positioned text extraction is especially important for text selection, searching, and the visual ruler.

### Current backend

`src/pdf.rs` isolates Hayro from the UI. It supplies page dimensions and
rendered pixels for requested pages; the viewport displays visible pages and
discards obsolete render results. Hayro avoids a separate native PDF library.
PDFium (`pdfium-render`) is a comparator if a significant gap appears, not the
current implementation. See the [validation record](rendering-validation.md)
for the decision, evidence, and accepted limitations.

## Rendering Pipeline

The application should use virtualized page rendering.

Only pages within or near the visible viewport should be rendered.

Conceptual flow:

1. Determine visible page regions.
2. Request matching render operations.
3. Prioritize visible pages.
4. Render asynchronously.
5. Cache completed page images.
6. Upload/display page images through GPUI.
7. Evict old cache entries as appropriate.

### Cache and zoom

Keep retained bitmaps bounded and invalidate results when the document,
viewport, or scale changes. Rasterize at the requested display density off
the UI thread; preserve a page-local reading position through zoom.

## Coordinate Systems

The application must distinguish between:

1. PDF/document coordinates
2. Page-local viewport coordinates
3. Window/screen coordinates

Coordinate conversion should be centralized and tested.

Persistent positions must be expressed in document coordinates.

Be aware that PDF pages may have rotations, crop boxes, and non-default page origins.

## Document State

Keep the document identity, page-local reading position, and zoom state
independent of window dimensions. Add history, marks, search, and ruler state
when those features are implemented.

## Navigation

Keep navigation actions separate from input handling. Scrolling, page
movement, and zoom can be triggered by keyboard or UI controls without
changing the PDF backend. Later command parsing and configurable bindings
belong at the input boundary, not inside reader actions.

## Visual Ruler

The ruler is a later goal, not an MVP gate. Keep positioned text and
document-space coordinates available for line-based movement; scanned PDFs
need a positional fallback. Render the guide as a GPUI overlay so movement
does not rerasterize a page. Grouping text into reading-order lines,
especially across columns, remains an open problem.

## Persistence

SQLite is proposed for:

- Recently opened documents
- Reading positions
- Marks
- Navigation metadata
- Future bookmarks and highlights

Use TOML for user-editable settings and keybindings.

Database schema design is deferred until the persistence layer is implemented.

## Cross-Platform Support

The first downloadable MVP targets macOS; Linux and Windows have separate
delivery gates. Keep platform-specific file dialogs, menus, window behavior,
and packaging out of core document and navigation code.

## Open Questions

- What are the practical cross-platform limitations of GPUI?
- How should PDF rendering workers be managed?
- What text-line grouping strategy works best?
- Should PDF annotations eventually be written into PDFs or stored separately?
- What licensing model should Kver use?
