# Roadmap

Kver's first job is to read PDFs well. The first downloadable product is a
macOS MVP, not a replacement for every existing reader. Capabilities beyond
the daily reader are goals, not a fixed release sequence. See
[features](features.md) for the corresponding scope.

## 0. Validate the stack

**Outcome:** Evidence that the UI and an initial PDF backend can support the
reader before committing to a release.

**Current work:** A Rust/GPUI single-page prototype uses Hayro for PDF
rasterization, with the PDF work off the UI thread. The macOS window opens the
README-derived [sample PDF](../examples/readme-sample.pdf). The initial preview
appeared soft; the prototype now requests display-density-aware images, but
its quality still needs visual testing with real documents. Text
selection belongs to the daily-reader milestone. Synthetic crop/rotation
transforms, text positions on the sample, and a small
[public-PDF spot check](rendering-validation.md#public-pdf-spot-check) have been
recorded. [Platform build blockers](rendering-validation.md#platform-feasibility-and-current-build-limits)
are documented; full real-document, UI, and platform checks remain.

- Bootstrap Rust and GPUI; open a macOS window and render a PDF page at
  several scales without blocking input.
- Validate Hayro first with representative text, scanned, long, and
  rotated/cropped PDFs. Record rendering quality, responsiveness, packaging
  trade-offs, and usable positioned text geometry. Prefer it for the initial
  backend if it meets the reader's needs; compare PDFium (`pdfium-render`) on
  any significant gap before accepting a limitation or switching backends.
- Validate transforms between document and screen coordinates, including
  rotated/cropped pages. Scans should still render without a text layer.
- Check Linux and Windows feasibility early. Record blockers rather than
  promising release artifacts at this stage.
- Confirm Hayro as the initial backend based on the evidence, or document the
  blockers and the PDFium comparison needed to resolve them.

**Gate:** Pages render correctly at multiple scales; input remains responsive;
text geometry and coordinate transforms have been tested; backend and platform
risks are documented. A full PDFium implementation is not required if Hayro
meets these checks. This is a technical prototype, not a release. It need not
implement the visual ruler.

## 1. Ship a macOS MVP

**Outcome:** A buildable, downloadable PDF reader for macOS.

- Open local PDFs and report failures clearly.
- Show and navigate pages with continuous scrolling, zoom, fit width, and fit
  page. Keep scrolling and zooming responsive in long documents by rendering
  visible pages in background work and bounding cache memory.
- Provide basic predefined keyboard controls for scrolling, pages, and zoom,
  alongside ordinary mouse interactions. A full Vim command language and
  configurable bindings are not release blockers.
- Package a downloadable macOS build and test it with long, scanned,
  rotated/cropped, and text PDFs. No OCR is promised.
- Include password-protected PDFs if support is straightforward; otherwise
  give an understandable error and keep support as an explicit later goal.

**Gate:** Someone can download and build the macOS app, open ordinary PDFs,
read across pages, and use basic keyboard/mouse navigation without UI stalls or
unbounded page loading. An unsigned preview is acceptable; paid Developer ID
signing and notarization are not prerequisites. Communicate the resulting
macOS security prompts with the download.

## 2. Make it a daily reader

**Outcome:** Common reading workflows no longer require another PDF reader.

- Search document text and move between highlighted results.
- Select and copy text with the mouse.
- Follow internal links and show the document outline when present.
- Navigate back and forward and restore the last reading position.

**Gate:** These workflows work on representative PDFs, and navigation remains
predictable after zoom and scrolling. The order within this milestone is
flexible. Search-result highlights are transient, not saved annotations.

## Separate platform delivery

Linux and Windows remain product targets. Their compatibility is checked
during technical validation, but each needs its own build, packaging, and PDF
regression checks before a downloadable release. Neither blocks the macOS MVP
or daily-reader milestones.

## Goals to schedule after evidence

Keep these visible without implying a date or order:

- Visual ruler: keyboard movement along text lines, a positional fallback on
  scans, and stable placement through zoom/scroll. Validate its coordinate and
  text-geometry prerequisites in Phase 0; implement it when ready.
- Configurable keybindings, numeric counts, multi-key commands, named marks,
  recent-document navigation, and a command palette.
- Multiple-document switching, tabs or windows, and drag-and-drop.
- Password-protected PDFs if not shipped in the MVP.
- Notes, persistent highlights, and annotation interchange.
- Reload after PDF rebuilds and SyncTeX source/PDF navigation for LaTeX.
- Split views, snapshots or overviews, smart reference jumping, and portals.

Promote a goal into a delivery milestone when its user value and technical
cost are understood. Protect the core reading experience while doing so.
See [architecture](architecture.md) for boundaries that keep these additions
possible without building them prematurely.
