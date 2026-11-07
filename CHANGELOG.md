# Changelog

All notable changes to Kver will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Choose whether Kver remembers reading history; while off, new opens stay available during the session without being saved. Clear all saved reading data from Settings with confirmation, or forget one PDF from Open Recent without deleting PDF files.
- Find previously opened PDFs with Open Recent from the command palette, File menu, or Command-Shift-O; search older entries.
- Read one PDF page at a time in Single Page mode; scroll within that page without turning it, and switch modes or turn pages with reader controls, shortcuts, menus, or the command palette.
- Open Settings from the reader, command palette, or macOS Kver menu; choose System, Light, or Dark appearance and keep that choice after restarting.
- Close the active window with Command-W or the Window menu on macOS.
- Choose whether closing the last window quits Kver; on macOS, leave it running to reopen from the Dock, create a new window, or open a PDF from Finder.
- Choose whether newly opened PDFs start at 100%, Fit Width, or Fit Page in Settings without changing PDFs already open.

### Changed

- Apply valid edits to the settings file while Kver is open; keep the last working appearance and show an error during incomplete or invalid edits without overwriting the file.
- Show Contents before Find in the sidebar and open Contents by default the first time the sidebar is shown.

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
