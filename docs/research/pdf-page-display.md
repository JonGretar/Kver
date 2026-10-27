# PDF page display: bitmap versus SVG

## Findings

- A PDF page contains drawing instructions, text, images, and other resources.
  Viewers generally **rasterize at the target viewport/density** and display
  the resulting pixels; they do not need to convert each page into SVG.
  PDF.js's [rendering example](https://mozilla.github.io/pdf.js/examples/)
  requests a page and renders it to a sized HTML canvas, accounting for device
  pixel ratio. MuPDF's [Page API](https://mupdf.readthedocs.io/en/latest/reference/javascript/types/Page.html)
  offers `toPixmap`; PDFium's
  [rendering API](https://pdfium.googlesource.com/pdfium/+/main/public/fpdfview.h)
  includes bitmap rendering. These examples establish common supported paths,
  not a claim that *every* PDF viewer uses the same pipeline.
- Sioyek's [renderer](https://github.com/ahrm/sioyek/blob/development/pdf_viewer/pdf_renderer.cpp)
  caches `fz_pixmap` render responses and uploads their samples as OpenGL
  textures (`find_rendered_page`, around lines 140–200). Its
  [renderer header](https://github.com/ahrm/sioyek/blob/development/pdf_viewer/pdf_renderer.h)
  sets `num_cached_pages = 5` and includes workers and pending requests.
- PDF.js's [viewer source](https://github.com/mozilla/pdf.js/blob/master/web/pdf_viewer.js)
  uses a bounded `PDFPageViewBuffer` (default 10 at the time of reading),
  resizes it using visible-page counts, and invokes
  `renderHighestPriority` for visible pages (around lines 75, 160, 1990–2008).
  Its [render task API](https://mozilla.github.io/pdf.js/api/draft/module-pdfjsLib-RenderTask.html)
  exposes cancellation. MuPDF's
  [display lists](https://mupdf.readthedocs.io/en/latest/reference/javascript/types/DisplayList.html)
  can replay a page at different resolutions without reinterpreting the file.

## Implications for Kver

Hayro already produces pixels; passing them directly to GPUI is normal and
avoids the old PNG encode/decode round trip. SVG conversion would add a second
representation without addressing blank transitions or bitmap render latency.
Keep the last completed page visible while replacing it, then investigate
bounded adjacent-page caching/prefetch if transitions remain noticeable.
Memory cost matters: a 1224 × 1584 BGRA page is roughly 7.8 MB before
GPU copies. Keep request prioritization and stale-result checks independent
of any cache.
