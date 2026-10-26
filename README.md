# Kver

**A keyboard-friendly PDF reader, starting on macOS.**

Kver aims to be a responsive PDF reader for people who spend a lot of time
reading documents. A single-page technical prototype exists; there is no
downloadable reader yet.

The name _Kver_ is Icelandic for a small book or booklet.

Inspired by [Sioyek](https://sioyek.info/) and
[Skim](https://skim-app.sourceforge.io/), Kver aims to pair keyboard navigation
with a minimal desktop interface.

Although particularly suited to developers, researchers, and technical users, Kver aims to be a general-purpose PDF reader that is equally comfortable with a mouse.

## Goals

- Fast and responsive PDF viewing
- Keyboard-first, but not keyboard-only
- A first downloadable release on macOS, with Linux and Windows as separate targets
- Minimal, distraction-free interface
- Powerful navigation inspired by Vim and Sioyek
- Search, text interaction, and reading-position restoration as the reader matures
- A visual ruler and configurable keyboard shortcuts as future goals

## Technology

Kver is written in Rust.

The proposed technology stack is:

- **UI:** [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui)
- **PDF engine:** PDFium via `pdfium-render` or an alternative, to be tested
- **Persistence:** SQLite
- **Configuration:** TOML

The prototype uses Hayro to render one page without requiring a native PDF
library. This is not a backend decision: PDFium and `gpui-pdf` still need to
be evaluated for rendering quality, text geometry, and packaging. GPUI
provides the window; Hayro rasterizes the PDF page in background work.

See [Architecture](docs/architecture.md) for details.

## Plan

1. Validate the UI and PDF backends with representative documents, including
   text geometry and coordinates needed for future features.
2. Release a buildable, downloadable **unsigned macOS MVP** that opens and
   reads PDFs smoothly, with basic predefined keyboard and mouse controls.
3. Add search, selection/copy, links, outline, history, and reading-position
   restoration to make it a daily reader.

The visual ruler, configurable bindings, annotations, and LaTeX/SyncTeX
integration remain goals without assigned release dates. Password-protected
PDFs are required eventually, but do not block the first download unless
support is straightforward. See [Features](docs/features.md) and
[Roadmap](docs/roadmap.md) for the gates and later goals.

## Status

The prototype accepts a local PDF path, renders its first page, and offers
zoom buttons. It does not navigate pages, compare PDF engines, or satisfy
the macOS MVP gate.

On macOS, install Rust and Xcode with the Metal Toolchain component, then
open the sample PDF generated from the introduction and goals above:

```sh
cargo run -- examples/readme-sample.pdf
```

You can also pass another local PDF path. The `−` and `+` buttons render its
first page at different scales. To regenerate the sample after changing this
README, run `python3 examples/generate_readme_pdf.py` (Python standard library
only).

Run `cargo test --no-default-features --lib` to exercise PDF rendering without
building GPUI. The complete `cargo test` and `cargo clippy` require the Metal
Toolchain. If Xcode reports it missing, install it with:

```sh
xcodebuild -downloadComponent MetalToolchain
```

The next validation step is to run the window with real PDFs and compare
backends as described in the [roadmap](docs/roadmap.md#0-validate-the-stack).

## Inspiration

- [Sioyek](https://github.com/ahrm/sioyek) — Keyboard-driven PDF navigation
- [Zed](https://zed.dev/) — Modern Rust desktop applications and GPUI
- Vim / Neovim — Modal navigation and configurable commands

## License

To be determined.
