# Changelog

All notable changes to Kver will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Follow a page with a reading ruler: toggle it from the menu or command
  palette, or Option/Alt-click text to place it. In the Vim keymap, `r`
  toggles the guide and `j`/`k` follow lines while it is enabled.
- Continue ruler reading across columns and pages in either layout, including
  past textless pages. The guide reveals text at high zoom and follows manual
  scrolling after it settles, without pulling the page back.
- Keep the ruler through search, links, marks, contents, and back/forward
  navigation. Page jumps place it on the first line; selection and annotation
  editing temporarily hide it without losing your place.
- Automatic ruler movement skips confidently repeated running headers, footers,
  and page numbers using nearby pages. Option/Alt-click can still place the
  ruler on skipped text; selection, copying, and search remain unchanged.

### Changed

- In Vim mode, `j` and `k` navigate the focused Contents or Annotations
  sidebar just like the Down and Up arrow keys.

## [0.5.1] - 2026-11-14

### Changed

- First-launch setup now explains each keyboard map and lists system appearance first.
- Selected annotations now have compact editing controls with their current
  color, undoable deletion, and a separate dismiss button. Escape deselects
  without deleting; toolbar creation controls remain separate.
- Overlapping annotations show styled passage previews when choosing a target.
  The Annotations sidebar now starts directly with the list; copying selected
  annotation text remains available by keyboard.

## [0.5.0] - 2026-11-13

Add a little emphasis to your reading: Kver can now save text markup directly
in your PDFs, ready to reopen here or in another reader.

### Added

- **Text markup:** highlight, underline, strike through, or add squiggly
  underlines to selected text. Choose from five colors, with a remembered color
  for each markup type.
- **An Annotations sidebar:** browse marked passages, jump to them, copy their
  text, and recolor or delete markup. Existing annotations from other readers
  are included, and overlapping annotations can be chosen individually.
- **Save and Save As:** save editable annotations in the PDF or make a separate
  copy. “Unsaved changes” beside the filename and Save / Discard / Cancel
  prompts help protect your work. Read-only originals can be saved elsewhere.
- **Shared editing and undo:** windows showing the same PDF share edits and
  undo/redo, even after saving, without moving each other's reading position.
- **External-change protection:** clean PDFs reload when another app changes
  them. If you have unsaved edits, choose between saving your version elsewhere
  and discarding it to reload; Kver does not silently overwrite the other file.
- **Optional annotation authorship:** set a name for new markup in Settings.
  Existing authors stay unchanged and appear in the Annotations list.
- **A first-launch welcome window:** choose appearance, keyboard preset,
  annotation author, and reading view before you start.
- **Optional Open Recent cleanup:** remove missing PDFs from the list without
  erasing saved marks or reading positions. Automatic cleanup is off by default.

### Changed

- A compact markup toolbar remembers your last type and offers a separate color
  picker. The sidebar shows styled text previews, and annotation commands are
  grouped under **Edit → Annotation**.
- **Vim shortcuts have changed:** use `J` / `K` for page turns and `n` / `N`
  for Find matches. `g c` opens Contents and `g a` opens Annotations.
  `Ctrl-F` / `Ctrl-B` scroll a screen; `Ctrl-D` / `Ctrl-U` scroll half a screen.
  Space / Shift-Space remain half-screen aliases.
- Vim's `a a` repeats the last markup type; `a h`, `a u`, `a s`, and `a w`
  apply the four types. Use `u`, `Ctrl-R`, and `x` to undo, redo, and delete
  annotations. Standard has its own modified shortcuts; see **Settings →
  Keyboard** for the active bindings.
- Settings now has search and a resizable sidebar.

### Fixed

- Selection and copying now follow ordinary multi-column text in reading order,
  finishing one column before the next instead of spanning the gutter.

### Known issues

- Some PDFs with malformed annotation entries may hide later annotations in
  Kver, even though their saved data is intact. This rendering limitation is
  tracked in [#142](https://github.com/JonGretar/Kver/issues/142).

## [0.4.0] - 2026-11-09

### Added

- Choose Standard or Vim keyboard shortcuts in Settings, browse active shortcuts by task, and see reader shortcuts in the command palette. Customize bindings and aliases in the settings file; valid edits apply live, while invalid edits leave the last working map active. Unbinding is not yet supported.
- Follow links within a PDF by clicking or focusing them and pressing Enter. Supported links to another local PDF open that document in a new window; unavailable targets leave the current reader intact.
- Jump back and forward through link, Find, Contents, and local-mark destinations with Control-O/Control-I, toolbar buttons, the Navigate menu, or the command palette.
- Open Settings from the reader, palette, or app menu; choose a persistent System, Light, or Dark appearance and a default opening zoom of 100%, Fit Width, or Fit Page.
- Choose Continuous or Single Page as the opening reading view, with an optional per-PDF override. Single Page scrolls within the page without turning it; page turns remain explicit.
- Search all stored PDFs in Open Recent; optionally resume at the last reading position. Turn off history recording to keep new per-PDF activity session-only, forget one PDF, or clear all saved reading data without deleting the files.
- Save per-PDF letter marks, optionally name them, and revisit them with reader keys, the command palette, or a searchable popup (Command-Shift-M). Marks persist across restarts while history recording is on.
- Show or hide the toolbar and status bar in Settings. On macOS, choose whether closing the last window quits Kver or leaves it available from the Dock, menu, and Finder.

### Changed

- Standard now uses conventional desktop navigation; Vim-style letters and mark sequences belong to the Vim preset. Vim adds sideways and half-screen scrolling, `gg`/G for document bounds, `/` for Find, and `o`/Shift-O for opening PDFs.
- Settings are organized into Application, Reader, Persistence, and Keyboard pages. Valid hand edits apply while Kver runs; incomplete or invalid edits do not overwrite the file or replace working settings.
- Reader chrome uses Catppuccin Mocha/Latte, a centered filename, quieter Find/Contents controls, and compact toolbar controls when space is tight. PDF page colors are unchanged.

### Fixed

- Keep marks search focused when deleting an entry and prevent popup clicks from reaching the PDF.
- Continue opening a PDF when its window moves between displays with different pixel densities during loading.

## [0.3.1] - 2026-11-06

### Added

- Added auto-update on macOS

## [0.3.0] - 2026-11-05

### Changed

- Choose Light, Dark, or System appearance from an icon-only toolbar menu. The choice applies to all reader windows for the current session; Copy remains available through shortcuts and menus instead of a toolbar button.
- Resize the shared Find and Contents sidebar by dragging its edge; long Contents titles now truncate to one line with the full title on hover.

### Added

- Open a window-local sidebar for Find or Contents from the toolbar, menu, or command palette; switch panels with bottom tabs and dismiss it without losing the document view.
- Find mapped text across a PDF with a search query, page-labelled snippets, next/previous navigation, and a temporary highlight on the page.
- Browse a PDF's own Contents hierarchy and jump to supported page destinations, including positions on cropped or rotated pages. PDFs without navigable contents show an empty state.
- On macOS, choose Kver → About Kver for an About window with the app icon, version, GitHub link, and a button to copy app details.

## [0.2.0] - 2026-11-05

### Changed

- Read in a theme-aware window with a document title bar, compact navigation and zoom controls, and visible page/zoom feedback; opening a PDF from the centered empty state or recovering from a failed open is easier to find.

### Added

- Choose PDFs with a PDF-filtered native Open dialog.
- On macOS, open PDFs from Finder with Open With and optionally choose Kver as the default PDF viewer.
- Open a PDF from a searchable command palette with a visible shortcut hint, or use Command-O and the existing reader control.
- Find page navigation, zoom, and fit commands in the palette with active shortcut hints; unavailable document commands are dimmed until a PDF loads.
- On macOS, open a PDF from the File menu with Command-O or quit from the Kver menu with Command-Q.
- Open independent PDF reader windows with File → New Window or Command-N; cycle between them with the Window menu, command palette, or Command-` / Command-Shift-`.
- Drag across ordinary PDF text, including multiple lines and paragraphs, see the selected lines highlighted, and copy mapped text with spaces and line breaks to the clipboard with Command-C, the Edit menu, or the command palette.

### Fixed

- Keep selection and copied text aligned with the visible crop on rotated PDFs; image-only pages offer no text to select or copy.
- Sign the completed macOS app bundle to fix the invalid signature behind the original download's “Kver is damaged” error; downloads still require approval because the app is not Developer ID signed or notarized.

## [0.1.0] - 2026-11-01

### Added

- Open a local PDF from the running app or launch without a document; failed opens leave the window available for another attempt.
- Open a PDF supplied on the command line in a native macOS reader with continuous scrolling, page navigation, and zoom controls.
- Render visible pages in background work with bounded bitmap memory for long documents.
- Build an unsigned macOS app bundle with an icon.
- Zoom with `=`/`+` and `-` keys as well as toolbar buttons, keeping your reading position in view.
- Download a versioned Apple silicon macOS app archive from an authorized tagged release, with unsigned-app opening instructions.
- Fit the current PDF page to the available width or the whole viewport with toolbar controls or `w`/`f` keys; manual zoom returns to fixed zoom.

### Fixed

- Show the PDF filename instead of its full path, keeping navigation and zoom controls reachable even when the name is long or the window is narrow.
- Keep short last pages navigable and correctly labelled, without fit mode repeatedly changing the page in view.
