# Feature workflow

Use Delta-managed isolation for new implementation threads. A Delta worktree
is a separate checkout for that thread; creating an additional Git worktree is
not part of the routine workflow. Git and `jj` are both acceptable for
version-control operations in this repository.

1. **Decide what to build.** Use `docs/roadmap.md` for release gates and
   `docs/features.md` for scope. For a multi-session feature, agree on a spec
   and small, verifiable tickets before publishing them to
   [GitHub Issues](issue-tracker.md). Record settled domain terms in
   `GLOSSARY.md` and consequential trade-offs in `docs/adr/`.
2. **Start isolated.** For a new feature, create a Delta thread for Kver and
   choose **Isolated Delta Worktree** in **Project Settings** before sending
   its first message. Check the starting revision: a new thread normally
   starts from the default branch, which may not include work still under
   review in another thread. Use a subthread when a task needs the parent's
   current unlanded changes; request an explicit pull of its worktree changes
   back into the parent when appropriate.
3. **Build a slice.** Implement one usable behavior per ticket. Follow the
   applicable project skills (such as `tdd` for test-first work), run the
   checks relevant to that behavior, and demonstrate it with a real PDF where
   UI or rendering behavior matters. PR CI builds, tests, and runs the
   headless visual integration target on both macOS and Linux. These checks validate off-screen
   rendering, not a visible window or pointer hit-testing. Check a real macOS
   window for releases or changes to native window/input behavior; a Linux
   Cloud Runner cannot substitute for that check. Do not require a local Mac
   for every routine UI PR.
   For exploratory UI probes, temporarily add a Rust case to `tests/visual.rs`,
   run it on the main thread with `cargo +stable test --locked --features screenshot --test visual`,
   and inspect captures under ignored `target/`. Remove the probe code when
   done; retain a test or committed screenshot only when intentionally chosen
   as a stable regression gate. Preserve other work in the file when removing
   your probe.
4. **Review, then land.** Inspect **Review Changes** in Delta and resolve
   feedback. Publish an ordinary feature branch and pull request for review
   when needed. Require the passing macOS and Linux PR checks before landing. An
   independent thread's changes reach the project through version control;
   an isolated subthread's changes require an explicit worktree pull into its
   parent. Landing on `main` requires approval for that specific change.

Delta's continuous file history is not a commit or a merge. Keep plans,
tickets, tests, and the reviewed change distinct so each can be checked on
its own terms.
