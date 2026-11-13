# Future direction

Kver prioritizes responsive PDF reading, keyboard navigation, and a small,
maintainable desktop application. macOS leads; Linux, Windows, and Intel
macOS downloads need their own native testing and packaging.

These are directions, not a release schedule or implementation checklist:

- **Reading and navigation:** a movable visual ruler, numeric command counts,
  independent named bookmarks, link hints, and keyboard text selection.
- **Authoring:** notes and area markup beyond text markup; a persistent
  markup-tool mode and richer annotation browsing.
- **Typesetting:** SyncTeX forward/inverse navigation with LaTeX editors and
  a reload workflow suited to repeated PDF builds.
- **Reference workflows:** split views, snapshots, two-page/presentation
  layouts, smart reference jumps, and Sioyek-style overviews or portals.
- **Library integration:** investigate Zotero without assuming its database
  annotations are embedded in the PDF. No integration API is selected.
- **Desktop delivery:** signed/notarized macOS distribution and separately
  validated delivery for the other target platforms.

Other retained ideas include drag-and-drop, password-protected PDFs,
annotation palette/style controls, and an optional offer to save a newly
annotated original to a separate file.

Consult [GitHub Issues](https://github.com/JonGretar/Kver/issues) for agreed
scope, priorities, ownership, and blockers. Check the code before treating an
idea here as missing functionality. Change this page when product direction
changes, not when a ticket lands; shipped changes belong in the changelog.

PDF content editing, OCR, form editing, digital signatures, cloud document
libraries, and collaboration are not current product commitments.
