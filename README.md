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

## Appearance

Open **Kver → Settings…** (Command-,), use the Settings button in the reader,
or search for Settings in the command palette. Choose **System**, **Light**,
or **Dark**; the reader changes immediately and remembers your choice.

You can also edit `~/Library/Application Support/com.JonGretar.Kver/config.toml`
on macOS. For example:

```toml
[application]
appearance = "dark"
```

Valid `appearance` values are `"system"`, `"light"`, and `"dark"`. No file is
needed for the default System choice. Changes made directly to the file take
effect while Kver is running; invalid edits leave the last valid settings active.

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
corgi run -- examples/field-guide.pdf
```

Run `corgi run` without a path to choose a PDF from the window. Standard is the
default keyboard preset. While the reader has focus, use ↑/↓ to scroll a line,
Page Up/Down to scroll most of a screen, and ←/→ to turn pages. On macOS,
Option-↑/↓ also turns pages; Command-Plus/Minus zooms, and
Command-Option-W/F fits width/page. These modified shortcuts work from the
reader, not while typing in Find or using a dialog. The toolbar, menus, and
command palette invoke the same reader commands. `Ctrl-O`/`Ctrl-I` go back and
forward through semantic jumps when the reader or Contents has focus;
Option-←/→ are Standard aliases for those commands. Command-Shift-M opens the
marks dialog without requiring a letter sequence. Command-T opens Contents;
Kver has no tabs.

The prototype renders visible pages in background work. The illustrated,
24-page field guide includes a contents page, bookmarks, images, tables, and a
graph. The separate `examples/readme-sample.pdf` is a 100-page navigation and
test fixture.

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
