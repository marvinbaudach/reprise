# Reprise worktree cleanup

Reprise uses a conservative garbage collector instead of deleting directories
by name or age. Source changes always win over disk reclamation: dirty, locked,
active, protected, and worktrees with commits outside `dev` are retained.

## Close a merged worktree

After a topic branch was squash-merged into `dev`, run this from the topic
worktree while the GitHub pull request still identifies its exact head:

```sh
~/Projects/reprise/scripts/close-worktree.sh \
  --repo ~/Projects/reprise \
  --worktree "$PWD" \
  --pr 123
```

The command asks GitHub for the pull request state, target branch, source
branch, and head commit. It refuses cleanup unless all four match the local
worktree. If the current session or another process is still using the
directory, that case is recorded under
`~/.local/state/reprise-worktree-gc/pending/`. The weekly sweep completes it
after every process has left it.

## Inspect before applying

The default sweep is read-only:

```sh
scripts/reprise-worktree-gc.sh sweep \
  --scope ~/Projects/reprise
```

`--scope` governs **removal only**. It checks the canonical repository and
independent Git repositories found below its `.worktrees/` directory — that is
what lets the sweep find Claude worktrees nested inside an older standalone
checkout. Registered worktrees outside the scope's `.worktrees/` directory are
reported as `outside_scope` and never removed.

Build-artefact deletion is the other axis and is **not** bound to the scope: it
runs in every worktree the repository reports, inside the scope or not. Pass
`--exclude PATH` (repeatable) to take a worktree out of both axes; the timer
uses it for the nightly source tree, whose `target/` must survive so the 04:30
build stays incremental.

Use `--apply` only after reading the report. It removes clean worktrees that
have no commits outside `dev`, completes exact pending PR cleanups, prunes
their Git metadata, and deletes their local topic branches. It does not delete
remote branches.

Retained worktrees with build output of at least 1 GiB are eligible for
artefact deletion after seven days without recent activity — `target/`,
`android/app/build`, and `.gradle-user-home`. A dirty or unmerged worktree
keeps every tracked and untracked source file and loses only those three
directories; they are reproducible and never hold source. Only `active` and
`locked` worktrees, worktrees with a process using them, and excluded paths are
skipped, because a build may be running there. Each applied run is logged under
`~/.local/state/reprise-worktree-gc/runs/`, including the reclaimed KiB total.

## Enable the weekly sweep

Install and start the checked-in user timer:

```sh
scripts/install-worktree-gc-timer.sh
```

The installer copies the collector to
`~/.local/libexec/reprise-worktree-gc`, so the timer does not depend on the
checkout used for installation remaining on `dev`. Rerun the installer after
updating Reprise to refresh that installed copy.

It runs on Sunday at 04:15 with up to 30 minutes of randomized delay.
`Persistent=true` makes systemd run a missed sweep after the machine starts
again.

Inspect or run it manually:

```sh
systemctl --user status reprise-worktree-gc.timer
systemctl --user start reprise-worktree-gc.service
journalctl --user -u reprise-worktree-gc.service
```

## Prevent recursive Claude worktrees

Claude Desktop stores worktrees below the current project by default. Set
**Settings → Claude Code → Worktree location** to:

```text
~/.cache/reprise-agent-worktrees
```

Archive completed desktop sessions after their pull requests are merged.
The weekly collector remains the fallback for interrupted sessions and older
standalone repositories.
