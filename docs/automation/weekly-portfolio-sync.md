# Weekly Reprise portfolio evidence refresh

The intended schedule is Monday at 07:30 in `Europe/Zurich`. The ready-to-install crontab
fragment lives in `docs/automation/reprise-portfolio-sync.cron`; the runner itself remains safe
to invoke manually for validation. A successful run creates local review branches and never
pushes them.

Maintain the public-facing Reprise showroom and Marvin's Bewerbung project from current,
committed Reprise evidence. Work autonomously in the two isolated worktrees below:

- Reprise: `{{REPRISE_WORKTREE}}`
- Bewerbung: `{{BEWERBUNG_WORKTREE}}`

Read Reprise's `AGENTS.md`, `.superpowers/sdd/progress.md`, relevant plans and recent Git
history completely before editing. Read Bewerbung's `CLAUDE.md` completely before editing.
Treat each repository's committed `main` snapshot as ground truth; never use remembered figures
or changes from another working tree.

## Reprise showroom

Review the README, `docs/showcase.md`, and the SVGs under `docs/assets/`. Update them only where
the committed code, accepted benchmark evidence, or completed ledger entries have materially
changed. Keep the presentation concise, precise, visually consistent, and suitable for a senior
engineering portfolio. Every claim must map to code, a test, an accepted benchmark report, or the
progress ledger.

Do not rerun host-sensitive performance benchmarks as part of this weekly task. Change benchmark
figures only when a newer accepted report is committed on `main`, and retain its workload,
measurement method, and limitations. Do not fabricate screenshots, benchmark results, feature
status, or architecture details.

## Bewerbung

Run `scripts/reprise-stats.sh {{REPRISE_WORKTREE}}` from the Bewerbung worktree. Apply the reported
production and test code figures everywhere required by `CLAUDE.md`, including the CV and shared
profile contracts. Recount the top-level merge-readiness gates from the committed Reprise script
when that script changed. Keep the CV and showroom terminology semantically aligned.

Build and test the Bewerbung documents exactly as its repository instructions require. Do not
alter its established visual design or attempt to repair the documented PDF-viewer colour
artifact.

## Safety and completion

- Never access real music, Reprise's real database, user accounts, credentials, or the live GUI.
- Do not push, publish, open pull requests, or modify branches outside these worktrees.
- Preserve unrelated changes and avoid speculative product claims.
- Run every applicable repository gate before committing.
- Commit Reprise changes as `docs(showcase): refresh weekly Reprise evidence`.
- Commit Bewerbung changes as `docs(cv): refresh weekly Reprise evidence`.
- If a repository needs no factual update, leave it unchanged and do not create an empty commit.
- Finish with both worktrees clean and report evidence, changes, gates, and any honest deferrals.
