# Phase 0 rendering validation

Hayro is the preferred initial backend if it meets the reader's needs.
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
each zoom level (`cargo +stable run -- path/to/document.pdf`). The preview
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
still need visual testing. The UI still renders only page one.

For the remaining **interactive macOS check**:

1. Run `cargo +stable run -- examples/readme-sample.pdf` on a Mac display
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
coordinates. On the bundled PDF it saw **970 glyphs, all with Unicode
mappings**. Its first “K” starts at document point (50, 750), matching the
[sample generator](../examples/generate_readme_pdf.py); its space characters
have positions but no outline bounds. This has an automated example test.

This demonstrates *access to glyph-level positions*, not a complete search,
selection, or visual-ruler text model. The probe does not group lines, measure
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
memory use during sustained scrolling, and the current GPUI window still
shows only page one.

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

**Not yet validated:** The rendering command only covers Hayro rasterization.
It does not measure UI input latency, PDFium, line grouping and reading order,
document-to-screen transforms (including zoom, scrolling, and display density),
or Linux/Windows packaging. Follow-up work should test text bounds and full
forward/inverse screen coordinates on a real rotated-and-cropped document;
add a PDFium probe if Hayro has a significant gap. No OCR is required for scans.

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
