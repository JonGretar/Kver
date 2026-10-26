# Issue tracker: GitHub

Issues and specs for Folio live in GitHub Issues. Use the `gh` CLI for issue operations. The repository remote is `https://github.com/JonGretar/Folio.git`; use `jj git remote list` to check it.

## Issue operations

- Create: `gh issue create --title "..." --body "..."`
- Read, including comments: `gh issue view <number> --json number,title,body,labels,comments`
- List: `gh issue list --state open --json number,title,labels`
- Comment: `gh issue comment <number> --body "..."`
- Label: `gh issue edit <number> --add-label "..."`
- Close: `gh issue close <number> --comment "..."`

Run `gh` from the repository so it can identify Folio.

## Tickets and blockers

Publish each approved ticket as a separate issue. If its source is an existing issue, make it a sub-issue using `gh issue create --parent <parent> ...` when supported; otherwise put `Part of #<parent>` in the child body.

Prefer GitHub's native issue dependencies for blocking edges. Get a blocker's database ID with `gh api repos/JonGretar/Folio/issues/<number> --jq .id`, then add the edge with `gh api --method POST repos/JonGretar/Folio/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>`. If dependencies are unavailable, put `Blocked by: #<number>` in the child body.

`to-tickets` uses the `ready-for-agent` label. Create it if needed before publishing tickets that use it.

## Pull requests as a triage surface

**PRs as a request surface: no.** Set this to `yes` later if external PRs should enter a triage queue.
