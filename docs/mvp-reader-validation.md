# MVP reader validation (#17)

**Release decision: not yet ready for a download.** Release-mode headless Metal checks
on an Apple M4 (macOS 27.0.1, 2× display density) found a short-final-page
navigation failure, tracked as [#31](https://github.com/JonGretar/Kver/issues/31)
and blocking release [#18](https://github.com/JonGretar/Kver/issues/18).
The full release-candidate Mac-window check in #18 has **not** been completed.
The fix has passed headless checks and a targeted native-window recheck;
these results do not certify a downloadable app.

**Partial native-window report (Jón Grétar):** While opening and scrolling
`cropped-rotated-scaled.pdf` in either fit mode, the page flickered and moved
as zoom changed near the last page. A thin visible sliver of the preceding
page was counted as current. This is a reported real-Mac observation, not
an agent-measured frame trace; whether other documents reproduce it
remains unconfirmed. [#31](https://github.com/JonGretar/Kver/issues/31)
tracks the fit transition as well as the headless final-page failure.
After the first local fix, Jón Grétar reported that fit width and last-page
navigation were much better, but fit page still wavered between pages 1 and 2
of this PDF. After a second local change and rebuild, Jón Grétar reported
that the page 1–2 fit-page transition worked much better, with no glitches
seen while scrolling back and forth. This is a qualitative native-window
check of the #31 input change, **not** a frame-time measurement or a
release-candidate check of the rest of the workflow.

Download these public documents into ignored `target/validation-pdfs/`:

| Case | File, source and SHA-256 | Headless reader result |
| --- | --- | --- |
| Long | [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.pdf), `rfc9110.pdf`, 194 pages; `60b30efa1048900833d1758440247fe8ac85a3134f2327388dcb24e07d814c89` | Passed opening, first/middle/last captures, next/previous and wheel navigation, keyboard `=`/`-`/`shift-=`, `w`/`f` fit modes, invalid/missing-path errors, repeated jumps and scroll. |
| Ordinary text | [pdf.js `tracemonkey.pdf`](https://raw.githubusercontent.com/mozilla/pdf.js/c85e80cb42e365ae1e9cd6c10253b9ae0d6dbf71/test/pdfs/tracemonkey.pdf), 14 pages; `3662ff519e485810520552bf301d8c3b2b917fd2f83303f4965d7abed367e113` | Same reader checks passed. |
| Scan-style pages with OCR | [pypdf `vicksburg.pdf`](https://raw.githubusercontent.com/py-pdf/pypdf/945216fb51835a534ee6712bab34fe24fa91e11e/resources/Seige_of_Vicksburg_Sample_OCR.pdf), 6 pages; `e8b9d461f035f8ce2440f9061d35b2164148eb3511a487aacfc26a94cb9d90fe` | Same reader checks passed. OCR-layered printed-page images are not a photographic-scan stress test. |
| Rotated and offset-cropped | [py-pdf `cropped-rotated-scaled.pdf`](https://raw.githubusercontent.com/py-pdf/sample-files/main/027-cropped-rotated-scaled/cropped-rotated-scaled.pdf), 4 pages; `cc195eac510b81123de4f55ac9f8185f2120975a7b75a106460b98615737aacd` | **Before the local fix:** failed last-page jump at 125%: expected page 4/4, got 3/4 with bitmap 3 current. **After the local fix:** the full headless reader flow passed, including navigation to 4/4, keyboard zoom and fit modes; sampled bitmap use peaked at 7,392,000 bytes. Annotation/form appearance has the separate accepted [Phase 0 limitation](rendering-validation.md#release-mode-scan-and-rotatedcropped-follow-up-linux-cloud-runner). Native page 1–2 fit-page scrolling was qualitatively rechecked; the rest of the manual checklist remains. |
| One image-only page | [CC0 scan-style specimen](https://filedone.ai/sample-files/sample-scanned-page.pdf), `sample-scanned.pdf`; `79f6bf12b046ac134a11fd35758d72d39715b4ff97a2153216233413aed4b0e3` | Backend-only Hayro render at 1×/2×/3×: 595×842, 1190×1684, 1786×2526 pixels; 45/147/402 ms read + parse + raster + PNG encode in release mode. **Not** a reader navigation or native-window check; synthetic scan style, no OCR layer. |

Reproduce the behavior test for each multi-page file (replace `rfc9110` with
the other filename stem). The checked-in test leaves PNG captures under
ignored `target/captures/`, including first, middle, last, scrolled and error
views. The last-page assertion on the four-page specimen was red before the
local #31 change and is now green:

```sh
KVER_TEST_PDF=target/validation-pdfs/rfc9110.pdf \
KVER_CAPTURE_DIR=target/captures/issue-17-rfc9110 \
  cargo +stable test --locked --release --features screenshot --test visual
```

Downscaled headless captures of the RFC last page, `tracemonkey` middle page
and `vicksburg` middle page showed the expected page numbers, text, and
illustrations; the cropped/rotated middle capture showed multiple differently
oriented page regions with detached annotation shapes. This was a visual
spot-check of captures, not a pixel-level comparison against another renderer
or a native-window inspection.

The #31 regression also uses a generated three-page PDF with a short final
page and different page widths. It verifies both fit modes: scrolling to
the end selects the last page even with a sliver of the preceding page in
view; an explicit previous/next turn returns to the final page; and repeated
headless captures while scrolling back and forth do not change the fit scale
or current page without another scroll input. This constrains the feedback
loop but cannot establish that a visible window never flickers.

A second generated three-page case approximates the cropped document's page
sizes (384×504, 378×288 and 352×294 points). Before the second change,
forward-only 20-point scroll steps in fit-page mode selected page 2 at step
17 (2.349×), then incorrectly returned to page 1 at step 18 (1.365×).
This was reproducible in release-mode headless Metal. The fit change had
preserved the *top* document point while choosing the fit target at the
viewport center; the resulting layout moved that center back across the
page boundary. Now a fit transition caused by scrolling preserves the
center document point. The regression verifies nondecreasing page numbers
while scrolling forward, nonincreasing numbers when scrolling backward,
and a stable page/scale over repeated captures without input. Explicit
page turns, manual zoom and switching fit modes continue to preserve their
existing top-of-viewport reading position.

For the three passing reader specimens, the highest *sampled retained bitmap
bytes* during page jumps and scroll events were 12,502,696 (RFC), 12,117,600
(text) and 6,998,400 (OCR scan), below the reader's 128 MiB bitmap budget.
These are sampled UI-owned pixel buffers, **not peak process memory or an
exhaustive peak during background jobs**. On one RFC test process,
`/usr/bin/time -l` reported 363,282,432 bytes maximum resident set size and
400,393,128 bytes peak memory footprint; it includes the headless renderer,
PDF work, and PNG screenshots. This one run does not prove bounded resident
memory over sustained native use. The 194-page test jumps to each page, then
sends 388 scroll commands; no growing retained-bitmap count was observed at
the sampled points. Warm alternating turns at pages 97–98 of the RFC took
6.4–7.0 ms to accept and 10.6–11.5 ms through screenshot; pages 3–4 of the
OCR scan took 17.3–19.4 ms and 21.0–24.1 ms respectively (eight turns each,
100% zoom, 2× density). Headless capture timing does not measure native
key-to-present latency or human-perceived responsiveness.

The headless test also checked the empty reader, a cancelled headless picker,
opening/replacing a PDF by path, recovery after a missing or invalid PDF,
and that replacement clears the old bitmap. A native file chooser is **not**
exercised by this test. On this Mac `sh scripts/build-macos-app.sh` built
`target/release/Kver.app` with an arm64 Mach-O, but building a bundle is not
evidence that its visible window works.

## Remaining release-window interaction check (#18)

On an Apple silicon Mac with a display, launch the locally built app without
arguments with `open -n -a "$PWD/target/release/Kver.app"`; do not infer
success from `open` returning alone. In the visible window:

1. Use **Open PDF** to choose the RFC and confirm the page content appears.
   Open an invalid `.pdf` and a valid PDF again; record the visible error and
   whether the window stays usable. Try the ordinary-text and scan-style PDFs.
2. Use `n`/`p` or right/left repeatedly, and wheel-scroll across pages. Try
   `=`, `shift-=`, `-`, `w` (fit width), and `f` (fit page), including after
   resizing the window. Confirm that the displayed page, label and reading
   position agree, and that input remains responsive while pages render.
3. Open the four-page rotated/cropped specimen and try moving to page 4 from
   page 3; record the label, image and whether page 4 is reachable by key.
   Some annotation appearances are known to be misplaced on this fixture.
4. Record Mac model, macOS version, display density, documents/pages checked,
   any pauses and approximate process-memory behavior after repeated scrolling.
   Report observations separately from headless results. Repeat any pre-fix
   observations against the landed candidate before using them as release evidence.
