# PDF highlighting: format, reader behavior, and Kver options

## Summary

PDF has a standard, editable text-highlight annotation. Supporting it does not
require changing the page's text. Readers nevertheless differ in where they
store annotations: inside the PDF, in application data, or in a sidecar. An
application-local highlight and a portable PDF highlight can coexist, but
export/import is real work, not a property of drawing a colored overlay.

For Kver, creating persistent highlights from a selection is a bounded feature;
reliable PDF interchange adds a separate writer and compatibility boundary.
The agreed feature now includes four text-markup types; see
[spec #123](https://github.com/JonGretar/Kver/issues/123) for authoritative scope
and implementation tickets. This report retains the supporting format research,
alternatives, and highlight-specific feasibility findings.

## How highlights work in PDF

- A page's `Annots` array references annotation dictionaries. A text highlight
  uses `/Subtype /Highlight`, a bounding `/Rect`, and `/QuadPoints`: eight
  numbers per quadrilateral, in default PDF user space. A highlight can contain
  multiple quadrilaterals, so a wrapped or multi-column selection need not
  become one enormous rectangle covering intervening whitespace. Annotation
  dictionaries belong to one page; a cross-page selection needs multiple
  page annotations, even if an app presents them as one operation. [1]
- Geometry is not a stable text anchor. The standard does not require
  character offsets, a selected-text quotation, or a rule for finding that
  quotation again after a rebuild. `/Contents` can hold a note or description;
  `/NM` supplies a page-unique annotation name; `/T` identifies its author.
  Color, opacity, flags, and dates are also available. Keeping quoted text and
  context separately is useful for lists and future recovery, but automatic
  reattachment would be additional application behavior. [1]
- An appearance stream (`/AP`) describes how the annotation is drawn. PDF 1.7
  gives it precedence over `QuadPoints`; PDF 2.0 requires appearances for
  ordinary, non-degenerate highlight annotations. An export should generate
  both usable geometry and an appearance, not depend on every viewer making
  the same fallback picture. [1][2]
- Interoperability needs specimens, not just literal specification compliance.
  PDF.js explicitly normalizes differing real-world vertex orders and uses
  Multiply blending for highlights. PyMuPDF also recommends quadrilaterals
  for text markup, preserving orientation rather than reducing everything
  to axis-aligned rectangles. These are implementation precedents, not a
  guarantee that every viewer paints identical highlights. [3][4]
- Embedded annotations remain separate from page content and can be edited.
  Flattening makes the appearance part of the page instead; Apple explicitly
  distinguishes editable Save/Export from Print → Save as PDF flattening.
  Highlighting is not redaction and must not be presented as concealing or
  deleting the underlying text. [1][5]
- PDF supports incremental saves that append changed objects without rewriting
  the original bytes. This is not a blanket guarantee of safe signature
  preservation: certification permissions and allowed changes also matter.
  In-place writing needs explicit policies for signed/encrypted documents,
  concurrent external edits, write failures, and read-only files. [1]

An image-only scan has no selectable text in Kver. A freehand/area highlighter
could mark an image without OCR, but it is a different interaction from
text-aligned highlighting and should be scoped separately.

## What other readers do

| Reader | Creation and management | Storage and portability |
| --- | --- | --- |
| Apple Preview | Select text and apply a style, or enable a persistent highlight mode. Remove through the context menu; browse Highlights and Notes in the sidebar. | Save/Export preserves editable PDF annotations; printing to PDF flattens them. [5][6] |
| Adobe Acrobat | Selection quick actions create highlights; selecting a highlight offers Add Note, Change Color, and Delete. A comments pane provides document-wide management. | Uses PDF annotation/comment objects; the commenting UI extends well beyond simple highlights. Do not infer Kver needs the whole review system. [1][7] |
| Zotero | Selection popup for occasional highlighting, or a locked tool for repeated highlighting. Annotation sidebar and note extraction integrate highlights with research. | Own annotations live in its database. Embedded annotations are read-only by default; explicit import transfers them into Zotero and removes them from the PDF to avoid duplicates. Export embeds annotations into a copy. [8][9] |
| Sioyek | Select, then `h` and a type letter; use a current type or automatic selection-highlighting mode. Search document/all-document highlights, navigate by type, and delete from a list or the page. | Application databases plus JSON import/export; `embed_annotations` exports a PDF with annotations visible in other readers. Documented shortcuts are examples, not recommendations for Kver's current keymap. [10] |
| Skim | Treats highlights as part of its notes system. | Normally stores notes in extended attributes, not PDF data. Offers `.skim` sidecars, PDF bundles, and PDF export with embedded notes. Its manual warns extended attributes can be lost through email/archive workflows. [11] |

The recurring interaction is **selection → create → revisit/edit/delete**.
Repeated-highlighting mode is common but optional. A document-wide list is
useful independently of whether the app also supports comments or notes.
Separately stored annotations are established practice, but those readers
provide explicit portability mechanisms; local storage alone is not equivalent.

## Kver-specific observations

Code observations use this research worktree's `4ad2622` snapshot, not a claim
that it contains every subsequent main-branch change.

- [Selection](../../src/reader/selection.rs#L19) already owns a page-local range
  and [derives selected text and overlay rectangles](../../src/reader/selection.rs#L119-L194).
  Its [drag is limited to the starting page](../../src/reader/selection.rs#L100-L113).
  Persistent highlights should capture document geometry, not persist these
  transient glyph indices or zoomed overlay rectangles. Multi-page highlighting
  would require additional selection work.
- [PageTransform](../../src/coordinates.rs#L15-L46) supplies document/page
  conversion. Preserve the distinction between default PDF coordinates and
  crop/rotation-adjusted display coordinates when exporting.
- The pinned Hayro 0.8.0 stack provides rendering and a reading-oriented PDF
  object model, not a ready-made annotation editing/saving API. Kver's
  [PDF adapter](../../src/pdf.rs#L532-L558) currently reads and renders files.
  A PDF writer can be a separate internal component; changing the renderer is
  not a prerequisite. `lopdf` is one Rust manipulation-library candidate, not
  a selected application dependency; its limited feasibility evaluation is
  described below. [12][13]
- Kver already records an
  [annotation-appearance fidelity limitation](../rendering-validation.md#L371-L396).
  Displaying imported highlights, drawing Kver-owned overlays, and writing
  portable highlights are three different capabilities. A compatibility spike
  must test existing `/AP` streams and avoid rendering imported highlights
  twice; no general annotation fidelity is established by this research.
- [Reading-history deletion](../../src/reading_history/store.rs#L237-L260)
  deletes the document parent, cascading to its child records. Attaching
  authored highlights there would inherit those deletion semantics.
  Recommendation: do not silently make highlights disposable when recording
  is off or history is cleared. Any contrary product choice needs explicit UI
  language and confirmation.

## Storage choices and consequences

| Choice | Benefit | Cost or limitation |
| --- | --- | --- |
| PDF is authoritative | Highlights travel with the file and other readers can edit them. | Writer, appearance generation, permissions/signatures, file-change conflict handling, atomic saving, and reload/invalidation enter the first feature. |
| Kver data is authoritative; export an annotated copy | Original stays unchanged; read-only PDFs remain usable; fits responsive app-managed editing. | Need document identity/backup policy and explicit export. Import/edit rules must prevent duplicates; moving or rebuilding PDFs can orphan geometry. |
| Sidecar is authoritative | Original stays unchanged and annotations can travel beside it. | Users must carry two files; name/identity matching, collisions, and atomic updates remain. A custom sidecar is not automatically interoperable. |

The chosen product direction is embedded PDF annotations: the PDF is
authoritative, and ordinary saving must make highlights available to other
readers without an export step. Kver may cache annotation data for display,
but a private database must not be the only durable copy. The save policy
still needs to cover failures, read-only files, concurrent modification, and
signatures. Store page/quad geometry and stable highlight identity; isolate
persistence from rendering.

The spec settles the remaining product choices: supported imported markup is
editable, explicit PDF saving is independent of reading-history controls, and
creation starts from page-local mouse selection. It defines the list, colors,
undo, authorship, explicit saving, and conflict policies; do not treat the
experimental writer as implementing those policies.

### Writer feasibility

The standalone [writer spike](../../examples/highlight-spike/README.md) exercises
`lopdf` 0.47.0 incremental updates and Hayro 0.8.0 without adding a writer
dependency to Kver. Standard highlights with explicit normal appearances can
be created, recolored, and removed while retaining existing page content,
annotation dictionaries, and prior file bytes. The fixture covers multiple
regions on a page, a nonzero crop origin, and 0/90/180/270-degree rotations;
the existing linked field guide supplies a non-synthetic preservation case.

Apple PDFKit can read, recolor, and save the resulting annotations; the Rust
spike can reopen and edit that native save. Hayro and Poppler render the
generated highlights. This supports a separate writer alongside Hayro, not
a renderer replacement, for the tested annotation form.

Preview's interactive recoloring can drop selected regions when supplied
approximate padded geometry, with either Rust- or PDFKit-generated appearances.
Tighter text-selection geometry survives the same operation. A direct PDFKit
color-property edit does not reproduce that interactive failure: API-level
color/count checks are not a substitute for checking visible regions after
cross-reader edits. This supports a separate Rust writer, but requires real
selection geometry rather than hand-written padding. It does not establish
that all padding is invalid PDF or explain Preview's private algorithm.

The [Hayro geometry probe](../../examples/highlight-spike/src/geometry.rs)
extracts real outline bounds and advances for the fixture's three text runs,
without PDFKit or hard-coded region coordinates. It produces both ink-bound
and reader-style selection-bound variants, consistently in document space
through all four page rotations. These pass the renderer and direct PDFKit
recolor checks. Both Hayro-derived variants also preserve all selected regions
and their visible size when right-click recolored in Preview, as confirmed in
the native window. This establishes a viable writer/geometry path for the
fixture without depending on Apple's text extraction. The probe does not
implement general text-run grouping, multi-column selection, or arbitrary-angle
quads.

Even successful Preview edits can remove the original annotation's `/NM`
identifier. External-file reload and any future Zotero bridge cannot assume
annotation names or PDF object numbers always survive another reader's save.
The spike can reopen the successful Preview save and recolor/delete its
highlights while preserving their stored quads and unrelated content. Its
note-text lookup identifies only the controlled fixture and is not a suitable
production identity strategy.

The feasibility result supports specifying PDF-backed highlighting with Hayro
plus a separate writer. It does not require replacing the renderer. The first
production slice must still connect actual reader selections to persisted quads,
provide safe saving/error feedback, and validate representative real PDFs.

The spike also does not prove arbitrary text orientation, actual
selection-to-quad conversion, safe in-place replacement, signatures/encryption,
external-edit conflict handling, or large-document performance. Incremental
deletion leaves the earlier annotation bytes in previous revisions; it is not
secure erasure.

## Later Zotero integration

Embedded highlights and Zotero integration are compatible, but the PDF and
Zotero's database are not the same annotation store. Zotero displays external
embedded annotations read-only by default. Its explicit import transfers them
to its database and removes them from the file. Conversely, Zotero-created
database annotations are not present in the ordinary PDF bytes. Opening a
Zotero attachment in Kver therefore cannot by itself expose all annotations
from Zotero's reader. [9]

Prefer Zotero's API over directly editing its SQLite tables. The local API
serves the local library while Zotero runs, can resolve an attachment to its
on-disk file, and requires user opt-in. Current documentation describes
authorized local writes in Zotero 10+, with instance-specific versions and
conflict preconditions. Older-client support and annotation-specific read/write
round trips need their own compatibility investigation; these docs do not
establish that an integration already works. [14]

A later integration needs attachment/library identity, coordinate mapping,
stable annotation-ID mapping, conflict and deletion handling, and a clear
ownership rule to prevent duplicate PDF and database annotations. A Zotero
plugin is another possible bridge, not evaluated here. No running Zotero
database or API was accessed.

## Primary sources

1. [ISO 32000-1:2008, Adobe-hosted PDF](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf):
   §§7.5.6, 12.5.2, 12.5.5, 12.5.6.2, 12.5.6.10 (Table 179), 12.8.
2. [PDF Association PDF 2.0 errata, §12.5](https://pdf-issues.pdfa.org/32000-2-2020/clause12.html):
   appearance dictionaries and rendering rules.
3. [Mozilla PDF.js annotation implementation](https://github.com/mozilla/pdf.js/blob/master/src/core/annotation.js):
   `getQuadPoints`, `HighlightAnnotation`, and appearance generation.
4. [PyMuPDF Quad documentation](https://pymupdf.readthedocs.io/en/latest/quad.html)
   and [Page annotation API](https://pymupdf.readthedocs.io/en/latest/page.html#Page.add_highlight_annot).
5. [Apple: Annotate a PDF](https://support.apple.com/guide/preview/annotate-a-pdf-prvw11580/mac).
6. [Apple: Highlight, underline, and strike out text](https://support.apple.com/guide/preview/highlight-underline-and-strike-out-text-prvw757d4d5b/mac).
7. [Adobe: Annotation and drawing markup tools](https://helpx.adobe.com/acrobat/using/commenting-pdfs.html).
8. [Zotero: PDF reader](https://www.zotero.org/support/pdf_reader).
9. [Zotero: Why annotations are stored in the database](https://www.zotero.org/support/kb/annotations_in_database).
10. [Sioyek usage](https://sioyek-documentation.readthedocs.io/en/latest/usage.html#highlights)
    and [commands](https://sioyek-documentation.readthedocs.io/en/latest/commands.html#embed-annotations).
11. Skim: [embedded notes](https://skim-app.sourceforge.io/manual/SkimHelp_42.html),
    [sidecars](https://skim-app.sourceforge.io/manual/SkimHelp_39.html), and
    [bundles](https://skim-app.sourceforge.io/manual/SkimHelp_38.html).
12. [Hayro 0.8.0](https://docs.rs/crate/hayro/0.8.0/source/README.md)
    and [hayro-syntax 0.8.0 Pdf API](https://docs.rs/crate/hayro-syntax/0.8.0/source/src/pdf.rs).
13. [lopdf upstream documentation](https://github.com/J-F-Liu/lopdf#readme).
14. [Zotero local API](https://www.zotero.org/support/dev/web_api/v3/local_api).
