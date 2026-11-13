# Roadmap

Kver's first job is to read PDFs well. The first downloadable product is an
Apple silicon macOS MVP, not a replacement for every existing reader.
The action-driven, Find/Contents, and persistence/reading-mode gates are
complete; further goals have no fixed release sequence. See
[features](features.md) for their scope.

## Where we are

| Milestone | Status | What that means |
| --- | --- | --- |
| 0. Validate the stack | **Complete** | Hayro and GPUI were validated for the initial macOS prototype, with documented limitations. |
| 1. Ship a macOS MVP | **Complete** | The v0.1.0 Apple silicon app is downloadable; its archive and real-Mac launch and reading behavior have been checked. |
| 2. Build an action-driven reader | **Complete** | The integrated reader passed headless checks and an Apple silicon native-window check for actions, independent windows, and visible text copy ([#42](https://github.com/JonGretar/Kver/issues/42)). |
| 3. Find and navigate a document | **Complete** | Find and Contents work together in one sidebar, with headless and real-Mac checks ([#58](https://github.com/JonGretar/Kver/issues/58)); no release version or date is assigned. |
| 4. Remember and configure reading | **Complete** | Settings, recent files, positions, Single Page, and local marks work together ([#77](https://github.com/JonGretar/Kver/issues/77)); no release version or date is assigned. |
| Linux and Windows delivery | **Not scheduled** | Separate release gates; they do not block macOS. |

**Next work:** No further reader gate is scheduled. Internal and another-PDF
links, the jumplist, and Back/Forward controls are covered by
[navigation spec #96](https://github.com/JonGretar/Kver/issues/96).
Portable text markup and the Annotations list are complete within
[spec #123](https://github.com/JonGretar/Kver/issues/123), with integrated
and native-Mac acceptance recorded in
[#134](https://github.com/JonGretar/Kver/issues/134). This includes all four
types, shared editing and save state, and external-change handling.
The accepted malformed-array rendering limitation
([#142](https://github.com/JonGretar/Kver/issues/142)), scripted PDFKit
Squiggly rewriting limitation, and unavailable Acrobat validation remain explicit.
No release date or numbered milestone is assigned.
GitHub Issues tracks live ownership and blockers for subsequent keyboard
work; the [feature workflow](agents/workflow.md) explains how to check status.

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
meets these checks. This is a technical prototype, not a release.

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
The combined validation and supported limits are recorded in
[#58](https://github.com/JonGretar/Kver/issues/58).

## 4. Remember and configure reading

**Outcome:** A reader can set app-wide defaults, find a previously opened PDF,
resume its place, switch between continuous and single-page reading, and return
to document-local letter marks. The [spec (#67)](https://github.com/JonGretar/Kver/issues/67)
defines the scope and dependencies.

Settings stores user-editable app defaults in TOML and applies valid external
edits while running. SQLite stores recent files, positions, PDF-specific reading
modes, and local `a`–`z` marks. Turning off history recording keeps new per-PDF
changes usable in the running app without saving them; explicit clear actions
remove saved data. Single Page scroll stays on the current page until an explicit
page turn. On macOS, the last-window quit choice can leave Kver running without
a reader window; New Window, Dock activation, and Finder file-open still work.

**Gate:** Integrated reader tests cover isolated storage, live configuration,
privacy controls, reopening, changed PDFs, and independent windows. A native
Apple silicon check covers Settings, recent files, Single Page input, marks,
restart, and zero-window recovery. [#77](https://github.com/JonGretar/Kver/issues/77)
records validation and remaining limits. Linux and Windows delivery remain
separate gates; this milestone has no assigned release version or date.

## Separate platform delivery

Linux and Windows remain product targets. Their compatibility is checked
during technical validation, but each needs its own build, packaging, and PDF
regression checks before a downloadable release. Neither blocks the macOS MVP
or the reader-experience milestone.
Intel macOS builds also need an Intel-machine check before being offered as a
supported download; they are not part of the first Apple silicon release.

## Goals to schedule after evidence

Keep these visible without implying a date or order:

- Internal links and back/forward navigation history.
- Visual ruler: keyboard movement along text lines, a positional fallback on
  scans, and stable placement through zoom/scroll.
- Configurable keybindings, numeric counts, a general multi-key command
  language, independent named bookmarks, and drag-and-drop.
- Password-protected PDFs if not shipped in the MVP.
- Notes and broader annotation authoring beyond the completed text-markup scope.
- Reload after PDF rebuilds and SyncTeX source/PDF navigation for LaTeX.
- Split views, snapshots or overviews, smart reference jumping, and portals.

Promote a goal into a delivery milestone when its user value and technical
cost are understood. Protect the core reading experience while doing so.
See [architecture](architecture.md) for boundaries that keep these additions
possible without building them prematurely.
