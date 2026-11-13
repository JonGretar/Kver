# Feature workflow

Use Delta-managed isolation when implementation runs in parallel. A Delta
worktree is a separate checkout for that thread; an additional Git worktree
is not part of the routine workflow. Git and `jj` are both acceptable for
version-control operations in this repository.

Keep working conversations and requests for human checks in the Delta thread;
ask the person directly there, launching the app yourself when possible.
Put brief verification results and remaining limitations in the relevant
GitHub issue or PR, not in architecture docs or a test diary.

1. **Decide what to build.** Use the agreed request or issue for scope;
   `docs/roadmap.md` supplies broad future direction, not live status.
   For a multi-session feature, agree on a spec
   and small, verifiable tickets before publishing them to
   [GitHub Issues](issue-tracker.md). Use `to-spec` to agree on the scope and
   test seams, then `to-tickets` to propose demoable slices and their genuine
   blockers. Publish only after the relevant approvals. Record settled domain
   terms and qualifying decisions using the [domain docs rules](domain.md).
2. **Choose a worktree.** Continue in the current thread for small serial work.
   For independent features, select **Isolated Delta Worktree** before the
   first message in a new Delta thread. Check its starting revision: it may
   not include another thread's unlanded work. When delegating parallel work
   that needs the current thread's changes, use isolated subthreads. Each
   subthread starts with the parent's work but keeps its later edits private
   until the parent explicitly pulls them with `merge_worktrees`. Independent
   threads do not use that pull; bring their changes together through reviewed
   branches and PRs instead.
   In the first prompt to a newly created implementation thread, tell it to
   do the work in that thread; do not instruct it to create another thread.
   Ask working threads to report decisions needed, blockers that change the
   plan, and confirmed landings or dependency changes to the orchestrator.
   Keep routine implementation and CI progress in the working thread.
3. **Build a slice.** Implement one usable behavior per ticket. Follow the
   applicable project skills (such as `tdd` for test-first work), run the
   checks relevant to that behavior, and demonstrate it with a real PDF where
   UI or rendering behavior matters. Local and Cloud Runner threads may have
   different machines and tools; use the checks available in each, and briefly
   report what ran in the issue or PR. PR CI runs Rust and GPUI Kit in-process
   tests and builds the GUI binary on Linux and macOS. These checks do not
   validate a visible window, screenshot pixels, or OS-native pointer behavior.
   For changes to native window/input behavior, confirm a real-Mac manual check
   before landing and record its outcome in the
   issue or PR; a Linux Cloud Runner cannot substitute for that check. Before
   a downloadable release, validate launch and reading behavior in a real Mac
   window. Do not require a local Mac for every routine UI PR.
   For visible UI changes, use the opt-in `tests/visual.rs` tool to capture a
   representative state and show the image in the Delta thread when requesting
   visual feedback or approval. A file path alone does not show the result.
   Temporarily adapt the tool as needed, then run
   `corgi test --features screenshot --test visual` on macOS (or
   `cargo +stable test --locked --features screenshot --test visual` on Linux).
   Captures go under ignored `target/`; attach the image or use a link the
   reviewer can open, and check that it renders. If capture is unavailable,
   say what was not visually checked. Before finishing, restore the capture
   baseline while preserving other work. Put lasting behavioral regressions
   in ordinary Rust or `gpui_kit::test` tests; keep capture opt-in, not a PR gate.
4. **Review, then land.** Inspect **Review Changes** in Delta and resolve
   feedback. Implementation threads create PRs only when explicitly instructed;
   orchestrators should not ask them to create PRs by default. During an
   explicitly requested landing, the Land skill creates a PR if needed; an
   existing PR can also be used. Require passing Linux and macOS PR checks
   before landing, plus the real-Mac check
   for native window/input changes. Before tagging a release, run the
   unsigned archive verification in `scripts/package-macos-app.sh` on an Apple
   silicon Mac; see [macOS releases](../releasing.md) for the full procedure.
   An independent thread's changes reach the project through
   version control; an isolated subthread's changes require an explicit worktree
   pull into its parent. Landing on `main` requires approval for that specific
   change.
   Put `#<ticket number>` in commits for ticketed work. In the PR body, use
   `Closes #<number>` only when merging that PR will satisfy the ticket's
   acceptance criteria; otherwise use `Refs #<number>` and keep the issue
   open. Do not auto-close a parent spec when only one child ticket is done.
   Apply the [documentation lifecycle](#documentation-lifecycle): update only
   affected durable guidance and remove finished working files.
   Add a user-facing `CHANGELOG.md` entry for notable changes.

## Documentation lifecycle

Maintain the smallest useful set of durable documents. Before adding or
updating prose, identify its reader, purpose, and authoritative home:

| Information | Home |
| --- | --- |
| Product features, installation, and reader quick start | `README.md` |
| Developer setup, running from source, builds and tests | `docs/development.md` |
| Source map, major ownership boundaries, cross-cutting invariants | `docs/architecture.md` |
| Non-obvious local reason or constraint | Comment beside the enforcing code |
| Behavioral contract | Code and tests |
| Hard-to-reverse, surprising decision with real alternatives | ADR, only if it meets the [criteria](../adr/README.md) |
| Reusable user or maintainer procedure | Existing focused how-to/reference, such as `docs/keybindings.md` or `docs/releasing.md` |
| Broad future direction | `docs/roadmap.md` |
| Scope, acceptance criteria, unresolved limitations, live status | Issue/spec |
| Implementation discussion and verification results | PR or issue |
| Notable reader-facing changes | `CHANGELOG.md` |

Code comments should explain the essential reason locally; link to an issue
or PR for supporting history. A link alone is not an explanation. Prefer
reading the source over maintaining prose inventories of fields, shortcuts,
UI details, or feature behavior.

Temporary plans and research are welcome when they help execute the task.
Use `docs/plans/` or `docs/research/`, identify the active task, and keep them
only while needed. At completion or abandonment:

1. Check for vital information not already captured in its authoritative home.
2. Move only that information: a constraint to a comment/test, a qualifying
   decision to an ADR, a reusable procedure to a how-to, or unresolved work to
   the relevant issue (publishing still requires approval).
3. Delete the working files and fix their incoming links. Revision history
   preserves the original material; do not create a completed-plan archive.

Research does not automatically warrant an ADR. Keep test observations and
release acceptance evidence in issues/PRs rather than maintained validation
diaries. Finishing a feature does not require architecture or roadmap edits
unless their actual purpose is affected.

### Research and discussion history

Research normally stays in the working conversation. Use primary sources and
distinguish verified findings from proposals and untested assumptions. When
delegation is permitted and an independent investigation benefits from a
separate context, use the Scout subagent. A fork or separate thread is not
required merely to produce a research record.

ADRs are the source of truth for the engineering decisions they record.
Include the essential context, decision, alternatives, and consequences in
the ADR itself, following the existing ADR criteria. An ADR may link to a
Delta passage for optional background, but must remain understandable without
opening it. Keep other vital information in its authoritative home from the
table above, never solely in a thread.

Delta links are exceptional supporting references, not routine task metadata.
Add one only for consequential reasoning or evidence that would be costly to
reconstruct and is not already adequately captured elsewhere. Prefer links in
issues and PRs; source comments should normally reference those instead.
Use a focused passage link and check intended readers' access. Leave sharing
permissions unchanged unless explicitly authorized. No thread indexes or
per-feature link requirements.

## Modal command lists

Use Kit's modal behavior for searchable lists, pickers, and their confirmation
dialogs. A focus trap alone does not block pointer events from reaching the
reader underneath, implement outside-click dismissal, or repair focus when
the focused control disappears.

Choose the owner before composing the surface:

- Use the styled `Dialog` through `WindowExt::open_dialog` for a window-managed
  modal, as in [Open Recent](../../src/reading_history/recent_picker.rs).
- For a document-owned surface whose lifetime must follow its entity, use
  `gpui_kit::base::Dialog` with `DialogPopup` and supply the presentation, as in
  [local marks](../../src/reader/marks.rs). Keep open state with that owner;
  unmounting it must not close an unrelated dialog on the window's stack.

Both choices reuse Kit's dismissal, focus-trapping, and pointer behavior.
Keep application-specific list operations separate from that modal behavior;
prefer these existing primitives over another application popup framework.
Follow the GPUI Kit coding and design guides for their current interfaces.

When a row action removes its own focused button, explicitly focus a surviving
control, usually the search field, before removing the row. After a nested
confirmation closes, restore the parent list's intended keyboard target rather
than a disappearing trigger. If restoration waits for another frame, use a weak
owner and check that the same popup is still open before moving focus.

Test the composed window with real pointer and keyboard events:

- Click search and non-input parts of the surface, then type and use arrows.
- Delete a row, including the final row, then continue typing and navigating.
- Cancel and confirm nested dialogs; check focus returns to the parent list.
- Click the backdrop and use Escape; check the intended layer dismisses and
  focus returns to a valid target.
- Replace the document or supersede the popup during pending work; late replies
  must not reopen it or steal focus.

Typing, arrow navigation, and incidental clicks within a modal must not reach
the PDF behind it. See the
[marks modal regressions](../../src/reader/tests/marks/modal.rs) and
the [Recent workflows](../../src/app/document_window/tests/recent_picker.rs).
Apply the native-Mac validation requirement above; screenshots alone cannot
establish pointer routing or keyboard ownership.

## Work tracking

GitHub Issues is the source of truth for **what remains**, **what is active**,
and **what depends on what**. A spec issue describes the agreed scope; its
implementation tickets are separate issues, linked as sub-issues where
supported. Do not count the spec as an implementation ticket. Use issues
for the task graph and live ownership, not a repository status summary.

For a quick "what next?" answer, inspect open issues and the relevant spec's
open child issues. Check each candidate's native blockers (or
`Blocked by` text), assignee, start comments, and linked PR before calling it
ready. Report ready, in-progress, and blocked tickets with links, then suggest
unclaimed work within the agreed scope. If GitHub cannot be checked,
say that current ownership and blockers are unverified rather than guessing.
Explore code only when deciding how to implement a particular ticket.

- **Blocked:** An open ticket with an unfinished prerequisite. Record each
  actual prerequisite as a native `blocked by` dependency where available,
  otherwise list `Blocked by: #<number>` in the ticket. A shared file or likely
  edit conflict is a coordination concern, not automatically a dependency.
- **Ready:** An open, unclaimed implementation ticket whose prerequisites are
  complete. The `ready-for-agent` label means the ticket is *specified*, not
  that work has begun; a blocked ticket may still carry it.
- **In progress:** Before starting, check the blockers, claim the ticket with
  an assignee when possible, and add a brief start comment identifying the
  working thread or branch. Link the implementation PR to the ticket when
  opened. Coordinate with the owner before starting another implementation
  of an active ticket.
- **Done:** Close the ticket only after its acceptance criteria are met and
  its change is landed. Link the landing PR or other evidence in the ticket.
  A passing PR or an unmerged Delta worktree is not done.

When reporting status, group open implementation tickets into **in progress**,
**ready**, and **blocked**, and show the unfinished blocker numbers. Include
closed tickets only when explaining completed prerequisites. If a ticket has
no reliable start record, treat it as unclaimed rather than guessing from
Delta activity. An active ticket stays **in progress** even if a new blocker
appears; report that blocker alongside it. Work on independent ready tickets
can proceed in parallel; sequence tickets that truly need the same unfinished
behavior. Recheck the frontier when a blocker lands, and update a ticket's
edges if the implementation reveals a real new dependency.

Delta's continuous file history is not a commit or a merge. Keep plans,
tickets, tests, and the reviewed change distinct so each can be checked on
its own terms.

## Changelog and releases

Follow [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/).
Write notable changes under `## [Unreleased]`, using only relevant categories
such as `Added`, `Changed`, and `Fixed`. Describe the change from a reader's
perspective, not as a list of commits; omit routine internal work. The changelog is meant for the users of the app to see changes. Avoid lengthy technical information or instructions unless it's valuable for the user.

Prepare or publish a release only when explicitly requested. Follow
[macOS packaging and releases](../releasing.md) for versioning, signing,
candidate checks, and publication. Creating or pushing a release tag requires
authorization for that release.
