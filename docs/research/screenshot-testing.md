# Screenshot testing in Zed and Kver

Research against Zed revision
[`2c335b8`](https://github.com/zed-industries/zed/tree/2c335b81dc94d109bfcf05e8e54aba208837b33a).

Zed's
[`HeadlessAppContext::capture_screenshot`](https://github.com/zed-industries/zed/blob/2c335b81dc94d109bfcf05e8e54aba208837b33a/crates/gpui/src/app/headless_app_context.rs#L155-L174)
is a capture primitive: the caller drives the window, waits for readiness,
asserts behavior, and saves the returned image.

Zed has a separate
[macOS visual-test runner](https://github.com/zed-industries/zed/blob/2c335b81dc94d109bfcf05e8e54aba208837b33a/crates/zed/src/visual_test_runner.rs#L1-L40).
It creates a stateless app and temporary project, loads real fonts and assets,
and exercises UI states
([setup](https://github.com/zed-industries/zed/blob/2c335b81dc94d109bfcf05e8e54aba208837b33a/crates/zed/src/visual_test_runner.rs#L46-L81),
[initialization](https://github.com/zed-industries/zed/blob/2c335b81dc94d109bfcf05e8e54aba208837b33a/crates/zed/src/visual_test_runner.rs#L149-L190),
[cases](https://github.com/zed-industries/zed/blob/2c335b81dc94d109bfcf05e8e54aba208837b33a/crates/zed/src/visual_test_runner.rs#L389-L435)).
Its
[`VisualTestAppContext`](https://github.com/zed-industries/zed/blob/2c335b81dc94d109bfcf05e8e54aba208837b33a/crates/gpui/src/app/visual_test_context.rs#L19-L120)
uses the real macOS platform and a controllable test dispatcher; it
[offers input simulation](https://github.com/zed-industries/zed/blob/2c335b81dc94d109bfcf05e8e54aba208837b33a/crates/gpui/src/app/visual_test_context.rs#L225-L341)
and captures the rendered texture
([implementation](https://github.com/zed-industries/zed/blob/2c335b81dc94d109bfcf05e8e54aba208837b33a/crates/gpui/src/app/visual_test_context.rs#L379-L387)).
The runner saves actual images, optionally updates baselines, and compares at
least 99% of pixels with a two-unit per-channel tolerance, saving diffs on
mismatch
([implementation](https://github.com/zed-industries/zed/blob/2c335b81dc94d109bfcf05e8e54aba208837b33a/crates/zed/src/visual_test_runner.rs#L684-L832),
[threshold](https://github.com/zed-industries/zed/blob/2c335b81dc94d109bfcf05e8e54aba208837b33a/crates/zed/src/visual_test_runner.rs#L132-L143)).
Another
[visual-test utility module](https://github.com/zed-industries/zed/blob/2c335b81dc94d109bfcf05e8e54aba208837b33a/crates/zed/src/zed/visual_tests.rs#L159-L327)
has configurable comparison and ignored screenshot tests. Zed's
[normal test workflow](https://github.com/zed-industries/zed/blob/2c335b81dc94d109bfcf05e8e54aba208837b33a/.github/workflows/run_tests.yml)
does not invoke the visual runner at this revision; its declared
[baseline directory](https://github.com/zed-industries/zed/blob/2c335b81dc94d109bfcf05e8e54aba208837b33a/crates/zed/src/visual_test_runner.rs#L132-L139)
is absent from the repository tree. Do not assume Zed requires visual
regression tests in CI.

Kver now uses a
[main-thread opt-in capture tool](../../tests/visual.rs) rather than a
scenario language. Its baseline opens the same
[reader](../../src/reader.rs) as the GUI, waits for the first rendered page,
and saves a PNG for agent inspection. Agents can temporarily add interactions
and captures for the feature under development.
The [CI workflow](../../.github/workflows/ci.yml) no longer runs this
exploratory capture target; permanent loading, replacement, and navigation
regressions belong in ordinary Rust or GPUI Kit context tests. Retain the
capture target as an opt-in tool for inspecting real pixels, and restore its
baseline after each investigation. Avoid cross-platform pixel baselines unless
the rendering environment and fonts are pinned separately for each platform.
