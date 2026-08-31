---
slug: showreel-scripts-pass-shellcheck
worktree: /home/marvin/Projects/reprise-showreel-scripts-pass-shellcheck
branch: feature/showreel-scripts-pass-shellcheck
phase: coded
codex_session:
created: 2026-08-31
---
# The showreel scripts pass ShellCheck

`scripts/check-shell.sh` fails on `dev` with three warnings, all in scripts that
landed with #761:

```
scripts/showreel/cut-film.sh:81:1  SC2034  DEBADGE appears unused
scripts/showreel/cut-film.sh:165:9 SC2155  Declare and assign separately
scripts/showreel/take-android2.sh:20:1 SC2034 APP appears unused
```

Nobody saw them until now. The contract job runs "Verify project source
quality" before "Verify repository and workflow contracts", and the first step
was red on the showreel test since #761 — so the ShellCheck step was `skipped`
on every dev run and its failure never surfaced. #776 fixed that test; the
second step then ran for the first time and went red. A skipped job made a
broken one look green.

#775 already aimed at this ("The showreel scripts stop failing the contract
job's first step") but its own CI run aborted at the earlier step, so its fix
was never verified and these three survived.
