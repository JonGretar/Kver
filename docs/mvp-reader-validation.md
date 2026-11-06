# MVP reader validation (#17)

**Release decision: v0.1.0 is published and its download has been checked.**
Release-mode headless Metal checks on an Apple M4
(macOS 27.0.1, 2× display density) found a short-final-page navigation failure,
tracked and fixed in [#31](https://github.com/JonGretar/Kver/issues/31).
The fix passed headless checks and a targeted native-window recheck. The
subsequent local-candidate and downloaded-release checks for
[#18](https://github.com/JonGretar/Kver/issues/18) are recorded below.

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

These are historical results from a fuller exploratory capture probe, not
the current default capture flow. The last-page assertion on the four-page
specimen was red before the local #31 change and green afterward. The current
`tests/visual.rs` captures only the first page by default; to inspect another
document on macOS with Corgi installed, run:

```sh
KVER_TEST_PDF=target/validation-pdfs/rfc9110.pdf \
KVER_CAPTURE_DIR=target/captures/issue-17-rfc9110 \
  corgi test --features screenshot --test visual
```

For middle/last-page, scrolling, or error captures, temporarily add those
interactions to the probe and restore its baseline when finished. On Linux,
use `cargo +stable test --locked --features screenshot --test visual` instead.

Downscaled headless captures of the RFC last page, `tracemonkey` middle page
and `vicksburg` middle page showed the expected page numbers, text, and
illustrations; the cropped/rotated middle capture showed multiple differently
oriented page regions with detached annotation shapes. This was a visual
spot-check of captures, not a pixel-level comparison against another renderer
or a native-window inspection.

The #31 exploratory regression also used a generated three-page PDF with a short final
page and different page widths. It verified both fit modes: scrolling to
the end selects the last page even with a sliver of the preceding page in
view; an explicit previous/next turn returns to the final page; and repeated
headless captures while scrolling back and forth do not change the fit scale
or current page without another scroll input. This constrained the feedback
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

The historical headless test also checked the empty reader, a cancelled headless picker,
opening/replacing a PDF by path, recovery after a missing or invalid PDF,
and that replacement clears the old bitmap. A native file chooser is **not**
exercised by this test. On this Mac `sh scripts/build-macos-app.sh` built
`target/release/Kver.app` with an arm64 Mach-O, but building a bundle is not
evidence that its visible window works.

## Local release-candidate window check (#18)

On 2026-11-01, `sh scripts/package-macos-app.sh` built and inspected the
local `Kver-v0.1.0-macos-arm64.zip` on an Apple M4 (Mac16,1,
macOS 27.0.1). The extracted archive contained version 0.1.0, an arm64
executable and the app icon. The release metadata validator accepted
`v0.1.0`; `cargo +stable test --locked --all-targets --features screenshot`,
clippy and fmt passed on this Mac. The local archive SHA-256 was
`d3b0382b676a33ac40af60c5f48986845689b9839f2852dfec87e7511dc7452a`.

The agent launched the **locally built** bundle without a PDF path using
`open -n -a "$PWD/target/release/Kver.app"` and confirmed the app process
started. Jón Grétar reviewed the visible window and reported that the
requested checks worked as expected: opening an ordinary PDF through
**Open PDF**, page navigation, scrolling, zoom, fit modes and resizing,
with no reported responsiveness issue. This is a user-reported interaction
check, not an automated measurement. The document name, page count, invalid
PDF handling, sustained memory, and a fresh native check of the rotated
fixture were not recorded during this candidate review; the earlier
representative-PDF and #31 evidence above remains separate.

## v0.1.0 downloaded release check (#18)

After explicit authorization, the `v0.1.0` tag at commit `a5d920f9` triggered
the [macOS release job](https://github.com/JonGretar/Kver/actions/runs/41294400153),
which passed and published the
[GitHub Release](https://github.com/JonGretar/Kver/releases/tag/v0.1.0).
Its only asset is `Kver-v0.1.0-macos-arm64.zip`; no experimental Linux
download is offered. A separate GitHub CLI download of that asset had
SHA-256 `fd7f43877b55c00e27af488410a57421395b7e360a5c3bab656e62483884deb4`,
matching GitHub's asset digest. The extracted app had both bundle version
fields set to 0.1.0, an arm64 executable and the icon. The agent launched
this downloaded copy without an argument and observed a running process.

Jón Grétar reported that the downloaded app worked in the visible Mac
window and the download looked correct. When asked about a browser-downloaded
copy, Jón Grétar confirmed a macOS security warning and reported that the
behavior was as expected. The exact prompt text and override route were
not recorded. This is a user-observed real-window check, not an automated
test of picker or input handling, and the CLI download did not itself
carry a quarantine attribute. The
[unsigned-app security instructions](macos-app.md#download-an-unsigned-release)
explain safe ways to open a trusted copy without disabling Gatekeeper.

For future releases, download the archive on an Apple silicon Mac. Inspect
the extracted version, architecture and icon, and record any Gatekeeper
prompt or override used. Launch without arguments and check the visible
window; `open` returning alone is not evidence of working input. In the window:

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
   Report observations separately from headless and local-candidate results.
   Repeat any pre-fix observations against the downloaded app before using
   them as release evidence.
