# Skim and source-to-PDF synchronization: design notes

Research note for future Kver discussions; no feature or backend decision is
made here. Sources are Skim's own help/wiki, the SyncTeX project's documentation,
CTAN, KDE's Okular documentation, and the PDF Association.

## Skim as a reader reference

Skim is a macOS PDF reader and note-taker oriented toward studying papers, with
notes, highlights, bookmarks, contents/thumbnails, full-screen presentations,
and LaTeX integration
([Skim manual](https://skim-app.sourceforge.io/manual/),
[feature list](https://sourceforge.net/p/skim-app/wiki/Features/)).

- **Reading bar:** Highlights the current line; keyboard commands move it by
  line or page and adjust its line span, and the user can drag it. It can
  instead mask the surrounding text
  ([reading bar](https://skim-app.sourceforge.io/manual/SkimHelp_30.html),
  [appearance](https://skim-app.sourceforge.io/manual/SkimHelp_31.html)).
  This is a closer analogue of Kver's core
  [visual ruler](../architecture.md#visual-ruler) than Skim's annotation tools.
- **Snapshots:** Keep a selected page region visible for reference without
  navigating away. Skim says snapshots are *not* saved with the PDF
  ([snapshot help](https://skim-app.sourceforge.io/manual/SkimHelp_23.html)).
  Compare with Sioyek's automatic destination
  [overviews/portals](sioyek-orientation.md#what-sioyek-actually-offers);
  this is inspiration for later reading workflows, not Phase 0 scope.
- **Notes are not necessarily PDF annotations on disk:** Skim normally stores
  its notes/highlights as filesystem extended attributes. It also offers a
  `.skim` sidecar, a `.pdfd` bundle, or export to a PDF with embedded notes.
  The extended-attribute approach has portability pitfalls when sharing files
  ([bundle](https://skim-app.sourceforge.io/manual/SkimHelp_38.html),
  [sidecar](https://skim-app.sourceforge.io/manual/SkimHelp_39.html),
  [embedded export](https://skim-app.sourceforge.io/manual/SkimHelp_42.html)).
  Kver should explicitly decide *later* what is app-local versus portable;
  merely drawing a highlight is not the same as writing one into a PDF.
- **Reload during typesetting:** Skim supports reloading changed PDFs but warns
  about file replacement, multi-pass builds, and unsaved notes; its wiki
  recommends an explicit editor/build-triggered reload instead of relying
  exclusively on watching the file
  ([Skim synchronization wiki](https://sourceforge.net/p/skim-app/wiki/TeX_and_PDF_Synchronization/)).
  Kver will eventually need to preserve or deliberately invalidate its view
  state and raster/text caches when the PDF changes.

## SyncTeX: what crosses the application boundary

SyncTeX is a **TeX input/output mapping**, produced at typesetting time as an
auxiliary file associated with the output PDF, often `name.synctex.gz`. It maps
source file/line to output page/geometry (forward search), and PDF page/point
back to a source location (inverse/backward search). It is not automatically
present in an arbitrary PDF and is not the PDF text layer
([SyncTeX parser README](https://tug.ctan.org/support/synctex-parser/README.md),
[TeX Live manual](https://tug.org/svn/texlive/trunk/Build/source/texk/web2c/synctexdir/man5/synctex.5?revision=46371&view=markup),
[Skim synchronization wiki](https://sourceforge.net/p/skim-app/wiki/TeX_and_PDF_Synchronization/)).

Skim documents enabling `-synctex=1`, Shift-Command-click for inverse search,
an editor command configured with source file/line placeholders, and its
`displayline` command for editor-to-PDF forward search. The source argument
matters for multi-file LaTeX projects
([Skim synchronization wiki](https://sourceforge.net/p/skim-app/wiki/TeX_and_PDF_Synchronization/)).
This illustrates two separate integration tasks: **resolve the mapping** and
**transport a jump between the viewer and editor**. Skim uses its own command
and editor configuration; Okular also configures editor commands with its own
placeholders. These examples do not establish a universal viewer/editor IPC
protocol ([Skim synchronization wiki](https://sourceforge.net/p/skim-app/wiki/TeX_and_PDF_Synchronization/),
[Okular editor settings](https://docs.kde.org/stable_kf6/en/okular/okular/configeditor.html)).

The SyncTeX manual recommends using the `synctex` CLI or `synctex_parser`
library rather than independently parsing its sidecar format
([TeX Live manual](https://tug.org/svn/texlive/trunk/Build/source/texk/web2c/synctexdir/man5/synctex.5?revision=46371&view=markup)).
A Kver prototype could start with the CLI as an optional adapter and only
evaluate a library/FFI integration if latency and packaging warrant it.
Neither approach has yet been tested here.

## Related formats and standards (different purposes)

| Mechanism | What it gives a PDF reader | Relation to SyncTeX |
| --- | --- | --- |
| **PDFSync** (`.pdfsync`) | Older TeX-source ↔ PDF-location sidecar produced by the `pdfsync` package; Skim supports it alongside SyncTeX ([CTAN](https://ctan.org/pkg/pdfsync), [Skim manual](https://skim-app.sourceforge.io/manual/SkimHelp_46.html)). | Closest legacy alternative; no need to assume it is the preferred first integration. |
| **DVI source specials** | Source-location information can be recorded natively in DVI output ([Okular editor settings](https://docs.kde.org/stable_kf6/en/okular/okular/configeditor.html)). | Source sync for another output format, not a PDF feature; Kver currently targets PDFs. |
| **PDF annotations and document structure** | PDF's core specification governs interoperable PDF files ([ISO 32000-2 overview](https://pdfa.org/resource/iso-32000-2/)); Tagged PDF can encode intended reading order and semantics ([PDF Association FAQ](https://pdfa.org/resource/tagged-pdf-q-a/)). | Useful for portable notes, accessibility, and possibly better text-line order; **not** a mapping back to TeX source. |
| **PDF/UA** | Accessibility standard relying on semantic Tagged PDF ([PDF Association PDF/UA-2 overview](https://pdfa.org/iso-14289-2-pdfua-2)). | Worth considering when evaluating real PDFs and text navigation, but not a source-sync protocol. |

## Suggested design boundary, not a milestone change

If source synchronization becomes a priority, expose a viewer-level action
`go_to_document_location` for forward search and an optional
`source_location_at_page_point` mapping for inverse search. Keep mapping lookup
and editor invocation outside the PDF raster/text backend: the mapping comes
from a compiler sidecar, not necessarily from the PDF engine. Convert incoming
page coordinates using the same tested document-space transformations used by
the [reader](../architecture.md#coordinate-systems). For inverse search, make
editor invocation configurable and treat paths from source-map files as
untrusted input rather than interpolating them into a shell command.

Open questions for a later prototype: how to locate a sidecar when TeX outputs
to a different build directory, how to handle stale maps after a rebuild, how
forward requests reach a running Kver instance on each platform, and whether
reload retains marks/ruler state when pagination changes. No hands-on Skim
evaluation or SyncTeX integration test was performed for this note.
