# Kver

**A keyboard-friendly PDF reader, starting on macOS.**

Kver aims to be a responsive PDF reader for people who spend a lot of time
reading documents. A continuous-scroll technical prototype exists; there is no
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

The current prototype uses:

- **UI:** [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui)
- **PDF engine:** [Hayro](https://github.com/LaurenzV/hayro)

Hayro is the chosen initial backend. PDFium is a comparison option if a
significant limitation calls for revisiting that choice. SQLite persistence and
TOML configuration are future plans, not part of the prototype.

See [Architecture](docs/architecture.md) for implementation details.

The first goal is a downloadable unsigned macOS reader. See
[Features](docs/features.md) for scope and [Roadmap](docs/roadmap.md) for
release gates and current progress.

## Try the prototype

On macOS, install Rust 1.92 or newer and Xcode with the Metal Toolchain
component. Install [Corgi](https://github.com/ConradIrwin/corgi) with
`cargo install corgi-build`. From a checkout of this repository, run:

```sh
corgi run -- examples/readme-sample.pdf
```

Run `corgi run` without a path to choose a PDF from the window. Scroll
with a wheel/trackpad or `j`/`k`; jump between pages with left/right or `p`/`n`.
Use `=` and `-` or the toolbar buttons to zoom. The prototype renders visible
pages in background work, but is not yet the downloadable macOS MVP.

For a local unsigned `.app`, see the [macOS build guide](docs/macos-app.md).
For rendering evidence and known limitations, see the
[validation record](docs/rendering-validation.md).

### Build and test

Corgi is the local macOS build path. Its machine-wide cache shares compiled
artifacts across checkouts:

```sh
corgi build
corgi build --release
corgi test --all-targets --features screenshot
```

Until Corgi's feature-planning issue is fixed, Kver enables GPUI Kit's
`test-support` dependencies for normal GUI builds so Corgi can run the visual
tests. The `screenshot` feature still gates the visual test target; plain
`corgi test` does not run it. This adds build dependencies to normal GUI
builds, but the measured release binary grew by about 0.5 MiB. The first
Corgi build populates the shared store; it does not remove existing Cargo
`target/` directories or the user-local `sccache` cache.

Corgi is macOS-only. Linux Cloud Runners and CI continue using Cargo
(`cargo +stable test --locked --all-targets --features screenshot` for the
full suite). Local macOS packaging uses Corgi when available and falls back
to Cargo. See the
[Corgi evaluation](docs/research/corgi.md) for measurements and limitations.

## Inspiration

- [Sioyek](https://github.com/ahrm/sioyek) — Keyboard-driven PDF navigation
- [Zed](https://zed.dev/) — Modern Rust desktop applications and GPUI
- Vim / Neovim — Modal navigation and configurable commands

## License

Kver's original code, documentation, and app icon are [MIT-licensed](LICENSE).
Third-party dependencies and copied material retain their own licenses.
