# Issue #15: text-selection feasibility (investigation, not a feature)

**Decision recommendation:** Keep mouse selection and copy in the daily-reader
milestone. This investigation does not add selection to the macOS MVP or close
the implementation gap. Promoting a bounded version requires a separate scope
decision and an interactive acceptance test.

## Starting point and method

On the independent `issue-15-selection-investigation` branch, `HEAD` and
fetched `origin/main` were both `89e9fb5724335dff6e6f534bd19b7373a5aacebf`.
[Issue #15](https://github.com/JonGretar/Kver/issues/15) was open and
unassigned, with no native blockers, start comments, or linked open PRs; it
was assigned to `JonGretar` and given a
[start comment](https://github.com/JonGretar/Kver/issues/15#issuecomment-6501239419).
The [roadmap](../roadmap.md#1-ship-a-macos-mvp) explicitly calls this a
nonblocking investigation; [features](../features.md#daily-reader-milestone)
keeps selection in the daily-reader milestone.

This checked the pinned Hayro 0.8.0 text callback with the existing
[`text_geometry_probe`](../../examples/text_geometry_probe.rs) and compared
ordinary text with Poppler `pdftotext -layout`. It did **not** introduce a
selection implementation or simulate a user dragging in the window. The
ordinary fixture is the bundled
[`readme-sample.pdf`](../../examples/readme-sample.pdf), generated from
[`reader-sample.typ`](../../examples/reader-sample.typ). The other PDFs were
downloaded into ignored `target/selection-investigation/`, not committed.

## Why the two-zoom reader demonstration is blocked

The [reader page](../../src/reader.rs) is a `RenderImage` inside a GPUI `div`;
the page has neither mouse-down/drag handlers nor an accessible text layer or
copy action. Its [background render](../../src/reader.rs) supplies only bitmap
pixels. [`pdf::page_geometry`](../../src/pdf.rs) exposes page transforms, but
the glyph-extraction callback lives only in an experimental
[example](../../examples/text_geometry_probe.rs), not the reader API. Dragging
over the bitmap at either 100% or 125% cannot yield text or populate the
clipboard. This is the concrete obstacle to the requested **interaction** at
two zoom levels; geometric projections below are *not* an interaction test.

The existing [`PageViewport`](../../src/coordinates.rs) maps PDF document
points through crop/rotation, logical zoom, page origin, and scroll, and
[`pdf` unit tests](../../src/pdf.rs) cover a synthetic rotated/cropped
round-trip at 150% zoom. However, the reader does not yet connect the actual
GPUI page element's measured origin and scroll offsets to this transform for
pointer hit testing. Pixel density affects the raster request, not logical
page coordinates; the [`raster_scale`](../../src/reader.rs) and page size in
the reader use distinct scales. The first ordinary sample glyph `K` is at
page-local origin `(427.7, 61.9)` points, projecting to approximately
`(427.7, 61.9)` at 100% and `(534.6, 77.4)` at 125%, **relative to the page
origin before scrolling**. These values are computed from probe positions,
not measured pointer targets. At either zoom, a real hit test still needs
page-origin and scroll measurement, text advances/whitespace bounds, and
selection overlay coordinates independent of raster resolution.

## Text evidence and limits

| Case | Observation | Limit |
| --- | --- | --- |
| Bundled Typst text PDF, page 1 | Hayro's probe returned **351 glyphs, 351 decoded, 351 in crop**. First stream-order glyphs spell `KVER / READER SAMPLE`; `pdftotext -layout` also shows that heading and the page/body paragraphs. The first `K` has document origin `(427.7, 730.1)` and page origin `(427.7, 61.9)`. | This is an ASCII, mostly linear page. Stream-order preview and an independent extractor agreeing on its heading do **not** prove reading order for columns, ligatures, bidirectional text, combining characters, or a whole-page selection. |
| Rotated, cropped [py-pdf specimen](https://github.com/py-pdf/sample-files/tree/02aa148d618becd943a2a6bd78581de300fd75bc/027-cropped-rotated-scaled), page 2 | SHA-256 `cc195eac510b81123de4f55ac9f8185f2120975a7b75a106460b98615737aacd`. `pdfinfo -box` reports MediaBox `[9,9,459,549]`, CropBox `[99,81,387,459]`, rotation 90°. Probe: **192 glyphs, 176 decoded, 80 decoded overlapping crop**. Its first visible `t` maps from document `(112.5, 436.5)` to page `(355.5, 13.5)`, followed by `top left` vertically down the rotated rendered page. Off-crop text also appears in raw interpretation and must not be copied as visible text. | A crop-box intersection is only a candidate: arbitrary clipping, occlusion, and annotation appearances are not resolved. [Earlier rendering validation](../rendering-validation.md#release-mode-scan-and-rotatedcropped-follow-up-linux-cloud-runner) found misplaced annotation appearances on this exact stress fixture. `pdftotext -layout` emits duplicate corner labels and checkmarks; it is not a gold-standard reading-order oracle. |
| Image-only [pdf.js specimen](https://github.com/mozilla/pdf.js/blob/c85e80cb42e365ae1e9cd6c10253b9ae0d6dbf71/test/pdfs/images_1bit_grayscale.pdf), page 1 | SHA-256 `afbed07b9fe25563b2604b81cd7186c91d4356e5428a05322b8b7eae88301e24`. Probe: **0 glyphs, 0 decoded**; `pdftotext` prints nothing. | Without a text layer there is nothing to select/copy; do not silently copy guessed text or promise OCR. |

The [Hayro `Device` API](https://docs.rs/hayro-interpret/0.8.0/hayro_interpret/trait.Device.html)
provides positioned glyph runs, not ready-made word/line order or a clipboard
operation. The [probe](../../examples/text_geometry_probe.rs) skips unmapped
glyphs and has no outline box for spaces or Type 3 glyphs. The rotated fixture
already has 16 unmapped glyphs; this is a real Unicode **coverage** gap.
The ordinary page's decoded count and heading establish ASCII mapping only:
non-ASCII Unicode fidelity, normalization, and logical copy order remain
untested. Do not treat a rendered glyph as necessarily copyable, or concatenate
raw callback order as a verified clipboard string.

## Recommendation and reproduction

Leave #15 as a **nonblocking investigation**. If selection is later promoted,
scope an ordinary-text, text-layer-only slice: collect visible text geometry
off the UI thread behind the PDF abstraction, preserve document-space anchors,
measure the GPUI page origin and scroll to test pointer-to-glyph mapping at
100% and 125% in a real window, draw a transient overlay, and test actual
clipboard contents/order including spaces and non-ASCII text. Explicitly
report no selectable text on scans; test cropped/rotated pages without copying
off-crop content. Keep OCR, complex reading order, and annotation fidelity
separate decisions. Compare a backend alternative only for a demonstrated
unacceptable gap, not merely because glyph extraction is incomplete.

Commands run on macOS from this branch:

```sh
cargo +stable test --locked --no-default-features --example text_geometry_probe
cargo +stable test --locked --no-default-features --lib
cargo +stable run --locked --no-default-features --example text_geometry_probe -- examples/readme-sample.pdf 1
cargo +stable run --locked --no-default-features --example text_geometry_probe -- target/selection-investigation/cropped-rotated-scaled.pdf 2
cargo +stable run --locked --no-default-features --example text_geometry_probe -- target/selection-investigation/images_1bit_grayscale.pdf 1
```

Both probe tests and all ten library tests passed. To reproduce the outside
fixtures, download from the pinned source links above and check the SHA-256
values before probing. No native drag/copy test, screenshot, clipboard check,
or cross-platform selection test was run; there is no selection code to test.
