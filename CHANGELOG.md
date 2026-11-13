# Changelog

All notable changes to Kver will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Set up appearance, keyboard shortcuts, annotation author, and reading view in a first-launch welcome window.
- Optionally remove missing PDFs from Open Recent in Persistence settings, without erasing saved marks or reading positions. Automatic cleanup is off by default.
- See “Unsaved changes” beside the PDF filename in every window sharing its edits. The indication stays visible while saving and clears only when no unsaved edits remain.
- Browse existing highlights, underlines, strikethroughs, and squiggly underlines in the Annotations sidebar. Jump to marked passages and copy their text; annotations without recoverable text remain listed.
- Highlight selected text and save editable annotations in the current PDF, or deliberately create a copy with Save As. Windows of the same PDF share highlights and unsaved state; close, replacement, and quit offer Save, Discard, or Cancel. Readonly originals can be saved elsewhere, and externally changed PDFs are not silently overwritten.
- Set an optional author for new annotations in Settings or TOML. Existing authors remain unchanged, and the Annotations list shows stored authors when present.
- Select highlights directly on the page, choose explicitly when they overlap, and recolor or delete them across shared windows. Five named colors are available; imported geometry, authorship, and comments remain intact.
- Undo and redo annotation creation, recoloring, and deletion from either window, even after saving. Returning to the saved annotation state clears “Unsaved changes” without moving your reading position or text selection.
- Recolor or delete any supported text markup from the Annotations list, including entries without recoverable text. Stored authors and imported colors stay unchanged until explicitly edited.
- Underline, strike through, or add squiggly underlines to selected text from the context menu, Edit menu, or command palette. All four markup types support page editing, shared undo/redo, and editable PDF saving. Line markup starts red, and each type remembers its own last-used color during the session.
- Reload external PDF changes across shared windows while keeping each reading position where possible. Unsaved conflicts offer Save As elsewhere or discard-and-reload; cancellation and reload failures keep Kver's edits.

### Changed

- Apply any text-markup type from a compact split toolbar button, with a separate creation-color picker. Annotations show styled text previews and compact page/author details; narrow sidebar tabs use icons, and annotation commands are grouped in the Edit menu.
- Use dedicated Standard annotation shortcuts or Vim's `a` sequences to apply markup, including repeating the last type. Vim also adds `g a` for Annotations and `u`, Ctrl-R, and `x` for annotation undo, redo, and deletion; list deletion acts on the selected entry.
- Vim now uses `J/K` for PDF page turns, `n/N` for Find matches, `g c` for Contents, and Ctrl-F/B and Ctrl-D/U for screen and half-screen scrolling. Modified Find/sidebar shortcuts belong to Standard; Space and Shift-Space remain half-screen aliases.
- Red replaces Purple among the five markup color presets. Existing custom purple shortcuts and imported colors remain unchanged.
- Search settings and resize their sidebar in the new GPUI Kit settings layout. Appearance and keyboard preset can still be reset individually; other settings and their saved values are unchanged.
- Opening the same PDF in multiple windows keeps them on one loaded version, with independent reading positions, zoom, reading modes, and text selections.

### Fixed

- Select and copy ordinary multi-column text in reading order, finishing each column before continuing to the next, without highlighting across column gutters.

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
