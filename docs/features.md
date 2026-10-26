# Features

Kver is a general-purpose PDF reader for people who read long or varied
documents. Keyboard access matters, but mouse interaction should remain
natural. The [roadmap](roadmap.md) defines release gates; this page identifies
which capabilities belong to each gate.

## macOS MVP — first downloadable preview

- Open local PDFs; show understandable failures.
- Display pages with continuous vertical scrolling, page navigation, zoom,
  fit width, and fit page.
- Render visible pages asynchronously with bounded memory use so long
  documents remain responsive.
- Provide a predefined set of keyboard controls for scrolling, pages, and
  zoom, as well as ordinary mouse navigation.
- Distribute a buildable, downloadable unsigned macOS app. Signing and
  notarization can follow later.

Test with text, scanned, long, and rotated/cropped PDFs. Password-protected
PDFs may be included if straightforward, but do not block this release.
The MVP does not promise OCR.

## Daily-reader milestone

- Search document text; move between and highlight results.
- Select and copy text with the mouse.
- Follow internal links; display a PDF outline when present.
- Navigate backward and forward.
- Restore the previous reading position.

These are the next defined capability gate, not prerequisites for the first
download. Search-result highlighting is transient; it does not create an
annotation.

## Goals without a fixed release date

### Visual ruler

A movable reading guide should follow text lines where geometry is usable and
move by position on scans. Its position should survive zoom and scrolling.
Keyboard controls and an overlay independent of page rasterization keep
movement responsive. The ruler is wanted but is **not an MVP gate** or a
promise for the immediately following release.

### More navigation and desktop controls

- Configurable keybindings and preferences.
- Vim-style sequences, numeric counts, named marks, and a command palette.
- Recent documents, multiple-document switching, tabs or windows, and
  drag-and-drop.
- Password-protected PDFs if deferred from MVP.
- Link hints and keyboard-driven text selection.

### Authoring and study

- Reload PDFs after rebuilds; SyncTeX forward/inverse navigation with LaTeX
  source files.
- Notes, persistent text highlights, bookmarks, and annotation export. Decide
  how annotations are stored and shared before promising PDF interoperability.
- Split views, snapshots, overviews, and two-page or presentation layouts.
- Smart jumps to references, bibliography navigation, and portals inspired by
  Sioyek.

These capabilities can be promoted independently when they have a clear
use case and an implementation path. They do not define a mandatory order.

## Other platforms

Linux and Windows are targets with separate delivery gates. Evaluate their
feasibility early, but do not hold the first macOS download until their
packages are ready.

## Out of scope for the initial reader

Creating or editing PDF contents, OCR, PDF form editing, digital signatures,
cloud document libraries, and collaborative editing are not initial-release
commitments.
