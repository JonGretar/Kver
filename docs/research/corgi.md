# Corgi as the local macOS build path

## Findings

- [Corgi's README](https://github.com/ConradIrwin/corgi/blob/main/README.md)
  describes a machine-wide shared cache, sandboxed build scripts and macros,
  and compatibility with Cargo dependency resolution. It is currently macOS
  only and explicitly supports using Cargo in CI instead. This addresses
  duplicated build artifacts across worktrees, unlike Kver's existing
  user-local Cargo `rustc-wrapper = "sccache"`, which caches compiler outputs
  but does not eliminate per-worktree `target/` directories.
- The same README says sandboxed reads of non-Rust files must be declared as
  `extra-inputs`. Kver's
  [`include_bytes!` test](../../src/pdf.rs) embeds
  `examples/readme-sample.pdf`; `corgi test --no-default-features --lib`
  initially failed with `Operation not permitted` on that file. The
  [`corgi.toml`](../../corgi.toml) entry fixes the read. Zed's
  [more extensive configuration](https://github.com/zed-industries/zed/blob/main/corgi.toml)
  pins tools for its native dependencies, but Kver's normal macOS GUI build
  succeeded without any tool entries.
- On a local Apple Silicon Mac with Corgi 0.1.92, `corgi check`,
  `corgi build`, and `corgi test --no-default-features --lib` passed (10
  tests). The first invocation created `rust-toolchain.toml` with channel
  `1.99.1` and populated `/Users/Shared/corgi`; the store measured 6.2 GiB
  after these builds and tests. This is a new store, **not** 6.2 GiB saved. Its
  machine-wide location and sharing behavior are described in Corgi's README.
- On that version, `corgi check --features screenshot` and
  `corgi test --all-targets --features screenshot` both stopped during planning
  with `unit-graph package ... atomic@0.5.3 missing from metadata`. Cargo's
  `metadata --locked --format-version 1 --features screenshot --no-deps`
  succeeded. No Corgi visual test was run; use Cargo for the full suite.

## Workaround and clean-output comparison

On a later Apple Silicon Mac with Corgi 0.1.101 and Kver at `047dbb0`, the
same screenshot planning error remained. Temporarily adding
`features = ["test-support"]` to the `gpui-kit` dependency in `Cargo.toml`
made `corgi check --features screenshot`,
`corgi test --features screenshot --test visual`, and
`corgi test --all-targets --features screenshot` pass. The last command
reported 24 tests passed, including the visual test. This was an experiment
at the time; the feature setting was later adopted for local Corgi use.

For a like-for-like local workflow, both tools used that temporary feature
setting, Rust 1.99.1, cached downloads, and empty output directories.
The Corgi store was first warmed for all three commands. Timings are wall
time from `/usr/bin/time -p` and include running visual tests; they are
single observations, not benchmark averages. Cargo on this machine had
neither `sccache` installed nor a user Cargo wrapper configured.

| Command | Cargo, fresh `CARGO_TARGET_DIR` | Corgi, warm shared store and fresh `--target-dir` |
| --- | ---: | ---: |
| Debug build | 75.16 s; 3.7 GiB | 0.30 s; 125 MiB |
| All-target screenshot tests (cumulative disk) | 57.06 s; 4.2 GiB | 50.49 s; 282 MiB |
| Release build (cumulative disk) | 138.78 s; 5.9 GiB | 0.13 s; 312 MiB |
| Whole sequence | 271.00 s | 50.92 s |

The Corgi visual runner still took about 50 seconds despite cached test
results; the near-instant build and release commands were store hits. The
shared store was 9.9 GiB after warming and remained 9.9 GiB during the
fresh-output Corgi sequence. Thus **one** worktree's measured Cargo outputs
(5.9 GiB) were smaller than Corgi's shared store plus exported outputs
(about 10.2 GiB). Two identical Cargo worktrees would occupy about 11.8 GiB
in output directories alone, while two Corgi output directories plus the
same store would be about 10.5 GiB, *if* all builds hit the same store.
These are extrapolations, not measured second-worktree results; they also
exclude Cargo's registry and toolchain disk usage and cannot predict cache
retention or differing worktree inputs.

## Current workflow

Use Corgi for local macOS builds and visual tests. Keep
[`CI`](../../.github/workflows/ci.yml) and
[`Cloud Runner setup`](../../.agents/setup) on Cargo so Linux remains
supported. The pinned Rust toolchain makes Corgi's local selection explicit;
the documented `cargo +stable` commands still select stable explicitly.
The local [preparation script](../../.agents/prepare) fetches through Corgi
when it is installed on macOS, with Cargo as the fallback. The
local [macOS bundle builder](../../scripts/build-macos-app.sh) likewise
builds with Corgi when available, including when called by the archive
packager; without Corgi it keeps the Cargo release build. The release CI
runner does not install Corgi, so it takes that Cargo fallback. The
[`gpui-kit` dependency](../../Cargo.toml) now enables `test-support` for GUI
builds so Corgi's package metadata contains dependencies needed by the
separately gated `screenshot` feature. This is a workaround for Corgi's
feature-planning mismatch, not a reason to make the visual test target a
default. Revisit it when Corgi fixes the planner.

Measure several worktrees' `target/` sizes plus the shared Corgi store before
claiming a net disk saving. Do not remove Cargo caches until the commands
needed for a given workflow have been validated with Corgi.
