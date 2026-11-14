# Architecture

Kver is a native Rust PDF reader using GPUI Kit for the UI, Hayro for PDF
rendering and extraction, and a separate `lopdf` writer for annotation edits.
SQLite stores reading history; TOML stores application preferences.

This is a map to the source, not a feature specification. Read the relevant
module and its tests for behavior. The [ownership ADR](adr/0001-capability-owned-reader.md)
explains the main structural trade-offs; [GLOSSARY.md](../GLOSSARY.md) defines
domain terms.

## Where to start

| Area | Source entry points |
| --- | --- |
| Startup and OS entry points | [`main.rs`](../src/main.rs), [`app.rs`](../src/app.rs) |
| Window lifecycle and native menus | [`app/windows.rs`](../src/app/windows.rs), [`app/menu.rs`](../src/app/menu.rs) |
| Keybinding policy and command contexts | [`app/keymap.rs`](../src/app/keymap.rs), [`app/keymap/config.rs`](../src/app/keymap/config.rs), [`key_context.rs`](../src/key_context.rs) |
| Window composition, chrome, sidebar, palette, and file prompts | [`app/document_window.rs`](../src/app/document_window.rs) and its child modules |
| Central PDF surface and window-local reading state | [`reader.rs`](../src/reader.rs) |
| Shared PDF baseline, annotation edits, undo, saving, and reload | [`reader/document_session.rs`](../src/reader/document_session.rs) and its child modules |
| Layout, zoom anchors, and navigation geometry | [`reader/navigation.rs`](../src/reader/navigation.rs) |
| Raster scheduling and bitmap retention | [`reader/rendering.rs`](../src/reader/rendering.rs) |
| Shared page-text access and window-local reading ruler | [`reader/page_text.rs`](../src/reader/page_text.rs), [`reader/ruler.rs`](../src/reader/ruler.rs) |
| Text selection, links, and on-page annotation targeting | [`reader/selection.rs`](../src/reader/selection.rs), [`reader/links.rs`](../src/reader/links.rs), [`reader/page_annotations.rs`](../src/reader/page_annotations.rs) |
| Find, Contents, annotations list, and local marks | [`reader/find.rs`](../src/reader/find.rs), [`reader/contents.rs`](../src/reader/contents.rs), [`reader/annotations.rs`](../src/reader/annotations.rs), [`reader/marks.rs`](../src/reader/marks.rs) |
| Back/Forward jump history | [`app/document_window/jumplist.rs`](../src/app/document_window/jumplist.rs) |
| PDF backend boundary and text layout | [`pdf.rs`](../src/pdf.rs), [`pdf/text_layout.rs`](../src/pdf/text_layout.rs) |
| Annotation representation, reading, and encoding | [`pdf/annotations.rs`](../src/pdf/annotations.rs), [`pdf/annotation_read.rs`](../src/pdf/annotation_read.rs), [`pdf/writer.rs`](../src/pdf/writer.rs) |
| Coordinate transforms | [`coordinates.rs`](../src/coordinates.rs) |
| Preferences, paths, and Settings UI | [`settings.rs`](../src/settings.rs), [`settings_ui.rs`](../src/settings_ui.rs) |
| Reading history, serialized storage, and Open Recent | [`reading_history.rs`](../src/reading_history.rs) and its child modules |
| macOS quit protection and updates | [`app/quit_guard.rs`](../src/app/quit_guard.rs), [`updater.rs`](../src/updater.rs) |

## Ownership boundaries

- **Application:** startup, shared settings/history services, keymap policy,
  menus, and window creation.
- **Document window:** composition, focus routing, file prompts, sidebar,
  palette, and its jumplist.
- **Document session:** shared immutable PDF baseline, annotation catalog,
  editing history, dirty state, and serialized save/reload operations.
  Readers opening the same canonical path join the same session.
- **Reader:** window-local navigation, zoom, reading mode, ruler, selection, and
  viewport resources. Closing one reader does not discard another's session.
  Selection and ruler share bounded background page-text access; each retains
  only its active text page independently of the viewport cache.
- **Tools:** Find, Contents, Annotations, marks, and Recent own their retained
  interaction and asynchronous lifetimes, rather than storing them in the
  window compositor.

Navigation, selection, and raster rules use ordinary Rust state; retained
UI/workflow owners use GPUI entities. Commands are GPUI Actions handled by
their owner; keys, menus, and controls share that path.

## Cross-cutting constraints

- PDF parsing, extraction, rasterization, encoding, and filesystem work run
  off the UI thread. Backend-specific types stay behind the PDF adapter.
- Background consumers use immutable document snapshots. Weak ownership,
  document generations, tool revisions, and raster tickets prevent obsolete
  work from changing a replacement document or keeping a closed reader alive.
- Rendering is viewport-driven and memory-bounded, not eager document-wide
  rasterization. See `reader/rendering.rs` for the actual retention policy.
- Persistent geometry uses PDF points, not viewport pixels. Coordinate
  transforms derive cropped/rotated page and screen positions.
- Reading-history privacy/deletion is separate from authored PDF changes.
  Saving annotations belongs to the shared document session, not SQLite.

## Tests and operations

Pure and feature-local tests live beside their modules. Composed reader
contracts are under [`reader/tests/`](../src/reader/tests/); window routing
and focus workflows are under
[`app/document_window/tests/`](../src/app/document_window/tests/).
[`tests/visual.rs`](../tests/visual.rs) is an opt-in pixel capture tool, not
the behavioral test suite.

See the [workflow](agents/workflow.md) for validation policy and
[macOS release guide](releasing.md) for packaging, updates, and native quit
checks. Known bugs and acceptance evidence belong in issues and PRs.
