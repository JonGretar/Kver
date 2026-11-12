---
status: accepted
---

# Separate window composition from reader capabilities

The original reader entity mixed document processing, viewport rules, window
chrome, and independent tool lifecycles. Keep one Cargo package, but organize
modules by capability: the application owns startup and keymap policy, each
document window owns composition and focus routing, and its reader owns the
active session participation and central PDF surface.

Repeated opens join one document session by canonical local path. The session
owns the immutable PDF baseline and its loading lifetime; readers independently
own navigation, zoom, reading mode, text selection, focus, and tool presentation.
This extends the original per-reader document ownership to prevent windows of
one PDF from processing divergent baselines. Sharing only metadata while
rendering and text tools reread the file would not provide that guarantee.
The session registry is weak: closing or replacing one reader detaches only
that participant, and the next open after the final detachment loads afresh.
Relative paths and symlinks join; copies and distinct hard-link paths do not.

Find, Contents, marks, and Recent own their retained interaction and asynchronous
work where their lifetimes differ. Navigation geometry, selection rules, and
raster scheduling use ordinary Rust state rather than additional entities.
The reader remains the window-local document-generation authority; cooperating
tools receive the shared baseline with that generation and reject obsolete
replies. Background work never owns session membership or keeps a closed reader
alive. Raster tickets also reject obsolete zoom/density work. All persistent
reading data uses the same serialized history worker, separate from the PDF
baseline. External-change reload and annotation editing are not implemented by
this session boundary.

This favors local ownership over spreading methods on one large entity across
files, global `views`/`models` directories, or one crate per feature. It adds
explicit event and action routing, but avoids duplicate document state and
unrelated lifecycle coupling. GPUI remains the action-dispatch mechanism; there
is no second command bus. Compatibility re-exports preserve existing callers.

See the [completed modularization plan](../plans/completed/reader-modularization.md)
for the alternatives and migration intent, and [architecture](../architecture.md)
for current owners.
