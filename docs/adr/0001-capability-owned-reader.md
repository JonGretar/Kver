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
owns the current immutable PDF baseline and its loading lifetime; readers independently
own navigation, zoom, reading mode, text selection, focus, and tool presentation.
This extends the original per-reader document ownership to prevent windows of
one PDF from processing divergent baselines. Sharing only metadata while
rendering and text tools reread the file would not provide that guarantee.
The session registry is weak: closing or replacing one reader detaches only
that participant, and the next open after the final detachment loads afresh.
Relative paths and symlinks join; copies and distinct hard-link paths do not.

Find, Contents, Annotations, marks, and Recent own their retained interaction and asynchronous
work where their lifetimes differ. Navigation geometry, selection rules, and
raster scheduling use ordinary Rust state rather than additional entities.
The reader remains the window-local document-generation authority; cooperating
tools receive the shared baseline with that generation and reject obsolete
replies. Background work never owns session membership or keeps a closed reader
alive. Raster tickets also reject obsolete zoom/density work. All persistent
reading data uses the same serialized history worker, separate from the PDF
baseline. The session also owns one shared, lazy annotation catalog, annotation
revisions, annotation undo/redo, dirty state, and serialized background Save/Save As operations.
Annotations panels retain window-local list selection and extracted excerpts,
not competing document models. Backend-neutral, epoch-scoped annotation
identities let imported and newly authored annotations use the same representation.
Save and Save As install a new immutable baseline only after the file transaction succeeds;
all participating readers use it without sharing viewport or text selection.
Only Save As changes the shared path. A completed save records its captured
catalog, not newer edits. Undo history survives saving and restores document
content without restoring window-local interaction. Dirty state compares content
with that saved catalog; monotonic revisions separately invalidate obsolete work.
Windows own file prompts and Save/Discard/Cancel continuations; the session owns
the changes those decisions affect. In-place Save verifies the captured file version
through the commit path rather than trusting a preflight alone. The session also
owns external-change probes and transactional baseline replacement on reload.
Dirty conflicts retain Kver's version until Save As or deliberate discard-and-reload;
there is no automatic merge. Successful external reload advances the annotation
epoch and clears edit history, while each reader advances its document generation
and restores its own viewport anchor. This invalidates obsolete tool and annotation
references rather than attempting to reattach them across another writer's rebuild.

This favors local ownership over spreading methods on one large entity across
files, global `views`/`models` directories, or one crate per feature. It adds
explicit event and action routing, but avoids duplicate document state and
unrelated lifecycle coupling. GPUI remains the action-dispatch mechanism; there
is no second command bus. Compatibility re-exports preserve existing callers.

See the [completed modularization plan](../plans/completed/reader-modularization.md)
for the alternatives and migration intent, and [architecture](../architecture.md)
for current owners.
