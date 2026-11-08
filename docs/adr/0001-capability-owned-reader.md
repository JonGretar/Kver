---
status: accepted
---

# Separate window composition from reader capabilities

The original reader entity mixed document processing, viewport rules, window
chrome, and independent tool lifecycles. Keep one Cargo package, but organize
modules by capability: the application owns startup and keymap policy, each
document window owns composition and focus routing, and its reader owns the
active document and central PDF surface.

Find, Contents, marks, and Recent own their retained interaction and asynchronous
work where their lifetimes differ. Navigation geometry, selection rules, and
raster scheduling use ordinary Rust state rather than additional entities.
The reader remains the document-generation authority; cooperating tools receive
snapshots and reject obsolete replies. All persistent reading data uses the same
serialized history worker.

This favors local ownership over spreading methods on one large entity across
files, global `views`/`models` directories, or one crate per feature. It adds
explicit event and action routing, but avoids duplicate document state and
unrelated lifecycle coupling. GPUI remains the action-dispatch mechanism; there
is no second command bus. Compatibility re-exports preserve existing callers.

See the [completed modularization plan](../plans/completed/reader-modularization.md)
for the alternatives and migration intent, and [architecture](../architecture.md)
for current owners.
