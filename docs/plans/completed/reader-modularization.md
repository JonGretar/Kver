# Reader modularization

**Status: implementation complete; historical plan.** This is not an active
task list. Use the [architecture](../../architecture.md) for current structure and
the [ownership ADR](../../adr/0001-capability-owned-reader.md) for the settled
decision. Implementation completion does not imply landing or release approval.

## Purpose and scope

Restructure Kver so application policy, window composition, and PDF-reading
behavior have clear owners. Keep one Cargo package and preserve existing
reader behavior and presentation.

This plan records the design discussed with JonGretar and the original
implementation sequence. The sections below preserve the intent, alternatives,
and acceptance criteria, not a description of the current architecture.

Use this file for durable intent and acceptance criteria. Keep verification
results and review discussion in the thread and, for ticketed work, the
relevant GitHub issue or PR. GitHub Issues remain the source of live ticket
status. Creating this plan does not authorize publishing tickets, opening a
PR, landing changes, or creating a release.

## Why change the structure

At planning time, `src/reader.rs` contains approximately 6,500 lines, including
3,200 lines of tests. Its `Preview` entity owns the document, viewport,
raster scheduling, selection, persistence restoration, command palette,
marks, recents, and sidebar coordination. Its renderer exceeds 1,000 lines.

Some responsibilities already have files but not independent ownership.
For example, `find_panel.rs` holds search state while `reader.rs` runs
search batches and mutates that state. Moving more methods onto the same
entity in different files would reduce file size without resolving this
coupling.

Success means a search change no longer requires understanding recent-file
dialogs or raster scheduling. File size is a diagnostic, not a fixed quota.

## Agreed design

### Application, document window, and reader

| Owner | Responsibilities | Excludes |
| --- | --- | --- |
| Application | Startup, OS open/reopen events, shared settings/history initialization, native menus, keymap policy, window creation | Current page, search results, selection |
| Document window | Title bar, workspace composition, sidebar host, palette, window commands, routing file-open requests and focus between regions | Search batching, bitmap eviction, mark storage |
| Reader view | Central PDF surface, page layout, navigation, zoom, selection, visible-page rendering | Application startup, window cycling, recent-file browsing |

The reader **module** contains the broader PDF-reading capability, including
Find, Contents, and marks. `ReaderView` names the narrower central PDF
surface. Replace `Preview` with that name as the responsibility is extracted;
do not merely rename the existing all-purpose entity.

The document window must remain a composition owner, not become the old
reader implementation under another name. Extend the existing window
lifecycle implementation rather than introduce a parallel mechanism.
Keep one GPUI Kit `Root` per window for overlay and focus facilities.

### Feature-local ownership

- **Find:** owns the query, results, progress, active match, limits, task
  lifecycle, and query revision. It reports a document-space destination
  when a result is activated. The viewport receives highlight presentation
  data rather than reaching into Find's private state through the sidebar.
- **Contents:** owns outline loading, tree state, selection, and load/error
  presentation. It reports destinations; the reader applies them to its
  page, zoom, and scroll model.
- **Marks:** owns window-local mark interaction, including listing, naming,
  letter selection, and popup focus. Saved and session mark data remain
  with the existing shared persistence owner. Setting a mark captures a
  reader position; jumping uses the reader's navigation interface.
- **Sidebar host:** belongs to the document window. It owns width,
  visibility, panel switching, and focus transfer, but not search or outline
  state. Use a concrete host for known panels, not a plugin framework.
- **Recent picker:** owns its query, rows, loading/errors, and forget/open
  interactions. It reports an open request to the document window. It does
  not implement PDF loading or become a second history store.

Window layout state survives document replacement. Document-specific state
resets when the PDF changes. There must be one authoritative active-document
identity per window, supplied to cooperating features with a revision that
lets them reject obsolete work. Do not introduce independently mutable
copies of the current path or viewport.

Select the concrete document-lifecycle owner and the narrow destination
interface during extraction, before moving fields. Search, marks, and outline
navigation can retain their distinct placement/zoom policies while sharing
coordinate conversion and viewport mutation.

### Actions, contexts, and keymaps

Use GPUI's existing Action and dispatch mechanism, not a second command bus.

- A feature defines the actions it implements.
- `app/keymap.rs` assembles physical bindings and context expressions.
- The appropriate application, window, or feature owner handles the action.
- Focused views declare their contexts and attach their handlers.
- Menus, toolbar controls, palette entries, and keys invoke the same actions.

Examples: Quit is application-owned; Close Window is window-owned; Next Page
is reader-owned; Next Match is Find-owned. Open PDF can retain window-local
handling plus an application fallback when no document window exists.

Initially keep default bindings together in the keymap module, grouped by
capability. GPUI continues to own raw key dispatch, context matching, and
multi-keystroke sequences. Preserve action identities, context semantics,
and shortcut precedence when moving declarations.

The keymap module is the future home for user-override validation and
installation. Custom binding syntax, precedence, unbinding, and hot reload
are separate feature work. Do not implement them in this refactor. Register
bindings before native menus; future reloads must refresh menu shortcuts.

### Shared persistence and dependencies

The current `recent.rs` owns document identity, positions, reading modes,
and marks as well as recent paths. Rename that capability to
`reading_history` when extracting its presentation and internals.

Preserve its shared serialized worker, session-only behavior, schema,
cascading deletion, and protection against obsolete opens resurrecting
forgotten data. Do not create per-panel stores or workers.

Dependency direction should remain:

- Application composition depends on feature interfaces.
- Reader features depend on the PDF adapter, coordinates, settings, and
  history interfaces as needed, not on the application shell.
- The sidebar host composes panels through deliberate interfaces, not their
  mutable internals.
- PDF processing does not depend on GPUI presentation or expose Hayro types
  to the UI.

Keep pure rules in ordinary Rust types and functions. Use entities for
independent retained lifecycles, subscriptions, or async workflows; use
render helpers or `RenderOnce` for presentation without such a lifecycle.
Retain subscription/task handles where their lifetime belongs and avoid
strong parent-child ownership cycles. Do not turn every file into an entity.

## Target layout

This is a guide to ownership, not a requirement to create empty files.
Small action sets or cohesive features can remain in their entry module.

```text
src/
├── main.rs
├── lib.rs
├── app.rs
├── app/
│   ├── windows.rs
│   ├── keymap.rs
│   ├── commands.rs
│   ├── menu.rs
│   ├── document_window.rs
│   └── document_window/
│       ├── commands.rs
│       ├── sidebar.rs
│       └── command_palette.rs
├── reader.rs
├── reader/
│   ├── view.rs
│   ├── commands.rs
│   ├── navigation.rs
│   ├── rendering.rs
│   ├── selection.rs
│   ├── find.rs
│   ├── find/
│   │   └── matching.rs
│   ├── contents.rs
│   └── marks.rs
├── reading_history.rs
├── reading_history/
│   ├── store.rs
│   ├── worker.rs
│   └── recent_picker.rs
├── settings.rs
├── settings_ui.rs
├── pdf.rs
└── coordinates.rs
```

Unrelated modules, such as assets and updates, are omitted from this diagram;
their omission does not propose deletion or mandatory relocation.

Use named entry files such as `reader.rs` with private child modules. Expose
small interfaces and deliberate re-exports rather than internal folder
structure. Avoid global `models/`, `views/`, or `controllers/` directories.
No additional crates, major dependencies, generic event framework, or
repository-wide cleanup are needed.

## Test organization

- Keep small pure tests beside the implementation.
- Move substantial suites into adjacent test modules, grouped by behavior.
- Place cross-feature window workflows beside the document-window owner.
- Use repository-root `tests/` for tests that naturally exercise the
  library's public interface, not simply because a test uses GPUI.
- Do not widen production visibility just to relocate tests.

For example, `#[cfg(test)] mod tests;` in `reader.rs` can load
`reader/tests.rs`, which declares suites under `reader/tests/`. Those remain
crate-internal tests. Preserve assertions and behavioral coverage while
changing their location. Share fixture construction where useful, without
replacing real input workflows with helpers that bypass behavior under test.

## Implementation sequence

Each stage should leave a buildable, reviewable result. Keep mechanical moves
distinct from ownership changes where practical. Do not proceed past an
unexplained regression.

Record each coherent, validated slice as a local commit before starting the
next slice. Use the repository's Jujutsu workflow to describe the current
change and start an empty child; keep the refactor on its own bookmark rather
than moving `main`. Local checkpoint commits do not authorize pushing,
opening a PR, or landing the refactor.

### 1. Establish the baseline and organize tests

Inspect current code, instructions, and live changes before beginning. Run
the existing checks, then move relevant reader tests into adjacent files as
each feature is extracted, rather than relocate the entire suite in one
mechanical change. Inventory important input, lifecycle, and persistence
coverage; add missing regression coverage before changing the corresponding
seam. Check each initial test move independently from its ownership refactor.

Acceptance: the test runner still discovers equivalent coverage, assertions
are preserved, and the reorganized suite passes the baseline checks.

### 2. Extract Find as the first complete ownership change

Move query/result state, search execution, and revision handling into Find.
Keep the existing pure matching logic. Define document-change input and
destination/highlight output without exposing mutable state. The current
composition owner may wire this feature temporarily until stage 3.

Acceptance: typing, next/previous match, result activation, highlights,
close/reopen, query replacement, and document replacement work. Old-document
and old-query results cannot update current results or move the reader.

### 3. Separate application and document-window composition

Extract startup composition and keymap policy. Introduce the document-window
owner, moving the sidebar host and palette with it. Give Contents its own
loading and navigation lifecycle before moving the host, so the host does not
retain callbacks into the old reader entity. The sidebar itself owns resize,
panel selection, and focus; its composition owner observes layout changes
that affect PDF fitting. Separate the central
PDF surface and name it `ReaderView`. Keep document loading/restoration
coherent rather than duplicating it for each open entry point.

Acceptance: keys, menu items, palette commands, and toolbar controls still
reach the correct owner. Text input does not trigger bare reading keys.
Sidebar/overlay dismissal restores appropriate focus. Independent windows,
windowless reopening, and OS/file-picker opens retain existing behavior.

### 4. Extract remaining feature owners and history presentation

With Contents decoupled for the window split, extract marks and the recent
picker one at a time. Move and rename history internals while retaining the
existing persistence protocol.

Acceptance: outline navigation, mark set/jump/delete/name, recent search/open/
forget, cross-window invalidation, private sessions, and restart restoration
retain their contracts. Forgotten state cannot be resurrected by old work.

### 5. Separate reader rules from rendering coordination

Extract navigation, selection, and raster scheduling where their invariants
become easier to understand and test. Keep the view focused on composition
and coordination. Preserve the existing PDF adapter; do not introduce parsed
document caching, prefetching, or a new worker architecture.

Acceptance: continuous/single-page modes, zoom anchors, fit on resize,
selection/copy, document-space positions, and long-document behavior remain
correct. Retain visible-page prioritization, bounded bitmap memory, stale
raster rejection, and the existing one-current-raster-job scheduling policy.

### 6. Review the complete workflow and reconcile documentation

Review dependency direction, state ownership, action routing, and public
visibility. Exercise representative PDFs in the real application. Update
`docs/architecture.md` to describe the implemented structure; record settled
terminology and consequential trade-offs following `docs/agents/domain.md`.
Retain this plan as rationale, not as a second current-architecture document.

Acceptance: a reader can find each feature, its owner, and its tests without
following callbacks through unrelated features. No new window-sized
all-purpose entity has replaced the old one.

## Validation and completion

For each stage, run focused tests plus checks appropriate to changed
interfaces. At completion, run formatting, the full test suite, Clippy, and
the normal GUI build. Use the current CI workflow as the command reference.
Also verify the non-GUI configuration remains buildable when moving pure
logic or feature gates.

Follow the root command policy: on macOS use installed Corgi for supported
operations after checking its help; on Linux and in CI use Cargo. Do not
substitute a different check without reporting it or run equivalent Corgi
and Cargo checks just to validate twice.

Specifically exercise:

- Multi-keystroke marks versus ordinary reader shortcuts.
- Text-input focus, sidebar switching, nested overlays, and dismissal.
- Two windows with independent navigation and shared saved state.
- Document replacement during loading, search, outline, text, and raster work.
- Private/session-only history, forgetting, and restart restoration.
- Long/image-only PDFs, fit/zoom/resize, page turns, scrolling, and copying.

Use in-process GPUI interaction tests for dispatch and focus behavior, but
do not treat them as proof of native input behavior. Follow
`docs/agents/workflow.md` for the real-Mac check before landing native
window/input changes and for opt-in visual captures when UI review needs
them. Preserve appearance; any intended UI change needs separate agreement.

Report checks actually run and remaining limitations. Completing a stage
does not authorize a merge. If work spans sessions, agree on ticket slices
and obtain approval before publishing them under the project workflow.

## Using this historical plan

For new work, start with `AGENTS.md`, the current architecture, and the feature
workflow. Use live issues for remaining work rather than resuming this plan's
completed stages or treating its illustrative target tree as a checklist.

When revisiting the design, compare the original rationale here with the
ownership ADR and actual source. The acceptance criteria remain useful
regression scenarios; previous verification does not cover later edits.

## References

- [Current architecture](../../architecture.md)
- [Feature workflow and validation policy](../../agents/workflow.md)
- [Domain documentation policy](../../agents/domain.md)
- [GPUI Kit Coding Guides](https://gpui-kit.com/docs/coding-guides/)
- [Rust test organization](https://doc.rust-lang.org/book/ch11-03-test-organization.html)
