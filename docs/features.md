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

## Action-driven reader experience — next gate

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

These are the next defined capability gate, not prerequisites for the first
download. The [reader-experience spec (#33)](https://github.com/JonGretar/Kver/issues/33)
defines its acceptance checks; its child issues track implementation. This
gate does not promise a particular release version or date.

## Goals without a fixed release date

### Visual ruler

A movable reading guide should follow text lines where geometry is usable and
move by position on scans. Its position should survive zoom and scrolling.
Keyboard controls and an overlay independent of page rasterization keep
movement responsive. The ruler is wanted but is **not an MVP gate** or a
promise for the immediately following release.

### More navigation and desktop controls

- Search document text and navigate transiently highlighted results; follow
  internal links and display the PDF outline when present.
- Navigate backward and forward and restore the last reading position.
  Search-result highlights are not saved annotations.
- Configurable keybindings and preferences.
- Vim-style sequences, numeric counts, and named marks.
- Recent-file search, tabs, and drag-and-drop.
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
