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

## Keyboard shortcuts

Add bindings to `config.toml` to change a key or add an alias. Multi-key
sequences such as `g t` work too. See [Customize keyboard shortcuts](docs/keybindings.md)
for examples, command names, contexts, and error recovery.

## Goals

- Fast and responsive PDF viewing
- Keyboard-first, but not keyboard-only
- Minimal, distraction-free interface
- Powerful navigation inspired by Vim and Sioyek

## Technology

Kver is written in Rust.

Kver uses:

- **UI:** [GPUI](https://gpui.rs) & [GPUI Kit](https://gpui-kit.com)
- **PDF engine:** [Hayro](https://github.com/LaurenzV/hayro)

See [Architecture](docs/architecture.md) for a source map and ownership
boundaries, and [Future direction](docs/roadmap.md) for longer-term ideas.

## Run from source

On macOS, install Rust 1.92 or newer and Xcode with the Metal Toolchain
component. Install [Corgi](https://github.com/ConradIrwin/corgi) with
`cargo install corgi-build`. From a checkout of this repository, run:

```sh
corgi run -- examples/field-guide.pdf
```

Run `corgi run` without a path to choose a PDF from the window. Standard is
the default keyboard preset; Settings → Keyboard shows the active shortcuts.
The toolbar, menus, and command palette expose reader commands too.
Use `corgi run --release -- examples/field-guide.pdf` to evaluate performance.

The illustrated field guide includes bookmarks, images, tables, and a graph.
The separate `examples/readme-sample.pdf` is a long-document test fixture.

To build a local ad-hoc-signed `.app` (not Developer ID signed or notarized):

```sh
sh scripts/build-macos-app.sh
open -n -a "$PWD/target/release/Kver.app"
```

If `CARGO_TARGET_DIR` is set, use its `release/Kver.app` instead. The bundle
includes the app icon and Finder PDF association. Build on the Mac where you
intend to run it; binaries are architecture-specific. If the Metal toolchain
is missing, install it with `xcodebuild -downloadComponent MetalToolchain`.

Maintainers: see [macOS packaging and releases](docs/releasing.md).

### Build and test

Corgi is the local macOS build path. Its machine-wide cache shares compiled
artifacts across checkouts:

```sh
corgi build
corgi build --release
corgi test
```

Corgi is macOS-only. Linux Cloud Runners and CI continue using Cargo
(`cargo +stable test --locked --all-targets` in CI). For an opt-in capture,
run `corgi test --features screenshot --test visual` on macOS. Local macOS
packaging uses Corgi when available and falls back to Cargo. The capture tool
is not run by plain `corgi test` or PR CI.

## License

Kver's original code, documentation, and app icon are [MIT-licensed](LICENSE).
Third-party dependencies and copied material retain their own licenses.
