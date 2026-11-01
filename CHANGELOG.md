# Changelog

All notable changes to Kver will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
