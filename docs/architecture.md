# Architecture

## Overview

Kver is a native PDF reader in Rust using GPUI. Its current single-page
prototype renders with Hayro; the production PDF backend remains undecided.
macOS is the first delivery target; Linux and Windows are separate targets.

The application separates PDF processing, document navigation, rendering orchestration, and UI components.

The primary architectural goal is to support a responsive reading experience without tightly coupling the application to a particular PDF engine.

## Proposed Stack

| Component           | Technology                           |
| ------------------- | ------------------------------------ |
| Language            | Rust                                 |
| UI framework        | GPUI                                 |
| PDF backend         | PDFium / pdfium-render (provisional) |
| Alternative backend | Hayro                                |
| Persistent state    | SQLite                               |
| Configuration       | TOML                                 |

### UI components to revisit

[GPUI Kit](https://github.com/longbridge/gpui-kit) bundles GPUI with styled
controls, themes, and accessibility helpers. It could be useful when Kver
needs search, settings, and other desktop controls. The prototype currently
uses `gpui` 0.2.2 directly; GPUI Kit 0.8.1 pins a different GPUI release
(`gpui-pre` 0.3.10). Evaluate a single-version migration when those controls
are needed rather than adding both UI stacks now. It does not replace PDF
rendering or text geometry.

## High-Level Architecture

This diagram shows prospective responsibilities, including features scheduled
after the MVP. It is not a requirement to implement every branch at once.

```text
Application
│
├── UI (GPUI)
│   ├── Main window
│   ├── Document viewport
│   ├── Visual ruler overlay
│   ├── Search interface
│   ├── Command palette
│   └── Table of contents
│
├── Commands
│   ├── Keybindings
│   ├── Vim-style command parser
│   └── Application actions
│
├── Reader
│   ├── Document state
│   ├── Navigation
│   ├── Marks and history
│   ├── Text selection
│   └── Visual ruler
│
├── Rendering
│   ├── Page scheduling
│   ├── Background workers
│   ├── Page cache
│   └── Coordinate transformations
│
├── PDF Backend
│   ├── Document loading
│   ├── Page rasterization
│   ├── Text geometry
│   ├── Links
│   └── Document outlines
│
└── Persistence
    ├── Reading positions
    ├── Marks and bookmarks
    └── Settings
```

These are conceptual modules, not a mandatory directory structure.

## PDF Backend

### Requirements

The PDF backend should provide:

- Document opening and closing
- Page count and dimensions
- Page rendering at requested resolutions
- Positioned text extraction
- Link extraction
- Document outlines
- Coordinate transformations

Positioned text extraction is especially important for text selection, searching, and the visual ruler.

### PDFium

PDFium is an initial candidate, not a selected backend. Validate the claims
below against real PDFs alongside Hayro / `gpui-pdf`.

Advantages:

- Mature PDF rendering
- Broad PDF compatibility
- Text extraction and geometry APIs
- Permissive licensing

Disadvantages:

- Native binary dependency
- Packaging complexity
- Thread-safety and concurrency considerations

The `pdfium-render` crate provides Rust bindings.

### Hayro and gpui-pdf

Hayro is a pure-Rust PDF engine.

The `gpui-pdf` project provides an existing PDF viewing component for GPUI, built around Hayro.

Advantages:

- Pure Rust implementation
- Potentially easier cross-platform distribution
- Existing GPUI integration

Disadvantages:

- Less mature PDF compatibility
- Potential integration issues with GPUI versions
- Features and performance require evaluation

### Decision

The prototype isolates Hayro behind `src/pdf.rs`, which returns an encoded
page image to the UI. This adapter is deliberately limited to page one and
does not constitute the eventual backend interface.

The backend choice remains open. Compare rendering accuracy, responsiveness,
positioned text, document/screen coordinate handling, packaging, and Linux
and Windows feasibility before choosing the initial backend. Validate scans
and rotated/cropped pages. The prototype need not implement a visual ruler.

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

### Cache

A bounded LRU cache is a reasonable initial approach.

Cache keys should account for:

- Document identity and revision
- Page number
- Render scale or resolution
- Rendering options that affect the output

Avoid unlimited memory growth.

### Zooming

Support immediate visual scaling of cached images followed by asynchronous high-resolution rendering.

This provides responsive zoom behavior without requiring synchronous PDF rendering.

## Coordinate Systems

The application must distinguish between:

1. PDF/document coordinates
2. Page-local viewport coordinates
3. Window/screen coordinates

Coordinate conversion should be centralized and tested.

Persistent positions must be expressed in document coordinates.

Be aware that PDF pages may have rotations, crop boxes, and non-default page origins.

## Document State

Each open document should maintain:

- Document identity
- Current reading position
- Zoom mode and scale
- Navigation history
- Marks
- Search state
- Visual ruler state

A reading position should include at least:

- Page number
- Position within that page

It should not depend on the current window dimensions.

## Navigation

Navigation is separate from input handling.

Examples of navigation actions:

- Scroll up/down
- Move to next/previous page
- Go to page
- Navigate backward/forward
- Set mark
- Jump to mark
- Move ruler to next/previous line
- Follow link

Actions can be triggered from different inputs.

## Keyboard Commands

The macOS MVP needs predefined controls for scrolling, pages, and zoom. Keep
actions independent of their input so later work can add configurable
keybindings, multi-key sequences, counts, input contexts, and a command
palette without changing the reader or PDF backend.

Examples of possible later sequences (not MVP defaults):

- `j` / `k`: scroll
- `gg`: document beginning
- `G`: document end
- `10j`: repeat movement
- `ma`: set mark
- `'a`: jump to mark
- `/`: search

Exact bindings will be defined during development. Test any multi-key parser
when it is introduced.

## Visual Ruler

The visual ruler is a desired reading feature, not a macOS MVP requirement
or a scheduled next release. Technical validation should preserve the
document-space and text-geometry prerequisites without building the ruler.

It provides a visual guide for following text while reading.

### Modes

When implemented:

- Text-line-aware mode
- Coordinate-based fallback mode

### Text-Line-Aware Mode

Where PDF text geometry is available:

1. Extract positioned text.
2. Group text into logical lines.
3. Determine line bounds and reading order.
4. Move the ruler between lines.
5. Draw an overlay over the target line.

PDFs do not necessarily provide explicit logical lines, so line grouping requires heuristics.

Multi-column layouts need particular attention.

### Fallback Mode

For scanned PDFs or pages without usable text geometry, allow movement by fixed visual increments.

### Rendering

The ruler is a GPUI overlay.

Movement must not trigger PDF page rasterization.

It should support configurable:

- Color
- Opacity
- Height
- Movement behavior

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

Target platforms:

- macOS: first downloadable MVP, initially unsigned
- Linux: separate delivery gate after feasibility checks
- Windows: separate delivery gate after feasibility checks

Avoid platform-specific logic in core document and navigation code.

Platform-specific integrations may be required for:

- File dialogs
- Application menus
- File associations
- Window behavior
- Packaging
- Native PDF library loading

## Non-Goals for Initial Development

- PDF editing
- OCR
- Digital signatures
- Complex annotation tools
- Plugin architecture
- Cloud synchronization
- Smart academic reference detection

These may be considered later.

## Open Questions

- PDFium or Hayro as the primary backend?
- Can gpui-pdf be reused with the selected GPUI version?
- What are the practical cross-platform limitations of GPUI?
- How should PDF rendering workers be managed?
- What text-line grouping strategy works best?
- Should PDF annotations eventually be written into PDFs or stored separately?
- What licensing model should Kver use?
