# Roadmap

Kver's first job is to read PDFs well. The first downloadable product is an
Apple silicon macOS MVP, not a replacement for every existing reader.
Capabilities beyond the daily reader are goals, not a fixed release sequence. See
[features](features.md) for the corresponding scope.

## Where we are

| Milestone | Status | What that means |
| --- | --- | --- |
| 0. Validate the stack | **Complete** | Hayro and GPUI were validated for the initial macOS prototype, with documented limitations. |
| 1. Ship a macOS MVP | **In progress** | Continuous reading and a local app bundle work, but the app is not yet a downloadable reader. |
| 2. Make it a daily reader | **Not started** | Search, selection, links, outline, history, and position restoration follow the MVP. |
| Linux and Windows delivery | **Not scheduled** | Separate release gates; they do not block macOS. |

**Next work:** See the open child issues of the approved
[macOS MVP spec (#11)](https://github.com/JonGretar/Kver/issues/11) for live
ownership, blockers, and the next ready ticket. The
[feature workflow](agents/workflow.md) explains how to check work status.

## 0. Validate the stack

**Outcome:** Evidence that the UI and an initial PDF backend can support the
reader before committing to a release.

**Phase 0 decision:** Hayro is the chosen initial backend for the macOS
prototype; PDFium is a comparator if a concrete gap warrants revisiting it.
The [validation record](rendering-validation.md) contains the rendering and
coordinate evidence, accepted annotation-appearance limitation, and platform
checks. It does not certify all real PDFs or a Windows build. Text selection
remains in the daily-reader milestone.

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

**Already in the prototype:** Open a PDF by path or from the app; show loading
errors; scroll continuously through pages rendered in background work with
bounded bitmaps; navigate and zoom with keys or toolbar controls. A
[local unsigned `.app`](macos-app.md) can be built, but is not a download.

**Still needed for this gate:**

- [x] Open a local PDF from the app without a command-line argument, with
  understandable failures ([#12](https://github.com/JonGretar/Kver/issues/12)).
- [ ] Fit width, fit page, and predefined keyboard zoom controls, with
  predictable position across changes in zoom and viewport size
  ([#13](https://github.com/JonGretar/Kver/issues/13),
  [#14](https://github.com/JonGretar/Kver/issues/14)).
- [ ] Check reading, responsiveness, and bounded memory on representative
  long, scanned, rotated/cropped, and text PDFs; resolve release-blocking
  findings ([#17](https://github.com/JonGretar/Kver/issues/17)).
- [ ] Automate a versioned GitHub Release with a downloadable unsigned Apple
  silicon macOS app, then verify its launch and reading behavior in a real Mac
  window. Explain macOS security prompts
  ([#16](https://github.com/JonGretar/Kver/issues/16),
  [#18](https://github.com/JonGretar/Kver/issues/18)).

These are milestone outcomes, not one checkbox per ticket. Check each off only
when the full outcome is complete; GitHub issues hold the finer task graph.

**Non-blocking investigation (not an MVP release gate):**

- [x] Investigate mouse text selection and copy on an ordinary text PDF; document
  why the current bitmap-only reader cannot demonstrate it and recommend keeping
  implementation in the daily-reader milestone
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
Intel macOS builds also need an Intel-machine check before being offered as a
supported download; they are not part of the first Apple silicon release.

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
