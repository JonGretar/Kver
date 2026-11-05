# Roadmap

Kver's first job is to read PDFs well. The first downloadable product is an
Apple silicon macOS MVP, not a replacement for every existing reader.
The next gate improves the reader experience; other goals have no fixed
release sequence. See [features](features.md) for the corresponding scope.

## Where we are

| Milestone | Status | What that means |
| --- | --- | --- |
| 0. Validate the stack | **Complete** | Hayro and GPUI were validated for the initial macOS prototype, with documented limitations. |
| 1. Ship a macOS MVP | **Complete** | The v0.1.0 Apple silicon app is downloadable; its archive and real-Mac launch and reading behavior have been checked. |
| 2. Build an action-driven reader | **Complete** | The integrated reader passed headless checks and an Apple silicon native-window check for actions, independent windows, and visible text copy ([#42](https://github.com/JonGretar/Kver/issues/42)). |
| 3. Find and navigate a document | **Planned** | Find text and navigate the PDF's contents through two panels in one sidebar ([spec #54](https://github.com/JonGretar/Kver/issues/54)); no release version or date is assigned. |
| Linux and Windows delivery | **Not scheduled** | Separate release gates; they do not block macOS. |

**Next work:** Validate Find and Contents together in a real Mac window
([#58](https://github.com/JonGretar/Kver/issues/58)) after the
[Find implementation (#56)](https://github.com/JonGretar/Kver/issues/56)
lands alongside the completed [Contents (#57)](https://github.com/JonGretar/Kver/issues/57)
and [shared sidebar (#55)](https://github.com/JonGretar/Kver/issues/55).
The [Find and Contents spec (#54)](https://github.com/JonGretar/Kver/issues/54)
defines this gate; history, internal links, and the other
[unscheduled goals](#goals-to-schedule-after-evidence) remain deferred.
GitHub Issues tracks live ownership and blockers; the
[feature workflow](agents/workflow.md) explains how to check work status.

## 0. Validate the stack

**Outcome:** Evidence that the UI and an initial PDF backend can support the
reader before committing to a release.

**Phase 0 decision:** Hayro is the chosen initial backend for the macOS
prototype; PDFium is a comparator if a concrete gap warrants revisiting it.
The [validation record](rendering-validation.md) contains the rendering and
coordinate evidence, accepted annotation-appearance limitation, and platform
checks. It does not certify all real PDFs or a Windows build. Text selection
belongs to the next reader-experience gate.

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

**Shipped in v0.1.0:** Open a PDF by path or from the app; show loading
errors; scroll continuously through pages rendered in background work with
bounded bitmaps; navigate and zoom with keys or toolbar controls. An
[unsigned Apple silicon `.app`](macos-app.md) can be built locally or
downloaded from the GitHub Release.

**Completed for this gate:**

- [x] Open a local PDF from the app without a command-line argument, with
  understandable failures ([#12](https://github.com/JonGretar/Kver/issues/12)).
- [x] Fit width, fit page, and predefined keyboard zoom controls, with
  predictable position across changes in zoom and viewport size
  ([#13](https://github.com/JonGretar/Kver/issues/13),
  [#14](https://github.com/JonGretar/Kver/issues/14)).
- [x] Check reading, responsiveness, and bounded memory on representative
  long, scanned, rotated/cropped, and text PDFs; resolve release-blocking
  findings ([#17](https://github.com/JonGretar/Kver/issues/17)).
  [Validation evidence](mvp-reader-validation.md) includes the
  short-final-page navigation and fit-mode fix
  ([#31](https://github.com/JonGretar/Kver/issues/31)); the subsequent
  release-window checks are recorded under #18.
- [x] Automate a versioned GitHub Release with a downloadable unsigned Apple
  silicon macOS app, then verify its launch and reading behavior in a real Mac
  window. Explain macOS security prompts
  ([#16](https://github.com/JonGretar/Kver/issues/16),
  [#18](https://github.com/JonGretar/Kver/issues/18)).
  See the [v0.1.0 release and downloaded-app evidence](mvp-reader-validation.md#v010-downloaded-release-check-18).

These are milestone outcomes, not one checkbox per ticket. Check each off only
when the full outcome is complete; GitHub issues hold the finer task graph.

**Non-blocking investigation (not an MVP release gate):**

- [x] Investigate mouse text selection and copy on an ordinary text PDF; document
  why the current bitmap-only reader cannot demonstrate it and recommend keeping
  implementation after the MVP
  ([#15](https://github.com/JonGretar/Kver/issues/15)).

An experimental Linux binary may accompany the GitHub Release if it is simple
to produce; a supported Linux release retains its separate delivery gate.
Password-protected PDFs are optional for this gate: include them if
straightforward; otherwise show an understandable error and track support
as a later goal.

**Gate:** Someone can download and build the macOS app, open ordinary PDFs,
read across pages, and use basic keyboard/mouse navigation without UI stalls or
unbounded page loading. An unsigned preview is acceptable; paid Developer ID
signing and notarization are not prerequisites. Communicate the resulting
macOS security prompts with the download. A full Vim command language,
configurable bindings, and OCR are not release blockers.

## 2. Build an action-driven reader

**Outcome:** Kver feels usable as a PDF reader, not just a rendering prototype.
The [reader-experience spec (#33)](https://github.com/JonGretar/Kver/issues/33)
defines this gate; its child tickets track delivery and dependencies.

- Refresh the empty, loading, error, and reading UI with GPUI Kit. Decide the
  exact look during implementation, not in the roadmap.
- Make everyday operations discoverable through shared Actions, controls,
  shortcuts, an application menu, and a searchable app-action command palette.
- Open and switch independent reader windows without tabs.
- Select visible text with the mouse and copy it from ordinary text PDFs,
  including useful spaces and non-ASCII mapping where supported. Do not
  pretend that image-only scans contain selectable text.

**Gate:** A reader can use the controls, menu, shortcuts, and palette to open
and navigate PDFs in independent windows, and select and copy visible text.
Reader-level checks cover zoom, scrolling, rotated/cropped pages, and scans;
a real-Mac check confirms menus, window switching, UI usability, and actual
drag-to-copy/paste. Record text extraction limits rather than promising
perfect copy order for every PDF. This gate does not commit to another release
version or date.

The integrated checks and native-window confirmation are recorded in
[#42](https://github.com/JonGretar/Kver/issues/42). Selection is page-local,
does not guarantee reading order for complex layouts or missing text mappings,
and does not perform OCR on image-only PDFs.

## 3. Find and navigate a document

**Outcome:** Find text across the current PDF and use its table of contents
without crowding the reading area. The [spec (#54)](https://github.com/JonGretar/Kver/issues/54)
defines the agreed scope and links the implementation tickets and dependencies.

- Find mapped document text, navigate matches, and show temporary page
  highlights and result snippets.
- Show the PDF's outline when available and navigate its entries.
- Use one sidebar with separate Find and Contents panels, one visible at a
  time. Keep the sidebar dismissible and each reader window independent.

**Gate:** Search and outline navigation work on representative PDFs, including
long, rotated/cropped, scanned, and no-outline cases; obsolete search results
do not replace a newer query or document. A real-Mac window check confirms
focus, controls, panel switching, and result/outline navigation. OCR,
persistent highlights, configurable panels, and trackpad pinch-to-zoom are
not release requirements. No release version or date is assigned to this gate.

## Separate platform delivery

Linux and Windows remain product targets. Their compatibility is checked
during technical validation, but each needs its own build, packaging, and PDF
regression checks before a downloadable release. Neither blocks the macOS MVP
or the reader-experience milestone.
Intel macOS builds also need an Intel-machine check before being offered as a
supported download; they are not part of the first Apple silicon release.

## Goals to schedule after evidence

Keep these visible without implying a date or order:

- Internal links; navigation history and restoration of the last reading
  position.
- Visual ruler: keyboard movement along text lines, a positional fallback on
  scans, and stable placement through zoom/scroll. Validate its coordinate and
  text-geometry prerequisites in Phase 0; implement it when ready.
- Configurable keybindings, numeric counts, multi-key commands, named marks,
  recent-file search, and drag-and-drop.
- Password-protected PDFs if not shipped in the MVP.
- Notes, persistent highlights, and annotation interchange.
- Reload after PDF rebuilds and SyncTeX source/PDF navigation for LaTeX.
- Split views, snapshots or overviews, smart reference jumping, and portals.

Promote a goal into a delivery milestone when its user value and technical
cost are understood. Protect the core reading experience while doing so.
See [architecture](architecture.md) for boundaries that keep these additions
possible without building them prematurely.
