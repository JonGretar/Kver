# GPUI and GPUI Kit organization for the next reader gate

Research for [reader-experience spec #33](https://github.com/JonGretar/Kver/issues/33).
This is design input, not an approved restructuring plan. References to Kit
target the published **0.8.1** documentation; check the exact version and
compiled integration again before changing dependencies.

## What the upstream interfaces actually provide

- [GPUI Kit 0.8.1's published manifest](https://docs.rs/crate/gpui-kit/0.8.1/source/Cargo.toml)
  pins `gpui-pre` and `gpui-pre-platform` to `=0.3.10`; Kver currently pins
  [the same GPUI snapshot](../../Cargo.toml). That makes Kit a plausible
  compatible addition, **not** proof that Kver's GUI and screenshot targets
  compile unchanged. Kit's default features include styled components and
  assets; its `test-support` feature enables the matching GPUI test features.
- Kit's [Getting Started guide](https://gpui-kit.com/docs/getting-started)
  calls for `application().with_assets(...)`, `init(cx)` before creating
  component views, and `open_window(...)` to wrap the content view in a
  `Root` for overlays. Retained `Entity` state belongs to the owning view;
  `RenderOnce` values describe reusable presentation built each render.
- Kit's [Coding Guides](https://gpui-kit.com/docs/coding-guides) advise
  grouping larger capabilities by feature, not global `models/`, `views/`,
  and `commands/` directories. Their example uses feature crates; Kver can
  keep feature modules in one crate until independently owned capabilities
  justify a workspace split.
- GPUI's [key dispatch guide](https://github.com/zed-industries/zed/blob/main/crates/gpui/docs/key_dispatch.md)
  and Kit's [Action guide](https://gpui-kit.com/docs/action) put a typed Action
  on the focused dispatch path. Key contexts select bindings, not handlers.
  Buttons can dispatch to a saved focus handle; application menus can use
  the same Action. The target window/focus matters for per-window state.
- Kit's [Command guide](https://gpui-kit.com/component/command) describes
  `Command` entries built during rendering and persistent `CommandState` for
  query/focus/selection. `CommandItem::action` dispatches a typed Action and
  resolves its current shortcut hint; disabled items are noninteractive. This
  supports an app-action-only palette without a second command executor.

## Kver's current seams and pressure points

- [`pdf.rs`](../../src/pdf.rs) already hides Hayro and returns dimensions,
  transform, or display pixels. Every raster request rereads and reparses
  the file. The new text capability should extend this PDF-facing seam with
  positioned glyphs in document coordinates, rather than hand Hayro types
  to GPUI or expose an experimental backend callback as the interface.
- [`coordinates.rs`](../../src/coordinates.rs) is independent of GPUI and
  Hayro; keep it that way. Connect actual page placement and scroll to
  `PageViewport` for pointer hit-testing before claiming selection works.
- [`reader.rs`](../../src/reader.rs) currently owns document lifecycle,
  navigation/fit logic, bitmap budget and scheduling, GPUI focus/input, file
  picker, and the toolbar. In particular, direct key handling and direct
  button callbacks invoke navigation methods independently. Adding menu,
  palette, multiwindow, and selection here without reorganizing would
  multiply entry points and state invalidation paths.
- [`main.rs`](../../src/main.rs) opens one window and quits when the last
  closes. Future New Window/Next Window behavior needs one app-level owner
  for window creation and focus switching, while each reader window keeps
  its own document, scroll, rendering, and selection state.
- [`tests/visual.rs`](../../tests/visual.rs) already drives real GPUI events
  in headless windows. Preserve those behavior-level tests while adding
  two-entry-point Action tests and actual pointer/clipboard tests. As the
  [spec](https://github.com/JonGretar/Kver/issues/33) states, native menu,
  focus switching, and drag/copy still require real-Mac checks.

## Design consequence to evaluate

Prefer **one reader-window owner** with a small set of document/reading
operations, a separate PDF-facing module for expensive page/text work, and
an app shell for global menu and window lifecycle. The UI toolkit should own
controls, focus, palette, themes, and overlays—not PDF selection geometry or
the meaning of navigation commands. Split source files by cohesive
capability as that implementation arrives, not into many empty crates in
advance. Keep the selection overlay/transient anchors beside reading state,
while its extracted text and interpretation stay behind the PDF seam.

An early executable Kit compatibility spike should verify **one** GPUI
dependency tree, initialized themes/assets/Root, test-support under Kver's
headless target, and an ordinary window on a Mac before the UI refresh relies
on the integration.
