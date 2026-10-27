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
- **PDF engine:** Hayro preferred, pending validation; PDFium as a fallback
- **Persistence:** SQLite
- **Configuration:** TOML

The prototype uses Hayro to render one page without requiring a native PDF
library, and the pinned `gpui-pre` 0.3.10 GPUI snapshot for the UI and headless
screenshot renderer. Hayro is the preferred initial backend if validation
confirms rendering quality, positioned text, and responsive operation on
representative PDFs. PDFium remains the comparison/fallback on meaningful gaps.
GPUI provides the window; Hayro rasterizes the PDF page in background work.

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

The prototype accepts a local PDF path, renders its first page at the
window's display density, and offers zoom buttons. It does not navigate
pages or satisfy the macOS MVP gate; display quality still needs visual
testing on macOS with real PDFs.

On macOS, install Rust 1.92 or newer (Hayro 0.8's minimum) and Xcode with the
Metal Toolchain component, then open the sample PDF generated from the
introduction and goals above. `rustup update stable` installs or updates the
stable toolchain; `cargo +stable` uses it even when the default is an older
nightly:

```sh
cargo +stable run -- examples/readme-sample.pdf
```

You can also pass another local PDF path. The `−` and `+` buttons render its
first page at different scales. To regenerate the sample after changing this
README, run `python3 examples/generate_readme_pdf.py` (Python standard library
only).

Run `cargo +stable test --no-default-features --lib` to exercise PDF rendering
without building GPUI. The complete test and Clippy checks also require the
Metal Toolchain. If Xcode reports it missing, install it with:

```sh
xcodebuild -downloadComponent MetalToolchain
```

For new Delta checkouts, `.agents/prepare` fetches locked Rust dependencies
when Cargo is available (on macOS, install Rust first). On a Cloud Runner,
`.agents/setup` installs stable Rust, Clippy, rustfmt, Linux XKB and Fontconfig
development files, and Mesa's software Vulkan driver, then fetches dependencies
after preparation.
Both scripts need network access on a fresh machine; neither starts a window.
For headless Linux validation, run
`cargo +stable test --locked --no-default-features --lib`. To exercise the
actual GPUI reader with the sample PDF (no display server required) and save
before/after zoom captures, run:

```sh
cargo +stable test --locked --features screenshot --test visual
```

The [visual test](tests/visual.rs) writes `target/captures/before.png` and
`after.png`. Set `KVER_TEST_PDF` to open another document and
`KVER_CAPTURE_DIR` to choose another output directory. Agents can add Rust test
functions there to drive the reader, assert state, and capture any UI they need
to inspect; call new functions from the test target's
`main` so GPUI stays on the process main thread (required by macOS Metal).
Unlike the old scenario runner, this test opens the normal asynchronous reader,
not a pre-rendered page. It invokes the same zoom behavior as the GUI button,
but does not synthesize a click or prove the button's hit-testing. For an
exploratory pointer check, capture a frame first (to lay out hitboxes), call
`click(&mut cx, window.into(), gpui::point(px(x), px(y)))?` with window-local
**logical** pixel coordinates, then capture again. Screenshot dimensions may
be scaled by the display density, so screenshot pixel coordinates are not
necessarily click coordinates. Save probes under `target/` and remove the
temporary test code after inspecting them; keep only cases intentionally
promoted to CI.

The `screenshot` feature enables GPUI's test-support renderer; normal GUI
execution still rasterizes off the UI thread. Headless Linux captures depend
on a working Vulkan software adapter. Metal headless capture has also been
validated on a local Mac; the normal GUI binary builds and launches, and the
sample window's display was confirmed on a local Mac. The
[PR checks](.github/workflows/ci.yml) build, test, and lint on Linux and macOS;
the visual integration test is required on both. Captures are uploaded for
visual review.

The next validation step is to run the window with real PDFs and evaluate
Hayro's gaps as described in the [Phase 0 validation guide](docs/rendering-validation.md).

## Inspiration

- [Sioyek](https://github.com/ahrm/sioyek) — Keyboard-driven PDF navigation
- [Zed](https://zed.dev/) — Modern Rust desktop applications and GPUI
- Vim / Neovim — Modal navigation and configurable commands

## License

To be determined.
