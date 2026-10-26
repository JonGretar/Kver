# Folio

**A modern, cross-platform PDF reader built for the keyboard.**

Folio is a fast, lightweight PDF reader designed for people who spend a lot of time reading documents.

Inspired by [Sioyek](https://sioyek.info/), Folio combines Vim-style keyboard navigation with a modern, minimal desktop interface.

Although particularly suited to developers, researchers, and technical users, Folio aims to be a general-purpose PDF reader that is equally comfortable with a mouse.

## Goals

- Fast and responsive PDF viewing
- Keyboard-first, but not keyboard-only
- Cross-platform support for macOS, Linux, and Windows
- Minimal, distraction-free interface
- Powerful navigation inspired by Vim and Sioyek
- Persistent reading positions, marks, and document history
- A built-in visual ruler for focused reading
- Configurable keyboard shortcuts and behavior

## Technology

Folio is written in Rust.

The proposed technology stack is:

- **UI:** [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui)
- **PDF engine:** PDFium via `pdfium-render` (provisional)
- **Persistence:** SQLite
- **Configuration:** TOML

Alternative PDF implementations, particularly Hayro and `gpui-pdf`, will be evaluated during early development.

See [Architecture](docs/architecture.md) for details.

## Features

The initial focus is on creating an excellent everyday reading experience:

- Continuous PDF scrolling and zooming
- Vim-style keyboard navigation
- Visual ruler with keyboard-controlled movement
- Text searching, selection, and copying
- Document outlines and internal links
- Marks and navigation history
- Automatic restoration of reading position
- Configurable keyboard shortcuts

More advanced features, such as highlights, annotations, smart reference jumping, and split views, are planned for later development.

See [Features](docs/features.md) and [Roadmap](docs/roadmap.md).

## Status

Folio is in the planning and early prototyping stage.

The initial development priority is validating GPUI and the PDF rendering engine, followed by building a minimal but usable reader.

## Inspiration

- [Sioyek](https://github.com/ahrm/sioyek) — Keyboard-driven PDF navigation
- [Zed](https://zed.dev/) — Modern Rust desktop applications and GPUI
- Vim / Neovim — Modal navigation and configurable commands

## License

To be determined.
