# Kver

**A keyboard-friendly PDF reader, starting on macOS.**


The name _Kver_ is Icelandic for a small book or booklet.

Inspired by [Sioyek](https://sioyek.info/) and [Skim](https://skim-app.sourceforge.io/), Kver aims to pair keyboard navigation with a minimal desktop interface.

Although particularly suited to developers, researchers, and technical users, Kver aims to be a general-purpose PDF reader that is equally comfortable with a mouse.

## Install

### macOS (Apple silicon)

1. Download the macOS ZIP from [GitHub Releases](https://github.com/JonGretar/Kver/releases).
2. Extract it and drag `Kver.app` into **Applications**.
3. Open Kver and choose a PDF with **Open PDF**.

Kver is not yet Apple Developer ID signed or notarized, so macOS may block
it with a security warning. I plan to add signing and notarization when I can be
bothered to go through the process.

If macOS blocks the app, try **System Settings → Privacy & Security → Open
Anyway** or do this command:

```sh
xattr -dr com.apple.quarantine /Applications/Kver.app
```

### Windows and Linux

Windows and Linux releases will follow when someone joins me to work on
packaging and testing those platforms. If you would like to help, please
[open an issue](https://github.com/JonGretar/Kver/issues).

## Goals

- Fast and responsive PDF viewing
- Keyboard-first, but not keyboard-only
- Minimal, distraction-free interface
- Powerful navigation inspired by Vim and Sioyek

## Technology

Kver is written in Rust.

The current prototype uses:

- **UI:** [GPUI](https://gpui.rs) & [GPUI Kit](https://gpui-kit.com)
- **PDF engine:** [Hayro](https://github.com/LaurenzV/hayro)

See [Architecture](docs/architecture.md) for implementation details.

## Run from source

On macOS, install Rust 1.92 or newer and Xcode with the Metal Toolchain
component. Install [Corgi](https://github.com/ConradIrwin/corgi) with
`cargo install corgi-build`. From a checkout of this repository, run:

```sh
corgi run -- examples/readme-sample.pdf
```

Run `corgi run` without a path to choose a PDF from the window. Scroll
with a wheel/trackpad or `j`/`k`; jump between pages with left/right or `p`/`n`.
Use `=` and `-` or the toolbar buttons to zoom. The prototype renders visible
pages in background work.

For a local unsigned `.app`, see the [macOS build guide](docs/macos-app.md).
For rendering evidence and known limitations, see the
[validation record](docs/rendering-validation.md).

### Build and test

Corgi is the local macOS build path. Its machine-wide cache shares compiled
artifacts across checkouts:

```sh
corgi build
corgi build --release
corgi test
```

Until Corgi's feature-planning issue is fixed, Kver enables GPUI Kit's
`test-support` dependencies for normal GUI builds so Corgi can run the visual
capture tool when requested. The `screenshot` feature gates that opt-in target;
plain `corgi test` and CI do not run it. This adds build dependencies to normal GUI
builds, but the measured release binary grew by about 0.5 MiB. The first
Corgi build populates the shared store; it does not remove existing Cargo
`target/` directories or the user-local `sccache` cache.

Corgi is macOS-only. Linux Cloud Runners and CI continue using Cargo
(`cargo +stable test --locked --all-targets` in CI). For an opt-in capture,
run `corgi test --features screenshot --test visual` on macOS. Local macOS
packaging uses Corgi when available and falls back to Cargo.

## License

Kver's original code, documentation, and app icon are [MIT-licensed](LICENSE).
Third-party dependencies and copied material retain their own licenses.
