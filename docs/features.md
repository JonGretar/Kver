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
are delivered in the following gate; reading history and preferences are
delivered in the persistence gate below. This completed gate does not imply
another release version or date.

## Find and Contents — completed reader gate

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
scope; [#58](https://github.com/JonGretar/Kver/issues/58) records the combined
validation and supported limitations. This gate has no assigned release
version or date. Trackpad pinch-to-zoom may be investigated separately.

## Persistence, Settings, and Single Page — completed reader gate

- Set app-wide appearance, initial zoom, and opening reading mode in Settings.
  Valid hand edits to TOML apply while Kver runs; invalid edits leave the last
  valid preferences active until corrected.
- Search Open Recent across stored PDFs. Resume at the last page-local place
  when enabled, without automatically restoring old reader windows.
- Read in Continuous or Single Page mode. Scrolling a single page never turns
  it; explicit navigation does. A PDF-specific mode can differ from the app
  default without changing another open reader's live mode.
- Set and revisit PDF-local `a`–`z` marks using keyboard sequences, the
  command palette, or a searchable popup. A mark can have an optional display
  name, but the letter remains its identity.
- Keep new per-PDF activity session-only when recording is off. Clearing
  history or forgetting one PDF removes its saved reading data without deleting
  PDF files. On macOS, closing the last window can leave Kver available via the
  Dock, menu, and Finder; explicit Quit always exits.

The [spec (#67)](https://github.com/JonGretar/Kver/issues/67) defines the
scope; [#77](https://github.com/JonGretar/Kver/issues/77) records combined
validation and supported limits. There is no assigned release version or date.

## Goals without a fixed release date

### Visual ruler

A movable reading guide should follow text lines where geometry is usable and
move by position on scans. Its position should survive zoom and scrolling.
Keyboard controls and an overlay independent of page rasterization keep
movement responsive. The ruler is wanted but is **not an MVP gate** or a
promise for the immediately following release.

### More navigation and desktop controls

- Follow internal links.
- Navigate backward and forward. Search-result highlights are not saved
  annotations.
- Configurable keybindings, numeric counts, and general Vim-style commands
  beyond the built-in local-mark sequences.
- Independent named bookmarks and drag-and-drop.
- Password-protected PDFs if deferred from MVP.
- Link hints and keyboard-driven text selection.

### Authoring and study

- Reload PDFs after rebuilds; SyncTeX forward/inverse navigation with LaTeX
  source files.
- Persistent highlights, underlines, strikethrough, and squiggly underlines,
  saved as standard annotations in the PDF without an export step. Start with
  mouse-driven, page-local text selection and keyboard-operable commands;
  keyboard visual selection comes later.
- A document-wide Annotations list for supported text markup, with navigation,
  recoloring, and deletion, including supported annotations from other readers.
  Preserve unrelated annotation types and existing metadata when editing.
- Explicit Save and Save As for annotation edits, with unsaved-state feedback
  and Save / Discard / Cancel before closing a modified document. Note
  authoring, area markup, and other annotation types are deferred.
- Future Zotero library/annotation integration, with no assigned milestone or
  date. Access to Zotero's annotation database and coordination with PDF-backed
  annotations need a separate spec; no API or direct-database approach is
  selected yet.
- Split views, snapshots, overviews, and two-page or presentation layouts.
- Smart jumps to references, bibliography navigation, and portals inspired by
  Sioyek.

These capabilities can be promoted independently when they have a clear
use case and an implementation path. They do not define a mandatory order.

#### Text-markup behavior

These are agreed requirements for the planned feature, not current app behavior.
The [text-markup spec (#123)](https://github.com/JonGretar/Kver/issues/123)
defines the complete scope, implementation boundaries, and testing decisions.

- Start with one-shot creation from the current selection through keyboard
  commands and a selection context menu. Keep the selection after creation;
  revisit a persistent markup-tool mode later.
- Offer five named preset colors, initially yellow for highlights and red for
  line-based markup. Remember the last-used color per type during the session.
  Preserve imported colors until explicitly changed. Custom palettes, opacity,
  and line-thickness controls are deferred.
- List supported markup in document order with copy text, navigation, recoloring,
  and deletion. Use a plain list of marked text with a type indicator; decide the
  GPUI Kit component and appearance during implementation. Keep entries whose
  text cannot be recovered. Search, filters, whole-document annotation export,
  and cross-document lists are deferred.
- Selecting markup directly on the page is essential, alongside list access.
  Disambiguate overlapping annotations rather than choosing an arbitrary target.
  Preserve normal text dragging. Type conversion is deferred.
- Support session undo/redo for creation, recoloring, and deletion, including
  after Save. Prevent exact same-type duplicates; allow different markup types
  on the same text and independent partially overlapping annotations. Merging,
  splitting, and boundary dragging are deferred.
- Reapplying the same markup type to an identical region selects the existing
  annotation without duplicating, recoloring, or toggling it off, even when the
  current creation color differs.
- Disable editing of signed or encrypted PDFs initially. Allow edits to ordinary
  read-only PDFs in memory, with Save As to a writable file. Retain unsaved edits
  on save failure; do not bypass permissions or silently remove signatures.
- Never silently overwrite an externally changed PDF. Retain local edits on a
  conflict and offer saving a separate version or discarding edits and reloading,
  without automatic merging. Reload clean documents while preserving the reading
  position where possible.
- Share annotation edits, undo/redo, and save state between windows displaying
  the same document. Keep reading positions and text selections window-local;
  closing one window must not discard edits still open in another.
- Save As switches the shared editing session and all its windows to the new
  file, leaving the original unchanged. Do not silently replace a destination
  being edited in another Kver session.
- External reload starts a new editing baseline and clears annotation undo/redo.
  Ordinary Save preserves undo/redo.
- Provide an optional app-wide Annotation author setting, initially blank,
  applied to new annotations only. Do not infer a name from the operating-system
  account or rewrite existing authorship when recoloring. Existing annotations
  remain editable regardless of author. An onboarding dialog is deferred.
- Protect unsaved edits when closing, replacing a document, or quitting.
  Persistent crash recovery is deferred; unsaved edits can be lost on a crash
  or power failure.

## Other platforms

Linux and Windows are targets with separate delivery gates. Evaluate their
feasibility early, but do not hold the first macOS download until their
packages are ready.

## Out of scope for the initial reader

Creating or editing PDF contents, OCR, PDF form editing, digital signatures,
cloud document libraries, and collaborative editing are not initial-release
commitments.
