# Development

Kver is written in Rust, using [GPUI](https://gpui.rs) and
[GPUI Kit](https://gpui-kit.com) for the UI and
[Hayro](https://github.com/LaurenzV/hayro) for PDF rendering and extraction.
See [Architecture](architecture.md) for the source map and ownership
boundaries. Read [AGENTS.md](../AGENTS.md) for project conventions and the
[workflow](agents/workflow.md) for collaboration and validation requirements.

## Run from source

On macOS, install Rust 1.92 or newer and Xcode with the Metal Toolchain
component. Use the repository's `rust-toolchain.toml` for the pinned local
toolchain. If Metal is missing:

```sh
xcodebuild -downloadComponent MetalToolchain
```

Install [Corgi](https://github.com/ConradIrwin/corgi), the local macOS build
tool with a machine-wide shared cache:

```sh
cargo install corgi-build
```

From a checkout of this repository:

```sh
corgi run -- examples/field-guide.pdf
```

Run `corgi run` without a path to choose a PDF from the window. To evaluate
performance, use an optimized build:

```sh
corgi run --release -- examples/field-guide.pdf
```

The illustrated field guide includes bookmarks, images, tables, and a graph.
The separate `examples/readme-sample.pdf` is a long-document test fixture.

## Build and test

On macOS, use Corgi for supported build, check, test, and lint operations
when installed. Check its installed help for flags; follow the command
policy in `AGENTS.md`.

```sh
corgi build
corgi build --release
corgi test
cargo fmt --check
```

Corgi is macOS-only. Linux Cloud Runners and CI use Cargo; the CI test command
is:

```sh
cargo +stable test --locked --all-targets
```

For UI inspection, the opt-in capture tool is available on macOS:

```sh
corgi test --features screenshot --test visual
```

It is not run by plain `corgi test` or PR CI. See the
[capture workflow](agents/workflow.md) for adapting the tool, showing images,
and restoring its baseline. Headless tests and captures do not replace native
window/input checks.

## Build a local macOS app

```sh
sh scripts/build-macos-app.sh
open -n -a "$PWD/target/release/Kver.app"
```

If `CARGO_TARGET_DIR` is set, use its `release/Kver.app` instead. The bundle
includes the app icon and Finder PDF association. It is ad-hoc signed, not
Developer ID signed or notarized. Build on the Mac where you intend to run
it; binaries are architecture-specific.

The builder uses Corgi when available and falls back to Cargo.
Maintainers: see [macOS packaging and releases](releasing.md) for candidate
archives, update signing, release checks, and native quit upgrade checks.
