---
name: land
description: >-
  Land a reviewed Kver change on the default branch only when the user has
  explicitly requested landing, including through Land Changes or /land.
  Not for review, preparation, passing checks, or skill installation alone.
disable-model-invocation: true
metadata:
  delta-action: land
---

# Land a Kver change

An invocation of this skill is the landing request. Carry it through to the
destination; do not ask for the same merge authorization again. Stop for a
genuine blocker, unresolved scope, or an ambiguous conflict, and report that
the change has **not landed**. Follow the project's review and approval policy
in `docs/agents/workflow.md` (steps 2–4); keep incomplete roadmap gates
explicit rather than declaring them finished merely because code was merged.

1. **Establish scope and destination.** Inspect the complete branch diff,
   uncommitted and untracked files, recent history, remotes, and the current
   default branch. Identify the change the user asked to land and exclude
   unrelated files; ask when ownership or intended scope is unclear. Confirm
   the source remote (currently `origin` for Kver) rather than publishing to
   Delta's `local` backlink. Review the change and any pending feedback before
   publication. Use `jj` if the checkout is a `jj` repository; this project's
   present Delta checkout is Git-only, so use Git there. Do not rewrite a
   published branch's history or overwrite other people's work.

2. **Prepare a reviewable branch.** Fetch the destination branch and check for
   concurrent changes. From the existing change state, create a uniquely
   named feature branch if needed, stage only the selected files, inspect the
   staged diff, and commit with a descriptive message. Use non-interactive
   commands (`GIT_EDITOR=true` for Git operations that could open an editor).
   Resolve straightforward conflicts when the intended result is clear,
   preserving both sides' unrelated work; pause for the user's decision on
   ambiguous conflicts. Recheck the diff after a resolution.

3. **Verify the exact candidate.** On macOS with Rust 1.92+ and the Metal
   Toolchain, run `cargo +stable fmt --check`, `cargo +stable test
   --all-targets`, `cargo +stable clippy --all-targets -- -D warnings`, and
   `cargo +stable build --bin kver`. The formatting/Clippy convention is in
   `AGENTS.md` (Rust Conventions); the macOS test, Clippy, and binary checks
   are recorded in `docs/rendering-validation.md` (Platform feasibility);
   `Cargo.toml` defines the library, optional GPUI GUI, and `kver` binary.
   If these files or targets change, derive the checks from the current
   definitions instead. Include applicable real-PDF/macOS window verification
   for UI or rendering changes as required by `docs/agents/workflow.md`
   (step 3); distinguish performed checks from the still-open Phase 0 gate.
   Fix failures and rerun affected checks on the final candidate. Do not
   substitute a narrower test for a required check without reporting a
   blocker.

4. **Publish and review.** Push only the feature branch to the verified source
   remote and open a pull request targeting the repository's current default
   branch. Include a meaningful title/body, verification evidence, and known
   limitations; follow any applicable submission template or contribution
   requirements if they appear. `docs/agents/workflow.md` (step 4) calls for
   review and a feature branch/PR when needed; the PR runs the macOS and Linux
   checks defined in `.github/workflows/ci.yml`. Inspect its results, including
   the exploratory hosted-macOS headless capture, before merging. Verify the
   macOS and Linux PR checks have passed on the PR's current head and resolve
   outstanding review feedback. A pending, failing, missing, or unverifiable
   PR check blocks landing. The owner's `main` protection only prevents force
   pushes and deletion; do not query GitHub rulesets or branch-protection
   settings as a separate landing gate.

5. **Merge and prove it landed.** Before merging, ensure the PR head and
   destination are still the versions that were reviewed and checked. If the
   destination has moved, integrate it without rewriting published history,
   resolve simple conflicts per the preference above, and re-run affected
   local and remote checks on the new candidate. Use the ordinary GitHub
   squash merge without admin bypass or unattended auto-merge; pin the merge
   to the verified PR head. Confirm the PR is merged into the intended
   destination and its merge commit appears in that destination's remote
   history. Report the PR, commit, destination, checks, and any remaining
   limitations. A published branch or open PR is **not** a completed landing.
