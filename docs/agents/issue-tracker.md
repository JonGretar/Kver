# Issue tracker: GitHub

Issues and specs for Kver live at https://github.com/JonGretar/Kver/issues. Use the `gh` CLI for issue operations.

## Issue operations

- Create: `gh issue create --title "..." --body "..."`
- Read, including comments: `gh issue view <number> --json number,title,body,labels,comments`
- List: `gh issue list --state open --json number,title,labels`
- Comment: `gh issue comment <number> --body "..."`
- Label: `gh issue edit <number> --add-label "..."`
- Close: `gh issue close <number> --comment "..."`

Run `gh` from the repository so it can identify Kver's GitHub repository.
Before publishing a spec or ticket, check `gh repo view --json nameWithOwner`
and confirm it targets `JonGretar/Kver`, especially from forks and clones.

## Tickets and blockers

Publish each approved ticket as a separate issue. If its source is an existing issue, make it a sub-issue using `gh issue create --parent <parent> ...` when supported; otherwise put `Part of #<parent>` in the child body.

Prefer GitHub's native issue dependencies for blocking edges. Get a blocker's database ID with `gh api repos/JonGretar/Kver/issues/<number> --jq .id`, then add the edge with `gh api --method POST repos/JonGretar/Kver/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>`. If dependencies are unavailable, put `Blocked by: #<number>` in the child body.

`to-tickets` uses the `ready-for-agent` label. Create it if needed before publishing tickets that use it.
For the meaning of ready, in progress, blocked, and done, see
[work tracking](workflow.md#work-tracking); the label alone is not a work status.

## Pull requests as a triage surface

**PRs as a request surface: no.** Set this to `yes` later if external PRs should enter a triage queue.
