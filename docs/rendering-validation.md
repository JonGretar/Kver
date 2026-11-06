# Phase 0 rendering validation

Hayro is the confirmed initial backend for the prototype. Known PDF issues
will be evaluated as they arise rather than blocking that choice.
The [upstream project](https://github.com/LaurenzV/hayro#development-state)
reports broad rendering compatibility, but also calls out robustness,
performance, and API-surface work. Its current README is not a test result
for Kver's pinned Hayro 0.8.0 or for positioned-text workflows. Validate Hayro
on real documents first; compare PDFium at the **same output pixel dimensions**
when a significant gap appears. Keep results tied to specific documents and
pages; the bundled README PDF is only a smoke test.
The [Phase 0 gate](roadmap.md#0-validate-the-stack) also requires UI
responsiveness, text geometry, coordinate transforms, and platform risks.

## Capture a Hayro baseline

On macOS, run this from the repository root (no GPUI or Metal toolchain needed):

```sh
cargo +stable run --no-default-features --example render_validation -- \
  examples/readme-sample.pdf /tmp/kver-render-validation 1
```

The last argument is a **1-based page number**. The command renders that
page at 1, 2, and 3 image pixels per PDF point and prints a tab-separated row
per scale: backend, page, scale, image width, image height, elapsed milliseconds,
and PNG path. It writes `hayro-page-N-1x.png` through `3x.png` to the chosen
directory, replacing files with those names on a repeat run. Use a different
directory for each document. The elapsed time includes file reading, parsing,
rasterization, and PNG encoding for each fresh document load; it excludes
writing the PNG. The operating system may cache file reads between scales.
Width and height are the bitmap's integer pixel dimensions (Hayro truncates
fractional dimensions). Debug-build timings are diagnostic, not a backend
benchmark.

Inspect the PNGs at their **native pixel size** and in the GPUI window at
each zoom level (`cargo +stable run --release -- path/to/document.pdf`). The preview
keeps its display size in PDF points × zoom, but requests raster pixels at that
size × the window's display density. It retains the previous image, scaled
to the new logical size, until background rendering completes. Moving the
window to a display with a different scale factor triggers a fresh render.
Bitmap safety limits can cap the raster scale at extreme zoom levels; in
that case the UI still shows the requested logical zoom. A previous 1x
preview on a 2x-density screen could look soft. A window-only macOS capture
of the README sample at 100% now shows legible, sharp-looking text and
correct page layout at 2064×1888 window pixels. This is **one static view**:
real-document display quality, zoom transitions, and input responsiveness
still need visual testing. The current UI navigates one page at a time.

To repeat or extend the **interactive macOS check**:

1. Run `cargo +stable run --release -- examples/readme-sample.pdf` on a Mac display
   and compare the text with a familiar PDF viewer. The `−` and
   `+` buttons should change displayed size immediately and then replace
   the scaled preview with a newly rendered image.
2. Open an image-heavy or scan-style PDF and click zoom several times during
   the background render. The window should continue accepting input, keep
   the last image visible, and display the **latest** zoom result rather
   than a stale one. Note any visible pause and the display's scale factor.
3. If two monitors have different pixel densities, move the window between
   them and check that it remains the same logical size but becomes sharp
   again after its background rerender. The page must not disappear.

In an initial hands-on macOS check, the reader opened the CC0 scan-style
specimen listed below, zoom changes appeared almost instant, and the window
seemed responsive. The observer could not click the opposite zoom button
before a render finished, so overlapping-request behavior remains untested.
They also reported that zoom *appeared* to stop near 200%, while the status
label continued up to 400% and the image seemed to move rather than enlarge.
The preview now prevents its image from flex-shrinking and allows its container
to grow wider than the viewport. In a follow-up hands-on check with the
scan-style PDF, the observer confirmed that the image visibly zooms beyond
200%. Horizontal scrolling at high zoom and display-density changes remain
unchecked.

### Current release-window smoke check (macOS)

On an Apple M4 Mac running macOS 27.0.1, `cargo +stable build --locked
--release --bin kver` completed, then `target/release/kver
examples/readme-sample.pdf` opened a native 920×832-point window on the built-in
2× Retina display. A window-only 1840×1664-pixel capture showed page 1 of
100, the correct page label and legible text at 100% zoom. A later capture
showed page 83 with its matching label, but this was not a controlled
key-to-present measurement. In a follow-up, the user ran
`cargo +stable run --locked --release -- /tmp/kver-scan-validation.pdf`
(the SHA-256-matched CC0 scan-style specimen below) and reported that
everything looked as it should to their eyes when asked to try rapid `+`/`−`
zoom and, if convenient, the two displays. This is a qualitative
native-window acceptance check for basic visible zoom and responsiveness,
**not** a measured latency result. The report does not establish whether
opposing requests overlapped an active render or whether the window was
moved between monitors; neither was explicitly confirmed. Comparison to a
familiar viewer on a real document, horizontal scrolling at high zoom,
mixed-density window moves, and stress under sustained page changes are
still follow-ups, not claims established by this check.

The release-mode Metal headless scenario also passed with the six-page
OCR-layered `vicksburg.pdf` from the follow-up below:
`KVER_TEST_PDF=target/validation-pdfs/vicksburg.pdf
KVER_CAPTURE_DIR=target/captures/vicksburg-release cargo +stable test --locked
--release --features screenshot --test visual`. It exercised page navigation,
zoom, and error reporting with the real-PDF specimen. The captures remain
under ignored `target/`; headless Metal does not prove the native window
accepts input promptly during a slow render.

## Page-turn latency: debug versus release

The historical opt-in main-thread GPUI probe opened a PDF, rendered its
middle page, then alternated eight times between that page and the next. It
times from the `next_page`/`previous_page` call to acceptance of the new image
and to a headless Metal screenshot. The probe checks that successive captured
page regions differ. It exercises a **2× display density** at 100% zoom; set
`KVER_BENCH_ZOOM_IN=1` for 125% zoom (2.5× raster scale). These are warm,
repeated renders of two pages; Kver does not yet cache or prefetch them. The
probe did not measure native key dispatch, onscreen frame presentation, cold
start, or Skim/Sioyek on the same machine. The current `tests/visual.rs` is
a first-page capture baseline; it no longer includes this timing probe.

On this macOS machine (10 logical CPUs, 24 GB RAM), at 920×800 logical
pixels, elapsed time per turn was:

| PDF / zoom | Debug accepted / captured | Release accepted / captured |
| --- | --- | --- |
| Bundled 100-page sample, pages 50–51, 100% | 168–171 / 283–287 ms | 3–4 / 7–9 ms |
| pdf.js `tracemonkey.pdf`, pages 7–8, 100% | 910–1021 / 1028–1141 ms | 10–11 / 14–15 ms |
| pdf.js `tracemonkey.pdf`, pages 7–8, 125% | 1106–1242 / 1223–1360 ms | 12–13 / 16–17 ms |

The first duration includes a roughly 1 ms status-poll interval; the second
includes headless Metal capture/readback rather than an onscreen present.
Timings vary by PDF content and machine. The **build profile dominated this
check**: do not use `cargo run` without `--release` to decide whether prefetch
is required.

The public pdf.js corpus file used for this measurement was at revision
`c85e80cb42e365ae1e9cd6c10253b9ae0d6dbf71` (SHA-256
`3662ff519e485810520552bf301d8c3b2b917fd2f83303f4965d7abed367e113`).
To repeat the measurement, implement the page-turn timing experiment in the
opt-in capture tool and restore the baseline afterward. Compare the same PDF,
window, density, zoom, and build profile before inferring a backend speed
difference.

### Long-document and loading-error check

The release-mode visual test also opened the [194-page RFC 9110 PDF](https://www.rfc-editor.org/rfc/rfc9110.pdf)
(2.7 MB, SHA-256 `60b30efa1048900833d1758440247fe8ac85a3134f2327388dcb24e07d814c89`).
It captured pages 1, 97, and 194 at 2× display density, zoomed to 125%, and
queued page changes without waiting for intermediate results. The displayed
page and status matched the latest request; the first/middle/last captures
showed different, legible content. Eight release-mode turns between pages
97–98 took 6–7 ms to accept the image and 10–12 ms through headless capture.
This does **not** prove native-window responsiveness under sustained input.
The file remains under ignored `target/perf-pages/`, not in the repository.

The current capture baseline accepts `KVER_TEST_PDF` and `KVER_CAPTURE_DIR`
but captures only its first page; these historical middle/last-page and
error scenarios require temporary probe changes.

The historical visual test also opened a missing path and a deliberately invalid `.pdf`
under ignored `target/captures/`. The UI shows `Cannot read ...` for the
missing file and `Cannot open PDF: invalid or unsupported PDF file` for the
invalid one. `target/captures/invalid-pdf.png` captures the latter. Hayro's
`Invalid` result does not distinguish a damaged PDF from an unsupported one.
In a real macOS release window, the observer confirmed that the invalid file
was handled gracefully and that page turns in the 194-page RFC felt fast.
This is qualitative feedback, not a native key-to-present measurement or a
scan-heavy PDF check.

**Next decision:** This page-at-a-time navigation slice no longer needs
prefetch to be usable. Validate scan-heavy documents separately; if release
mode later shows a noticeable page-turn gap against another reader on the
same file, measure native key-to-present time first. Any future adjacent-page
cache needs a fixed memory budget, latest-page priority, zoom/display-density
invalidation, and obsolete-work handling; do not pre-render the whole PDF.

## Coordinate evidence so far

The library tests create a two-page PDF in memory with a nonzero media-box
origin and a smaller, offset crop box. One page has a 90° rotation; the other
is unrotated. They verify rendered dimensions at 2x, known crop-corner
positions, and round trips between PDF document points (y up) and rendered
page points (y down). `src/coordinates.rs` keeps this transform independent
of Hayro; `src/pdf.rs` obtains its coefficients from the current backend.
This is a controlled geometry check, **not** evidence that real-world PDFs
render correctly or that screen/scroll/zoom transforms are finished.

## Positioned text: initial Hayro probe

Run the separate, experimental text probe on a page with selectable text:

```sh
cargo +stable run --no-default-features --example text_geometry_probe -- \
  examples/readme-sample.pdf 1
```

Hayro 0.8 exposes [positioned glyph runs through `Device`](https://docs.rs/hayro-interpret/0.8.0/hayro_interpret/trait.Device.html).
Each [positioned glyph](https://docs.rs/hayro-interpret/0.8.0/hayro_interpret/font/struct.PositionedGlyph.html)
has a text-space transform and an optional Unicode mapping (which Hayro
describes as experimental). The probe interprets the page without a display
transform and reports glyph origins and outline bounds in PDF document
coordinates. On the original hand-built PDF it saw **970 glyphs, all with Unicode
mappings**; that fixture has since been replaced by the
[Typst sample](../examples/reader-sample.typ). Re-run the probe for current
font and text-mapping evidence. Its space characters had positions but no
outline bounds. This has an automated example test.

This demonstrates *access to glyph-level positions*, not a complete search
or selection text model. The probe does not group lines, measure
space advances, compute bounds for Type 3 fonts, or resolve missing Unicode.
It now reports both document-space and rendered-page coordinates, and
separates glyphs whose outline boxes intersect the visible crop from those
outside it. An automated test checks that hidden text is excluded without
changing its document coordinates. This is a *crop candidate*, not a
visibility guarantee: other clipping paths, occlusion, and Type 3 glyphs
need separate handling. The probe stays outside the reader API.

For comparison, [`pdfium-render` 0.9.4 exposes character Unicode, bounds, and
origins](https://docs.rs/pdfium-render/0.9.4/pdfium_render/prelude/struct.PdfPageTextChar.html).
That is an API comparison, **not** a measured result from running PDFium in
Kver. Hayro remains a promising smaller-distribution candidate; neither
packaged size nor correctness across representative PDFs has been measured.

## Documents and observations to collect

Use documents you can inspect locally; do not commit private PDFs. Record
their provenance and whether redistribution is permitted before adding any
test fixture to the repo.

| Case | Inspect |
| --- | --- |
| Text | Small fonts, thin rules, embedded fonts, transparency, sharpness at native pixels and in the window |
| Scan | Image decoding, crop edges, missing text layer, memory and rendering time |
| Long document | Early, middle, and late pages; total page count, opening latency, repeated zoom/input behavior |
| Rotated/cropped | Orientation, crop boundaries, nonzero page origins, position of known text or landmarks |

### Public-PDF spot check

I used PDFs from the [pdf.js regression corpus](https://github.com/mozilla/pdf.js/tree/c85e80cb42e365ae1e9cd6c10253b9ae0d6dbf71/test/pdfs)
at revision `c85e80cb42e365ae1e9cd6c10253b9ae0d6dbf71`. To reproduce
without committing third-party fixtures, download the listed filenames from
`https://raw.githubusercontent.com/mozilla/pdf.js/c85e80cb42e365ae1e9cd6c10253b9ae0d6dbf71/test/pdfs/`
into a temporary directory, then run the commands above on the indicated
pages. This was a macOS Apple M4, debug build, using Poppler 26.04.0
(`pdfinfo`, `pdftoppm`, `pdftotext`) as an independent visual reference—not
PDFium. Timings below include a fresh load, rasterization, and PNG encoding
and are **single runs, not benchmarks**.

| PDF | Pages checked | Hayro observations |
| --- | --- | --- |
| `tracemonkey.pdf` (14 pages) | 1, 7, 14 | All rendered at 1x/2x/3x; page 7's text, two columns, and figures appeared aligned with a Poppler 1x render. At 1x, page 7 took 756 ms; at 3x, 2,758 ms. The text probe decoded all 4,282 glyphs it saw on page 1, 4,612 on page 7, and 2,709 on page 14. |
| `hello_world_rotated.pdf` (5 pages, 90° page rotation) | 1, 5 | Rendered as 792×612 at 1x; page 1 visually matched the Poppler orientation and text placement. Text probe decoded 11/11 glyphs on both pages, with document-space positions. |
| `images_1bit_grayscale.pdf` (one image-only page) | 1 | Decoded 0 text glyphs and rendered image samples visually similar to Poppler. Hayro's 1x bitmap was 595×841; Poppler rounded the fractional A4 page up to 596×842. This does **not** establish photographic-scan compatibility. |

### Crop, simulated scan, and 500-page check

For crop handling I used Chromium's
[`hello_world_cropped.pdf`](https://chromium.googlesource.com/chromium/src/+/main/pdf/test/data/hello_world_cropped.pdf)
(SHA-256 `9274819fdc0ef1c597d209e2f04ba1ca17f5b1b679b4e9a6d98a99e575f6f877`)
and PDFium's
[`cropped_text.pdf`](https://pdfium.googlesource.com/pdfium/+/main/testing/resources/cropped_text.pdf)
(SHA-256 `1e2a6b5f13c77d8c0611ac7fcde88cd28352c7c0eb8a36bc593e2d91ef3c4ccc`).
Their crop boxes differ from their media boxes; `cropped_text.pdf` also has a
page with a negative media-box origin. All five pages rendered at 1x/2x/3x.
At 1x the visible text and boundaries looked aligned with Poppler using
`pdftoppm -cropbox` (without this flag Poppler instead renders the media box).
The revised probe found 28 Unicode-mapped glyphs on each cropped page: 18
overlap the crop on `cropped_text.pdf` pages 1–2, and 7 on
`hello_world_cropped.pdf`. Its first in-crop glyph in stream order on
`cropped_text.pdf` page 1 begins at document x≈52.7, rendered-page x≈2.7,
matching the offset crop box [50, 50, 150, 150]. On
`hello_world_cropped.pdf`, a second-line “y” barely overlaps the crop
(about 0.4 PDF points); the criterion preserves partly cropped glyphs.
For a rotated pdf.js test page, a glyph at document (210, 370) maps to
rendered-page (370, 210) and all 11 glyphs overlap the page.
**Raw extraction includes hidden text**; crop filtering is needed before
search or selection, and clipping against arbitrary paths remains untested.
No claim is made that the rasterizer displayed the off-crop glyphs.
These are small regression PDFs, not a rotated-and-cropped real-world page.

The [CC0 scanned-page specimen](https://filedone.ai/sample-files/sample-scanned-pdf)
(SHA-256 `79f6bf12b046ac134a11fd35758d72d39715b4ff97a2153216233413aed4b0e3`)
has a skewed, grainy page image and no PDF text layer (`pdftotext` returned no
printable text; the probe found 0 glyphs). Hayro rendered it at 1x/2x/3x;
the 1x page looked aligned with Poppler. Its 1x and 3x debug requests took
821 ms and 4,409 ms, respectively. This is a **synthetically produced
scan-style PDF**, not a physical scan, and the OS cache may affect timings.

The [CC0 500-page specimen](https://pdftoolskit.org/sample-pdfs)
(SHA-256 `cf43d5389c3c04984275d9dd3b7b7745d684e77e9bad00a1b917646045a4de30`)
rendered pages 1, 250, and 500 at 1x/2x/3x. The 1x images visibly carried
the correct page numbers. Requests took about 264–268 ms at 1x and
1,071–1,080 ms at 3x. Its repetitive, machine-generated pages do not test
memory use during sustained scrolling, and the GPUI window showed only page
one at the time of this spot check. The subsequent
continuous-scroll reader renders visible pages throughout the document.

No clear missing content appeared in the inspected 1x images. This is a
small qualitative check, not pixel equality or proof that complete documents
are correct. The 1-pixel size difference on fractional pages comes from
Hayro's simplified rendering API; Kver now reports the bitmap's actual size
instead of stretching it. `scan-bad.pdf` was also rendered but was mostly
blank with a small text layer, so it is not counted as a scanned-page check.

For each document record: filename or non-sensitive identifier, page number,
page boxes and rotation (if known), scale, output dimensions, visible artifacts,
timing, machine/display density, and whether input remains responsive while
rendering. Re-run representative pages to separate warm-cache effects from
fresh requests. When comparing PDFium, use the same pages, output pixel
dimensions, and inspection conditions. A failed render or geometry extraction
is evidence too.

**At the time of this spot check:** The rendering command only covered Hayro
rasterization. The release-mode follow-up below adds a targeted PDFium
comparison, but still does not measure UI input latency, line grouping and
reading order, document-to-screen transforms (including zoom, scrolling, and
display density), or Linux/Windows packaging. Follow-up work should test
text bounds and full forward/inverse screen coordinates on a real
rotated-and-cropped document. No OCR is required for scans.

## Release-mode scan and rotated/cropped follow-up (Linux Cloud Runner)

This is a **backend-only** check on a headless Linux x86-64 Cloud Runner
(`cargo +stable` 1.99.1, Hayro 0.8.0 via `Cargo.lock`); it is not the
interactive macOS check above. The release binaries were built with
`cargo +stable build --locked --release --no-default-features --examples`.
Each `render_validation` invocation reopened the PDF at each scale and timed
read + parse + raster + PNG encoding, excluding PNG disk write. The table
reports a first run followed by a repeat in the same session for representative
pages; these are not cold-cache benchmarks or key-to-present times. Inspection
used the saved PNGs at their native pixel dimensions. Independent visual
references were PyMuPDF 1.26.7 (MuPDF) and, **after a visible disagreement**,
`pypdfium2` 5.6.0 (PDFium 147.0.7713.0). These Python tools were installed
only under ignored `target/validation-tools/`, not added to Kver.

Reproduce without committing the third-party PDFs:

```sh
mkdir -p target/validation-pdfs
curl -fLsS -o target/validation-pdfs/sample-scanned.pdf \
  https://filedone.ai/sample-files/sample-scanned-page.pdf
curl -fLsS -o target/validation-pdfs/vicksburg.pdf \
  https://raw.githubusercontent.com/py-pdf/pypdf/945216fb51835a534ee6712bab34fe24fa91e11e/resources/Seige_of_Vicksburg_Sample_OCR.pdf
curl -fLsS -o target/validation-pdfs/cropped-rotated-scaled.pdf \
  https://raw.githubusercontent.com/py-pdf/sample-files/02b8203bf416db3dbdb03438bc8278dfcb0555f3/027-cropped-rotated-scaled/cropped-rotated-scaled.pdf
curl -fLsS -o target/validation-pdfs/font_ascent_descent.pdf \
  https://raw.githubusercontent.com/mozilla/pdf.js/c85e80cb42e365ae1e9cd6c10253b9ae0d6dbf71/test/pdfs/font_ascent_descent.pdf
sha256sum target/validation-pdfs/*.pdf
cargo +stable build --locked --release --no-default-features --examples
target/release/examples/render_validation target/validation-pdfs/vicksburg.pdf \
  target/validation-pdfs/output/vicksburg 4
target/release/examples/text_geometry_probe target/validation-pdfs/vicksburg.pdf 4
```

On macOS, replace `sha256sum` with `shasum -a 256`.
Repeat the last two commands with each PDF and page number below. The filedone
specimen is CC0 and artificially scan-styled, not an actual archival scan.
The six-page *Siege of Vicksburg* is an OCR-layered reproduction of printed
pages from the pypdf test resources (image objects plus text), not proof of
performance on large photographic scans. The four-page py-pdf
`cropped-rotated-scaled.pdf` is a deliberately **complex generated**
annotation/crop/rotation test (the source README says CC-BY-SA-4.0), not an
ordinary production document. The pdf.js `font_ascent_descent.pdf` is an
actual presentation page with `/Rotate 90` but no offset crop. SHA-256 values
in the table identify exactly the downloaded files. No PDF fixtures were
committed.

| PDF (SHA-256) / page; MediaBox, CropBox, `/Rotate` | Hayro 1× / 2× / 3× output pixels; first-run elapsed ms (repeat where measured) | Native-size visual and geometry findings |
| --- | --- | --- |
| `sample-scanned.pdf` (`79f6bf12b046ac134a11fd35758d72d39715b4ff97a2153216233413aed4b0e3`), p1; `[0,0,595.44,842.04]`, same, 0° | 595×842 / 1190×1684 / 1786×2526; 70/234/391 (repeat 70/231/392) | Grain, skew, table lines and page edges present and aligned with MuPDF at 1×; MuPDF rounds to 596×843. Hayro and PDFium find zero text characters. No OCR is expected. |
| `vicksburg.pdf` (`e8b9d461f035f8ce2440f9061d35b2164148eb3511a487aacfc26a94cb9d90fe`), p1, p4, p6; `[0,0,432,648]`, same, 0° | All: 432×648 / 864×1296 / 1296×1944. p1: 58/74/137; p4: 67/92/125 (repeat 49/75/134); p6: 53/75/144 | Inspected early/middle/late pages at 1× and p4 at 2×: headers, folios (49, 53, 55), body and scan-style print detail present and aligned with MuPDF; no missing blocks observed. Hayro decoded 2101/2101, 2340/2340, 1996/1996 positioned glyphs respectively, all in crop. On p4 the first header glyph has document origin (163.0, 598.8), page origin (163.0, 49.2) and document outline x≈163.1–168.2, y≈598.8–604.8; the rendered header is near the top at y≈43–49. This is a plausible text-overlay position, not an OCR accuracy or line-grouping test. |
| `cropped-rotated-scaled.pdf` (`cc195eac510b81123de4f55ac9f8185f2120975a7b75a106460b98615737aacd`), p1; `[12,12,612,732]`, `[132,108,516,612]`, 0° | 384×504 / 768×1008 / 1152×1512; 6/10/15 | Four corner labels and crop boundary visible. Hayro probe: 192 glyphs, 176 mapped, 80 mapped glyphs intersect crop (same counts on p2–4). Decorative/form glyphs without Unicode and off-crop glyphs need separate handling. |
| Same PDF, p2; `[9,9,459,549]`, `[99,81,387,459]`, 90° | 378×288 / 756×576 / 1134×864; 5/7/10 (repeat 5/7/11) | **Significant visual discrepancy at 2×:** Hayro places yellow shapes and red triangular marks detached in the blank page interior and draws extra form labels/marks over corner content; PDFium and MuPDF place the four corner groups together without the detached shapes. PDFium was rendered at exactly 756×576 with `draw_annots=True` and also with `False`; with annotations disabled PDFium omits the extra annotation content entirely. PDFium and MuPDF themselves differ on some form/annotation appearances, so do not attribute every difference to the page-content rasterizer. Hayro's first in-crop `t` has document origin (112.5,436.5) and page origin (355.5,13.5), near the top-right label at 2× (711,27); the base text mapping is plausible despite misplaced annotation appearances. |
| Same PDF, p3; `[11,7,561,427]`, `[121,63,473,357]`, 180° | 352×294 / 704×588 / 1056×882; 5/6/9 | Upside-down corner labels and expected crop present; Hayro again draws detached yellow/red shapes in white space at 2×, unlike PDFium's annotations-on render of the same size. |
| Same PDF, p4; `[11,13,561,793]`, `[121,117,473,663]`, 270° | 546×352 / 1092×704 / 1638×1056; 5/9/14 | Corner content has the expected orientation, but Hayro places two large yellow marks in the white interior at 2×; PDFium keeps them with the corner groups. |
| `font_ascent_descent.pdf` (`ac32f419e728d1fa96daa24172871f8d38cf7ad6a7f3076a2f86b76a6811517e`), p1; `[0,0,595,842]`, same, 90° | 842×595 / 1684×1190 / 2526×1785; 5/12/23 | The orange Odfjell heading and black subtitle appear upright at 1× and match MuPDF's placement. Hayro reports 104 glyphs but **0 Unicode mappings**. PDFium and MuPDF extract control characters rather than readable text on this file as well; this is not evidence that PDFium fixes its text layer. |

The rotated/cropped file's probe preserves PDF document coordinates even when
text is outside the crop. For example, p1's first visible `t` is at
document (150,582), page (18,30); p2's first visible `t` is at
(112.5,436.5), page (355.5,13.5). Those placements agree with the visible
corner labels. This checks the backend's crop/rotation transform for selected
glyphs, **not** screen-space placement after scrolling/zoom or accurate
visibility behind arbitrary clips/annotations. The 16 unmapped glyphs on each
complex specimen page and the real presentation's unmapped text mean text-based
features cannot assume Unicode is always available.

**Phase 0 backend decision:** Hayro 0.8.0 is the chosen initial backend.
This pass found it viable for ordinary text, these scan-style pages, and base rotated
text/crop coordinates in release mode. Do **not** call annotation-heavy
rotated/cropped pages validated: PDFium comparison confirms a material
appearance-placement gap on this complex specimen (and has its own form
differences from MuPDF). **Accepted limitation for now:** mispositioned
annotation/form appearances on this stress fixture do not block continuing
with Hayro as the initial backend. Defer investigation and a fix or explicit
annotation-support policy until annotation fidelity is in scope, or revisit
sooner if ordinary reading documents show misplaced content. This is not a
claim that all annotations or real rotated-and-cropped documents render
correctly; test an intact real document with both rotation and offset crop
when broad fidelity is evaluated. No backend change was made. This Cloud
Runner pass could not establish interactive input
responsiveness, macOS display quality, native key-to-present latency, memory
under sustained scrolling, or a representative sample of photographic scans.

The follow-up [MVP reader validation](mvp-reader-validation.md) checks current
behavior on representative PDFs. It does not change the Phase 0 backend
decision or erase its annotation-appearance limitation.

## Platform feasibility and current build limits

- **macOS (before the `gpui-pre` migration):** GPUI 0.2.2 uses Metal. After
  installing the Metal Toolchain component on this macOS host,
  `cargo +stable check`, `cargo +stable test --all-targets`,
  `cargo +stable clippy --all-targets -- -D warnings`, and a binary build pass.
  The sample app opened an on-screen window. After screen
  capture permission was granted, a window-only capture showed the sample
  PDF laid out clearly at 100% on a high-density display. An initial
  hands-on scan-style PDF check found fast zoom and apparent responsiveness;
  the corrected layout was confirmed to visibly zoom beyond 200%. Concurrent
  renders, horizontal scrolling at high zoom, and real-document display
  quality remain unverified. The preview now requests application quit when
  its last window closes; this close-to-exit behavior has not been retested
  interactively. With `gpui-pre` 0.3.10 on a local Apple Silicon Mac, the
  normal GUI binary builds and stays running after launch without reporting
  an open-window error; the sample PDF window was confirmed visible on that
  Mac. All 15 tests pass with the `screenshot` feature, and the headless
  scenario captures 1840 × 1600 before/after frames using Metal. Clippy and
  formatting checks pass.
- **Linux:** Kver now uses `gpui-pre` 0.3.10 and the matching platform crate.
  On a headless Debian Cloud Runner, a debug binary with the `screenshot`
  feature built and captured the sample PDF UI at 1840 × 1600 using the
  software Vulkan renderer. This validates off-screen rendering, not an
  interactive X11/Wayland window or Linux packaging. The previous `gpui`
  0.2.2 build was blocked in a transitive `xattr` dependency.
- **Windows (before the `gpui-pre` migration):** The previous GPUI source has a
  [Windows platform path](https://docs.rs/gpui/0.2.2/src/gpui/platform.rs.html#154-161).
  No Windows target is installed on this macOS host, and no Windows build
  or packaging test has been run. Treat support as plausible, not proven.

Hayro avoids distributing a separate native PDFium library. On this Apple
Silicon Mac, `cargo +stable build --release --bin kver` produced a
**14,500,112-byte** Mach-O executable; zipping that executable alone with
`ditto -c -k --sequesterRsrc --keepParent` produced a **5,577,231-byte**
archive. `otool -L` listed macOS system frameworks and libraries, with no
PDFium dynamic library. This is a rough *binary* baseline, **not** an
application bundle or a tested downloadable artifact. App resources,
packaging, signing, and notarization have not been measured or performed.
The current host's default Rust is 1.89-nightly, below Hayro 0.8.0's Rust
1.92 minimum; the installed `+stable` (1.98) works for all current checks.
Neither an available platform implementation nor a source inspection
substitutes for a build and window test on each target.
