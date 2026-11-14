# Future direction

Kver prioritizes responsive PDF reading, keyboard navigation, and a small,
maintainable desktop application. macOS leads; Linux, Windows, and Intel
macOS downloads need their own native testing and packaging.

These are directions, not a release schedule or implementation checklist.
Kver takes inspiration from Sioyek without aiming for feature parity or
requiring an academic workflow.

## High priority: visual ruler

A keyboard-controlled reading guide that highlights the current line and
moves to the next or previous line in reading order, including in
multi-column documents. It should make it easy to keep your place and
resume reading after an interruption.

The ruler should also provide a keyboard target for future reference
actions: preview or follow a reference on the current line without reaching
for the mouse. The reading guide is useful on its own; reference recognition
need not arrive with it.

## Priority: composable keyboard navigation and splits

Borrow Vim's composable navigation without adopting its entire modal editing
model. Exact keybindings remain configurable; examples below illustrate the
intended behavior, not a finalized default keymap.

- **Counts (numeric prefixes):** apply a quantity to a command, such as
  `10J` to advance ten pages or `5j` to move five steps. Define count behavior
  consistently for supported commands, including line movement with the ruler.
- **Keyboard text selection:** start a selection, extend it by word or line,
  then copy or highlight it without using the mouse. Build on the ruler where
  useful. Do something similar to sioyek's visual selection.
- **Follow Links:** Pressing `f` or `shift-F`(new window) will allow clicking 
  links and such things with keyboard only motion. Follow examples of ui from 
  sioyek, vimlike, vimium.
- **Independent splits:** view the same PDF at two locations, or two different
  PDFs, with independent navigation and keyboard focus switching. Start here;
  pinned reference panes and linked scrolling are possible later extensions.
- **Viewport positioning:** place the current reading target at the top,
  centre, or bottom of the view, following Vim's `zt`, `zz`, and `zb` idea.
  Apply this to the ruler, search results, and marks.


## Read references without losing context

Useful for research papers and textbooks, but also manuals and other
cross-reference-heavy documents:

- **Smart reference navigation:** recognize references to figures, tables,
  equations, and bibliography entries even when the PDF contains no links.
  For example, follow “Figure 2.3” to its caption or “[12]” to its bibliography
  entry. Recognition is best-effort, not a guarantee for every document.
- **Quick previews (overviews):** inspect a linked or recognized destination
  without leaving the reading position. The same idea can support previewing
  search results before jumping.
- **Persistent reference views (portals):** associate a reading location with
  another location and show that destination alongside the text when
  returning to it. Keep a figure, definition, or table in view while reading
  its explanation, rather than repeatedly jumping back and forth.

These are distinct capabilities: previews can use existing PDF links before
smart recognition exists, and manually created portals need not depend on
recognition either.

## Other directions

- **Reading and navigation:** independent named bookmarks and link hints.
- **Source-aware copying:** copy a passage with the document title and page
  label, with an optional Markdown quotation format.
- **Portable reading state:** export and import marks, bookmarks, and future
  portals without requiring a cloud service.
- **Authoring:** notes and area markup beyond text markup; a persistent
  markup-tool mode and richer annotation browsing.
- **Typesetting:** SyncTeX forward/inverse navigation with LaTeX editors and
  a reload workflow suited to repeated PDF builds.
- **Layouts:** snapshots and two-page/presentation layouts.
- **PDF Dark Mode:** Zathura/gpui-pdf supports making the PDF itself be dark mode
- **Form Support:** Allow filling out PDF Forms(Zathura/gpui-pdf do it)
- **CLI Tool:** Better support opening files from the cli
- **Open Links:** Download and open links.
- **Library integration:** investigate Zotero without assuming its database
  annotations are embedded in the PDF. No integration API is selected.
- **Desktop delivery:** signed/notarized macOS distribution and separately
  validated delivery for the other target platforms.

Other retained ideas include drag-and-drop, password-protected PDFs,
annotation palette/style controls, and an optional offer to save a newly
annotated original to a separate file.

## Sioyek-inspired ideas to consider

These are candidates for discussion, not additional product commitments:

- **External reference lookup:** recognize a paper title in a bibliography
  and offer to search for it in Google Scholar or another configured service.
  This is separate from following a citation within the current PDF.
- **Cross-document retrieval:** search bookmarks and highlights across
  previously read documents, rather than first remembering which PDF held
  the passage.
- **More targeted search:** search within a chapter or page range, with
  optional regular expressions.
- **Generated contents:** infer a navigable outline when a PDF has no
  embedded table of contents.
- **Content-aware fit:** fit the text area rather than the white page margins.

See [Sioyek's usage guide](https://sioyek-documentation.readthedocs.io/en/latest/usage.html)
for its ruler, SmartJump, overviews, portals, and external-search behavior,
and its [feature overview](https://sioyek.info/) for generated contents.

Other Vim-inspired ideas to investigate, not commitments:

- **Repeat last action:** replay the last meaningful action, like Vim's `.`.
  First identify useful reading workflows and define the target of a repeated
  action, especially for annotation edits. This is separate from counts.
- **Macros:** record and replay command sequences if concrete workflows
  justify the added interaction and configuration complexity.

## Scope and tracking

Consult [GitHub Issues](https://github.com/JonGretar/Kver/issues) for agreed
scope, priorities, ownership, and blockers. Check the code before treating an
idea here as missing functionality. Change this page when product direction
changes, not when a ticket lands; shipped changes belong in the changelog.

PDF content editing, OCR, digital signatures, cloud document libraries, 
and collaboration are a later luxury to research when we have nothing better 
to do.
