# Roadmap

## Phase 0 — Technical Validation

**Goal:** Confirm that the proposed technology stack is viable.

Tasks:

- [ ] Create initial Rust project
- [ ] Initialize GPUI application
- [ ] Open a native application window
- [ ] Evaluate PDFium through `pdfium-render`
- [ ] Evaluate Hayro / `gpui-pdf`
- [ ] Render a PDF page
- [ ] Test page rendering at different scales
- [ ] Extract text and text geometry
- [ ] Validate macOS support
- [ ] Investigate Linux and Windows compatibility
- [ ] Record findings and choose initial PDF backend

Acceptance criteria:

- PDF pages render correctly
- Text geometry is available
- UI remains responsive during rendering
- No fundamental cross-platform blockers are identified

## Phase 1 — Reader Prototype

**Goal:** Create a minimal PDF reader suitable for reading ordinary documents.

Tasks:

- [ ] Open local PDFs
- [ ] Display multiple pages
- [ ] Implement continuous scrolling
- [ ] Add zoom controls
- [ ] Add fit-width and fit-page modes
- [ ] Implement basic page virtualization
- [ ] Render pages asynchronously
- [ ] Add page image caching
- [ ] Implement basic Vim navigation
- [ ] Add an initial visual ruler overlay
- [ ] Add line-aware ruler movement
- [ ] Add fallback ruler movement

Acceptance criteria:

- A long PDF can be read comfortably
- Scrolling and keyboard navigation feel responsive
- The visual ruler works on text-based documents
- Zooming does not block the UI

## Phase 2 — Practical Daily Reader

**Goal:** Make Folio useful for everyday document reading.

Tasks:

- [ ] Implement text search
- [ ] Highlight search matches
- [ ] Implement text selection and copying
- [ ] Add PDF link navigation
- [ ] Add table of contents
- [ ] Implement navigation history
- [ ] Add marks
- [ ] Restore reading positions
- [ ] Add recently opened documents
- [ ] Support password-protected PDFs

Acceptance criteria:

- Folio can replace a conventional PDF reader for common workflows
- Navigation state behaves predictably
- Text interactions work on representative PDFs

## Phase 3 — Desktop Application

**Goal:** Prepare Folio for external beta testing.

Tasks:

- [ ] Add command palette
- [ ] Add user-configurable keybindings
- [ ] Add settings
- [ ] Add document tabs or equivalent switching
- [ ] Implement drag-and-drop
- [ ] Improve errors and loading states
- [ ] Add application icon
- [ ] Implement macOS packaging
- [ ] Implement Linux packaging
- [ ] Implement Windows packaging
- [ ] Test on all supported platforms

Acceptance criteria:

- Installation is straightforward
- Keyboard and mouse interaction are polished
- Common PDFs work reliably
- The application is ready for early users

## Phase 4 — Advanced Reading

**Goal:** Expand Folio beyond a basic reader.

Potential tasks:

- [ ] Text highlights
- [ ] Persistent bookmarks
- [ ] Improved navigation across documents
- [ ] Split views
- [ ] Link hints
- [ ] Keyboard-driven text selection
- [ ] Advanced visual ruler behavior
- [ ] Annotation support

These features should be prioritized based on actual usage.

## Phase 5 — Research Features

**Goal:** Introduce advanced Sioyek-inspired functionality.

Potential tasks:

- [ ] Smart jumping to figures and tables
- [ ] Bibliography reference navigation
- [ ] Overview windows
- [ ] Portals
- [ ] Advanced document structure detection

These features are not required for the initial release.

## Engineering Priorities

At every phase:

1. Preserve UI responsiveness.
2. Avoid unnecessary architectural complexity.
3. Keep PDF backend details isolated.
4. Test with real-world PDFs.
5. Maintain cross-platform portability.
6. Document major architectural decisions.

## Immediate Next Steps

1. Bootstrap the Rust + GPUI application.
2. Implement PDF page rendering using PDFium.
3. Test positioned text extraction.
4. Investigate the existing gpui-pdf implementation.
5. Prototype the visual ruler.
6. Evaluate rendering performance.
7. Confirm the initial architecture before expanding scope.
