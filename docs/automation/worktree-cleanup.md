# Reprise worktree cleanup

Reprise uses a conservative garbage collector instead of deleting directories
by name or age. Source changes always win over disk reclamation: dirty, locked,
active, protected, and worktrees with commits outside `dev` are retained.

## Close a merged worktree

After a topic branch was squash-merged into `dev`, run this from the topic
worktree while the GitHub pull request still identifies its exact head:

```sh
/home/marvin/Projects/reprise/scripts/close-worktree.sh \
  --repo /home/marvin/Projects/reprise \
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
  --scope /home/marvin/Projects/reprise
```

`--scope` checks the canonical repository and independent Git repositories
found below its `.worktrees/` directory. This is what lets the sweep find
Claude worktrees nested inside an older standalone checkout. Registered
worktrees outside the scope's `.worktrees/` directory and the configured
central agent cache are reported but never changed by the scheduled sweep.

Use `--apply` only after reading the report. It removes clean worktrees that
have no commits outside `dev`, completes exact pending PR cleanups, prunes
their Git metadata, and deletes their local topic branches. It does not delete
remote branches.

Clean retained Cargo worktrees with build output of at least 1 GiB are
eligible for `cargo clean` after seven days without recent target activity.
Dirty or locked worktrees and worktrees with a process using them are skipped.
Each applied run is logged under
`~/.local/state/reprise-worktree-gc/runs/`, including the reclaimed KiB total.

## Enable the weekly sweep

Install and start the checked-in user timer:

```sh
scripts/install-worktree-gc-timer.sh
```

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
/home/marvin/.cache/reprise-agent-worktrees
```

Archive completed desktop sessions after their pull requests are merged.
The weekly collector remains the fallback for interrupted sessions and older
standalone repositories.
