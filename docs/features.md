# Features

Kver is a general-purpose PDF reader for people who read long or varied
documents. Keyboard access matters, but mouse interaction should remain
natural. The [roadmap](roadmap.md) defines release gates; this page identifies
which capabilities belong to each gate.

## Apple silicon macOS MVP — first downloadable preview

- Open local PDFs; show understandable failures.
- Display pages with continuous vertical scrolling, page navigation, zoom,
  fit width, and fit page.
- Render visible pages asynchronously with bounded memory use so long
  documents remain responsive.
- Provide a predefined set of keyboard controls for scrolling, pages, and
  zoom, as well as ordinary mouse navigation.
- Distribute a buildable, downloadable unsigned Apple silicon macOS app through
  a versioned GitHub Release. Signing, notarization, and an Intel build can
  follow later.

Test with text, scanned, long, and rotated/cropped PDFs. Password-protected
PDFs may be included if straightforward, but do not block this release.
The MVP does not promise OCR.

## Action-driven reader experience — completed gate

- Refresh the reader UI with GPUI Kit components; settle its exact appearance
  during implementation rather than prescribing a layout here.
- Make everyday reader operations share GPUI Actions, accessible through
  controls, useful keyboard shortcuts, an application menu, and a searchable
  command palette for app actions.
- Open an independent empty reader window and switch between windows; each
  window keeps its own document and reading state, without tabs.
- Select visible text with the mouse and copy it to the system clipboard on
  ordinary text PDFs. Keep selection aligned through zoom and scrolling,
  exclude off-crop text, and do not imply OCR on image-only scans.

These capabilities were validated together in
[#42](https://github.com/JonGretar/Kver/issues/42), following the
[reader-experience spec (#33)](https://github.com/JonGretar/Kver/issues/33).
Selection remains page-local, uses available PDF text mappings, and does not
include OCR or guaranteed reading order for complex layouts. Find and Contents
belong to the next gate; history and the goals below remain deferred. This
completed gate does not imply another release version or date.

## Find and Contents — planned reader gate

- Find mapped text across the current PDF, navigate matches, and show short
  result snippets and transient highlights on the page. Scans without a text
  layer have no searchable text; this does not include OCR.
- Browse the PDF's table of contents when it provides an outline, and jump to
  its destinations. Show an honest empty state when no usable outline exists.
  Command-T opens Contents; Kver uses independent windows rather than document
  tabs.
- Give each reader window one sidebar hosting distinct Find and Contents
  panels. Show only one panel at a time by default; keep the sidebar
  dismissible and the PDF readable.

The [spec (#54)](https://github.com/JonGretar/Kver/issues/54) defines the
scope and validation. This gate has no assigned release version or date.
Trackpad pinch-to-zoom may be investigated separately, but does not block it.

## Goals without a fixed release date

### Visual ruler

A movable reading guide should follow text lines where geometry is usable and
move by position on scans. Its position should survive zoom and scrolling.
Keyboard controls and an overlay independent of page rasterization keep
movement responsive. The ruler is wanted but is **not an MVP gate** or a
promise for the immediately following release.

### More navigation and desktop controls

- Follow internal links.
- Navigate backward and forward and restore the last reading position.
  Search-result highlights are not saved annotations.
- Configurable keybindings and preferences.
- Vim-style sequences, numeric counts, and named marks.
- Recent-file search and drag-and-drop.
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
