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
in `docs/agents/workflow.md` (steps 2–4); apply its documentation lifecycle
instead of maintaining a parallel roadmap status ledger.

1. **Establish scope and destination.** Inspect the complete branch diff,
   uncommitted and untracked files, recent history, remotes, and the current
   default branch. Identify the change the user asked to land and exclude
   unrelated files; ask when ownership or intended scope is unclear. Confirm
   the source remote (currently `origin` for Kver) rather than publishing to
   Delta's `local` backlink. Review the change and any pending feedback before
   publication. Check whether this checkout supports `jj`. Prefer it when
   useful, but Git is also acceptable in agent threads; use one tool coherently
   for mutating a change rather than mixing `jj` and Git history operations.
   Do not rewrite a published branch's history or overwrite other people's work.

2. **Prepare a reviewable branch.** Fetch the destination branch and check for
   concurrent changes. From the existing change state, create a uniquely
   named feature branch or `jj` bookmark if needed. Include only the selected
   files, inspect the complete candidate diff, and commit or describe the
   change with a descriptive message. Use non-interactive commands
   (`GIT_EDITOR=true` for Git operations that could open an editor).
   Resolve straightforward conflicts when the intended result is clear,
   preserving both sides' unrelated work; pause for the user's decision on
   ambiguous conflicts. Recheck the diff after a resolution.

3. **Verify the exact candidate efficiently.** Inspect the diff and choose
   local checks for the feedback they add before PR CI. For docs or skills,
   validate changed links, syntax, or instructions; a Rust rebuild adds no
   evidence about those changes. For Rust changes, a focused behavior test,
   formatting check, Clippy, or build may catch a likely failure quickly.
   Choose the useful subset; no local suite or build is mandatory merely
   because Rust changed. A broad or risky change may merit the full local
   suite. Derive commands from current project targets. Include applicable
   real-PDF/macOS window verification for UI or rendering changes as required
   by `docs/agents/workflow.md` (step 3). Fix failures and rerun affected
   checks on the final candidate. Report what actually ran; passing macOS
   and Linux PR CI is the full cross-platform landing gate.

4. **Publish and review.** Push only the feature branch to the verified source
   remote and open a pull request targeting the repository's current default
   branch. Include a meaningful title/body, verification evidence, and known
   limitations; follow any applicable submission template or contribution
   requirements if they appear. `docs/agents/workflow.md` (step 4) calls for
   review and a feature branch/PR when needed; the PR runs the macOS and Linux
   checks defined in `.github/workflows/ci.yml`. Inspect their results and,
   for UI or rendering changes, relevant hosted-macOS headless captures.
   Verify the macOS and Linux PR checks have passed on the PR's current head
   and resolve outstanding review feedback. A pending, failing, missing, or
   unverifiable PR check blocks landing. The owner's `main` protection only
   prevents force pushes and deletion; do not query GitHub rulesets or
   branch-protection settings as a separate landing gate.

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
