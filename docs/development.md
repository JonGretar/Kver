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
sh packaging/macos/build-app.sh
open -n -a "$PWD/target/release/Kver.app"
```

If `CARGO_TARGET_DIR` is set, use its `release/Kver.app` instead. The bundle
includes the app icon and Finder PDF association. It is ad-hoc signed, not
Developer ID signed or notarized. Build on the Mac where you intend to run
it; binaries are architecture-specific.

The builder uses Corgi when available and falls back to Cargo.
Maintainers: see the [release runbook](releasing.md) for candidate
archives, update signing, release checks, and native quit upgrade checks.

## Build a Linux archive

Use native x86-64 Ubuntu 24.04 and Python 3.11 or newer. Install the same
Fontconfig/xkbcommon build libraries as Linux CI, plus dynamic loaders and
ELF inspection tools:

```sh
sudo apt-get install --no-install-recommends \
  libfontconfig1-dev libxkbcommon-dev libxkbcommon-x11-dev \
  libvulkan1 mesa-vulkan-drivers libwayland-client0 libegl1 binutils
rustup toolchain install stable --profile minimal --component rust-docs
cargo +stable install cargo-about --locked --version 0.9.3 --features cli
sh packaging/linux/package.sh
```

The entrypoint resolves the checkout from its script path, builds the locked
release binary for `x86_64-unknown-linux-gnu`, generates dependency/runtime
license notices, and verifies the actual tarball after clean extraction.
Output is under `target/x86_64-unknown-linux-gnu/release` (or
`CARGO_TARGET_DIR`). `readelf`/`ldd` inspect linked dependencies; explicit loader
probes cover Wayland, Vulkan and EGL. Missing required contents, notices or
loaders fail packaging rather than silently omitting them.

The **Linux archive** workflow uses `ubuntu-24.04`. It builds in parallel with
macOS and makes the tarball and checksum available to the release workflow
for upload after publication. Manual recovery can still attach assets to an
existing release. The [Linux guide](linux.md), also shipped in the archive,
covers runtime limits, manual updates and the native desktop checklist.

## Build an experimental Windows archive

Use native x86-64 Windows, PowerShell 7, Visual Studio's **Desktop development
with C++** tools, and the Windows SDK (including `rc.exe` and `fxc.exe`).
Run from an **x64 Native Tools** developer environment. Install stable MSVC
Rust and the build-only inspection/license tools:

```powershell
rustup toolchain install stable-x86_64-pc-windows-msvc --profile minimal --component rust-docs
python -m pip install -r packaging/windows/requirements.txt
cargo +stable install cargo-about --locked --version 0.9.3 --features cli
./packaging/windows/package.ps1
```

Python 3.11 or newer is required. The entrypoint resolves the repository from
its script path, builds with `cargo +stable build --locked --release --target
x86_64-pc-windows-msvc --bin kver`, generates dependency license notices, and
packages under `target/x86_64-pc-windows-msvc/release` (or `CARGO_TARGET_DIR`).
`pefile` inspects the produced PE resources/imports; `cargo-about` collects
license texts from the locked Windows dependency graph. Neither ships as an
application dependency. Rust documentation supplies the Rust runtime notices.
Unknown imports or unsatisfied licenses fail the job rather than silently
shipping incomplete requirements.

The **Experimental Windows archive** workflow runs the same entrypoint on
`windows-2025` and uploads a versioned experimental ZIP and checksum, without
publishing a release. It also supports manual dispatch. The ZIP is extracted
into a fresh temporary directory and rechecked, including the landed CLI's
`--version` from an unrelated working directory. No native GUI acceptance is
implied. See [Windows tester guidance](windows.md), also included in the ZIP,
for extraction, runtime/security requirements, manual updates and native checks.
