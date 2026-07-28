# Branch workflow

Reprise uses a two-level integration model:

```text
main <- dev <- feature/<short-description>
```

`main` is the stable branch. It is always expected to contain a revision that
passed the complete project gate. `dev` is the integration branch for the next
stable revision. Normal feature and fix branches start from `dev` and open a
pull request back to `dev`.

## Required flow

1. Update local `dev` from `origin/dev`.
2. Create a focused `feature/*`, `fix/*`, or `chore/*` branch from `dev`.
3. Open a pull request to `dev`.
4. Merge only after the gate is green — see "What actually enforces this"
   below for which gate that is today — and squash the pull request; see
   "Merge method".
5. Promote a tested development snapshot with a pull request from `dev` to
   `main`. That one is a merge commit, not a squash. Any other head branch
   targeting `main` is wrong unless it uses the emergency `hotfix/*` path
   below.

Direct pushes to `main` and `dev`, force pushes, and deleting either branch
are all forbidden. Read that as a rule this project holds itself to, not as
one the platform stops you from breaking — see below.

Emergency hotfixes branch from the current `main` as `hotfix/*` and may open
a pull request directly to `main`. They run the same complete gate; urgency
never bypasses the gate or the pull-request boundary. After the hotfix
merges, immediately synchronize `main` back into `dev` through a pull request
so the integration branch cannot lose the repair. Routine fixes still use the
normal feature-to-`dev` promotion path.

## Merge method

The method depends on what the head branch is, and there are exactly two
cases:

| Pull request | Method | Result on the base branch |
| --- | --- | --- |
| `feature/*`, `fix/*`, `chore/*` → `dev` | **Squash and merge** | one commit per pull request |
| `hotfix/*` → `main` | **Squash and merge** | one commit per hotfix |
| `dev` → `main` (promotion) | **Merge commit** | the snapshot, with its per-pull-request commits |
| `main` → `dev` (post-hotfix sync) | **Merge commit** | the repair, without duplicating it |

Rebase merging is not used at all.

Topic branches squash because their internal history is working history: the
test-first loop from `AGENTS.md` produces a commit per task plus follow-up
fix commits, and none of that is worth carrying on `dev` forever. What is
worth carrying is one reviewed, gate-passed change per pull request.

The two integration pull requests must **not** squash, for a reason that is
structural rather than aesthetic: their head branches are long-lived. Squashing
`dev` into `main` would write a commit that no longer contains `dev` in its
ancestry, so every later promotion would replay the whole difference again and
`dev` would diverge from `main` permanently. The same applies to syncing a
hotfix back from `main`. A merge commit keeps both branches on one ancestry —
which is also what makes `git merge-base --is-ancestor` in
`scripts/check-merge-readiness.sh` a meaningful staleness check.

The squash commit message is the pull request title in the project's
conventional-commit form (`fix(sync): name the running step`); GitHub appends
the `(#N)` reference. Trim the auto-collected list of branch commits out of the
body and leave the explanation instead. No attribution footer, as everywhere
else in this repository.

Two consequences of squashing, both of which bite silently:

- **`git branch -d` refuses to delete a squashed branch.** Git sees no merge,
  because there is none — the branch content arrived as a new commit. Use
  `git branch -D <branch>` locally after confirming the squash commit is on
  `dev`, and delete the remote branch as described below.
- **Do not stack a topic branch on another topic branch.** Once the parent is
  squashed into `dev`, the child still carries the parent's original commits,
  and merging `dev` back in conflicts against content that is textually
  identical but has no shared ancestry. Branch from `dev`, or rebase the child
  onto `dev` after the parent lands.

Nothing enforces the choice. Both methods stay enabled under
*Settings → General → Pull requests* precisely because the promotion needs the
merge commit, so GitHub offers the maintainer both buttons on every pull
request and the correct one is the one the table above names. Turning off
"Allow merge commits" would break the promotion; turning off "Allow squash
merging" would break everything else. Enable "Default to pull request title
for squash merge commits" so the common case needs no editing.

## What actually enforces this

Honestly: nothing on GitHub's side. Verified 2026-07-28 —

```console
$ gh api repos/marvinbaudach/reprise/branches/dev/protection
Upgrade to GitHub Pro or make this repository public to enable this
feature. (HTTP 403)

$ gh api repos/marvinbaudach/reprise/rulesets
Upgrade to GitHub Pro or make this repository public to enable this
feature. (HTTP 403)
```

The repository is private on a Free plan, where both classic branch
protection and rulesets are unavailable. There are no required checks, no
enforced pull-request boundary, and nothing stopping a direct push to `dev`
or `main`. A pull request whose checks are red still reports `MERGEABLE`.

An earlier version of this document described required checks, resolved
conversations, "administrators do not bypass these rules" and prohibited
force pushes as facts. They were the intended configuration, never an active
one. Recording an intention as a guarantee is worse than recording no
guarantee at all: it invites trusting a gate that will not catch anything.

**So the gate is local, and running it is the maintainer's job:**

```sh
MERGE_READINESS_BASE_REF=origin/dev scripts/check-merge-readiness.sh
```

Run it on the branch, against the branch you are merging into, after merging
the latest target branch in. It refuses to run on a dirty worktree and on a
branch that is behind its base, which is exactly the discipline the missing
server-side rules would have provided. A merge without it is unverified, no
matter how green the last local `cargo test` looked.

Two consequences worth stating plainly:

- **Branch cleanup is manual.** `.github/workflows/delete-merged-branch.yml`
  only runs if Actions run. Delete the branch locally (`git branch -D`, see
  "Merge method") and on the remote after merging.
- **Turning the plan on changes this file, not the workflow.** If the
  repository ever moves to a plan with rulesets, configure the branches to
  require `CI / Quality gate` and replace this section with what is then
  actually true.

## What CI would enforce

Every pull request runs `.github/workflows/ci.yml`, and so does every push to
`dev` and `main` — that is what the workflow declares. Its `CI / Quality gate`
executes `scripts/check-merge-readiness.sh --no-fetch`, covering formatting,
strict Clippy, warning-free Rust documentation, all non-ignored workspace
tests, the rule-owned GTK/Xvfb display tests, architecture and UX policy
checks, and the dependency audit — the same script the local gate runs.

The Action uses an isolated Arch Linux container because Reprise requires GTK
4.22 and libadwaita 1.9. Tests use temporary XDG directories, a private D-Bus
session, Xvfb, and the fake audio sink through the existing project gates. CI
runs up to four independently isolated display tests concurrently; local runs
stay serial unless `DISPLAY_TEST_JOBS` is set explicitly.

As of 2026-07-28 the workflow does not run at all: GitHub refuses to start
jobs for this account with *"recent account payments have failed or your
spending limit needs to be increased"*. Until that is resolved, CI is not a
second opinion on the local gate — it is absent, and the local run is the
only verification there is.
