# Feature workflow

Use Delta-managed isolation when implementation runs in parallel. A Delta
worktree is a separate checkout for that thread; an additional Git worktree
is not part of the routine workflow. Git and `jj` are both acceptable for
version-control operations in this repository.

Keep repository Markdown files for durable decisions, their reasons, and
information needed to build or use Kver. Keep working conversations and
requests for human checks in the Delta thread; ask the person directly there,
launching the app yourself when possible. Put brief verification results and
remaining limitations in the relevant GitHub issue or PR, not in architecture
docs or a test diary.

1. **Decide what to build.** Use `docs/roadmap.md` for release gates and
   `docs/features.md` for scope. For a multi-session feature, agree on a spec
   and small, verifiable tickets before publishing them to
   [GitHub Issues](issue-tracker.md). Use `to-spec` to agree on the scope and
   test seams, then `to-tickets` to propose demoable slices and their genuine
   blockers. Publish only after the relevant approvals. Record settled domain
   terms in `GLOSSARY.md` and consequential trade-offs in `docs/adr/`.
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
   existing PR can also be used. While the repository is private, require the
   passing Linux and macOS PR checks before landing, plus the real-Mac check
   for native window/input changes. Before tagging a release, run the
   unsigned archive verification in `scripts/package-macos-app.sh` on an Apple
   silicon Mac. An independent thread's changes reach the project through
   version control; an isolated subthread's changes require an explicit worktree
   pull into its parent. Landing on `main` requires approval for that specific
   change.
   Put `#<ticket number>` in commits for ticketed work. In the PR body, use
   `Closes #<number>` only when merging that PR will satisfy the ticket's
   acceptance criteria; otherwise use `Refs #<number>` and keep the issue
   open. Do not auto-close a parent spec when only one child ticket is done.
   Update project docs when a durable fact changes: check off each
   `docs/roadmap.md` item fully completed by that change, leaving partial
   items unchecked. Update **Where we are** when the milestone status or next
   work changes.
   Add a user-facing `CHANGELOG.md` entry for notable changes.

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
supported. Do not count the spec as an implementation ticket. The roadmap
summarizes milestone outcomes and current state; use issues for the finer
task graph and live ownership. When tickets are published, replace the
roadmap's "no open implementation tickets" note with a link to the spec or
ticket set. Do not mirror each ticket's mutable status in the roadmap.

Keep active implementation plans directly under `docs/plans/`. Retain completed
plans with useful rationale under `docs/plans/completed/`, marked historical
and linked to current architecture or ADRs. Completion of a plan does not imply
that its changes have landed or been released.

For a quick "what next?" answer, read the roadmap's **Where we are** and the
linked spec's open child issues. Check each candidate's native blockers (or
`Blocked by` text), assignee, start comments, and linked PR before calling it
ready. Report ready, in-progress, and blocked tickets with links, then suggest
unclaimed work from the current milestone's ready frontier. The roadmap's
initial task list is a pointer, not live status; if GitHub cannot be checked,
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

Prepare a release only when explicitly requested. Move the released entries
under `## [x.y.z] - YYYY-MM-DD`, leave a new `## [Unreleased]` at the top,
set the package version to `x.y.z`, and use a `vx.y.z` tag. The automated
release workflow should check that these agree before attaching assets to
the GitHub Release; creating the tag or publishing the release requires
authorization for that release. The separate `.github/workflows/release.yml`
runs a macOS archive build only for `v*` tag pushes, not for routine PRs or
pushes to `main`. Verify the archive before tagging; do not push a release
tag to test a CI change.
