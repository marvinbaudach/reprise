# Protected branch workflow

Reprise uses a two-level integration model:

```text
main <- dev <- feature/<short-description>
```

`main` is the stable branch. It is always expected to contain a revision that
passed the complete automated project gate. `dev` is the integration branch
for the next stable revision. Normal feature and fix branches start from
`dev` and open a pull request back to `dev`.

## Required flow

1. Update local `dev` from `origin/dev`.
2. Create a focused `feature/*`, `fix/*`, or `chore/*` branch from `dev`.
3. Open a pull request to `dev`.
4. Merge only after `CI / Quality gate` is green.
5. Promote a tested development snapshot with a pull request from `dev` to
   `main`. Pull requests from any other head branch to `main` fail CI unless
   they use the emergency `hotfix/*` path below.

Direct pushes to `main` and `dev` are prohibited by GitHub branch rules.
Force pushes and branch deletion are prohibited. Required checks are strict,
so a pull request must be tested against the latest target branch before it
can merge. Conversations must be resolved. The repository has one maintainer,
so an approval is not required; CI and the pull-request boundary are required.

Emergency hotfixes branch from the current `main` as `hotfix/*` and may open
a pull request directly to `main`. They run the same complete required gate;
urgency never bypasses CI or the pull-request boundary. After the hotfix
merges, immediately synchronize `main` back into `dev` through a pull request
so the integration branch cannot lose the repair. Routine fixes still use the
normal feature-to-`dev` promotion path.

## What CI enforces

Every pull request runs `.github/workflows/ci.yml`, regardless of its target
branch. Pushes to `dev` and `main` run it again. Its required
`CI / Quality gate` executes
`scripts/check-merge-readiness.sh --no-fetch`, including formatting, strict
Clippy, warning-free Rust documentation, all non-ignored Workspace tests,
the rule-owned GTK/Xvfb display tests, architecture and UX policy checks, and
the dependency audit.

The Action uses an isolated Arch Linux container because Reprise requires GTK
4.22 and libadwaita 1.9. Tests use temporary XDG directories, a private D-Bus
session, Xvfb, and the fake audio sink through the existing project gates.

## GitHub rules

Both protected branches require `CI / Quality gate`, require a pull request,
require resolved conversations, require the branch to be current, and disallow
force pushes and deletion. Administrators do not bypass these rules. `main`
remains the repository default branch.
