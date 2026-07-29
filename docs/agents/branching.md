# Branch workflow

Reprise uses a two-level integration model:

```text
main <- dev <- feature/<short-description>
```

`main` is the stable branch. It is always expected to contain a revision that
passed the complete project gate. `dev` is the integration branch for the next
stable revision. Normal feature and fix branches start from `dev` and open a
pull request back to `dev`.

Since 2026-07-28 every pull request is squashed, because squash merging is the
only method the repository allows. That single setting decides the shape of
everything below: `main` is not merged into from `dev`, it is fast-forwarded
to it. See "Merge method".

## Required flow

1. Update local `dev` from `origin/dev`.
2. Create a focused `feature/*`, `fix/*`, or `chore/*` branch from `dev`.
3. Open a pull request to `dev`. Every pull request in this repository targets
   `dev` — there are no pull requests against `main` any more.
4. Merge only after the gate is green — see "What actually enforces this"
   below for which gate that is today. The merge is a squash; GitHub offers no
   other button.
5. Promote a tested development snapshot by fast-forwarding `main` to `dev`:

   ```sh
   git fetch origin
   git push origin origin/dev:main
   ```

   This is the one sanctioned direct push to `main`, and only the repository
   owner performs it. It is safe precisely because it cannot be anything but a
   fast-forward: git rejects a non-fast-forward push by default, so a `main`
   that has drifted away from `dev` announces itself here instead of being
   papered over.

Direct pushes to `main` other than the promotion above, every direct push to
`dev`, all force pushes, and deleting either branch are forbidden. Read that
as a rule this project holds itself to, not as one the platform stops you from
breaking — see below.

Emergency hotfixes branch from `dev` as `hotfix/*`, open a pull request to
`dev` like everything else, and reach `main` through the same fast-forward
promotion. They run the same complete gate; urgency never bypasses the gate or
the pull-request boundary.

State the cost of that plainly, because it is the price of a `main` that can
never diverge: a hotfix cannot reach `main` alone. Promoting it promotes
everything else sitting on `dev` at that moment. If `dev` carries work that
must not ship, the fix waits for that work to be finished or reverted — there
is no third option. A `hotfix/*` merged straight into `main` would put a
commit there that `dev` does not have, the next promotion would be rejected as
a non-fast-forward, and the only repairs available are a merge commit (the
setting forbids it) or a force push (this document forbids it). So: never.

Rolling a bad release back moves forward, not backward. Revert the offending
commits on `dev` through the normal pull-request path, then fast-forward
`main` again. `main` never moves backwards, which is what keeps the promotion
a fast-forward for good.

### One-time transition, not yet done

The first promotion under this model will be rejected, and it is not a mistake
in the command. Verified 2026-07-28:

```console
$ git merge-base --is-ancestor origin/main origin/dev; echo $?
1
```

`main` (`d1de9e3`) is not an ancestor of `dev` (`6b595fb`). Under the old model
it never had to be: each promotion wrote a merge commit **on `main`**, and
nothing carried that commit back into `dev`. So `main`'s history is a chain of
promotion merges that `dev` has never seen, and `git push origin origin/dev:main`
will refuse with `non-fast-forward` until that is repaired once.

The repair records `main` in `dev`'s ancestry without touching `dev`'s content,
which is exactly what `-s ours` means:

```sh
git checkout dev && git pull --ff-only origin dev
git merge -s ours origin/main -m "chore: adopt fast-forward promotion of main"
git push origin dev
```

`-s ours` is correct rather than lazy here: every commit reachable from `main`
is either an old `dev` commit or a promotion merge whose tree came from `dev`,
so `main` holds no work `dev` lacks, and `dev`'s tree is the one that must
survive. A plain `git merge` would try to reconcile a stale snapshot against
five weeks of newer work for no gain.

That push is a direct push to `dev`, forbidden everywhere else in this
document. It is a one-time migration the repository owner performs, and after
it every promotion is an ordinary fast-forward.

## Merge method

There is one method, and it is not a matter of judgement: **squash and merge**.
*Settings → General → Pull requests* has "Allow merge commits" and "Allow
rebase merging" switched off, so GitHub shows exactly one button and the API
answers any other `merge_method` with 405. Every pull request leaves one commit
on `dev`.

That is why `main` is fast-forwarded rather than merged into. A squashed
`dev` → `main` pull request would write a commit that does not contain `dev`
in its ancestry; the merge base would stay where it was, every later promotion
would replay the whole accumulated difference, and the two branches would
diverge for good after the second one. A fast-forward has no such failure mode:
`main` becomes the very commit `dev` already is.

Squashing suits topic branches for their own reasons. Their internal history is
working history — the test-first loop from `AGENTS.md` produces a commit per
task plus follow-up fix commits, and none of that is worth carrying on `dev`
forever. What is worth carrying is one reviewed, gate-passed change per pull
request.

One thing the model gives up, recorded so nobody rediscovers it as a surprise:
`main` can no longer receive an isolated repair. It is always exactly some
`dev` snapshot. The emergency path above says what to do instead, and why
attempting the isolated repair anyway is unrecoverable.

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

Set the squash "Default commit message" dropdown to **"Pull request title"**.
Not "Pull request title and description": the description carries this
project's review checklist, which does not belong in the permanent history.

## What actually enforces this

The merge method, and nothing else. Turning off "Allow merge commits" and
"Allow rebase merging" is a repository setting rather than a branch rule, so it
applies on the Free plan and cannot be clicked past — that is the whole reason
the promotion moved to a fast-forward push rather than staying a pull request.

Everything else here is honestly unenforced. Verified 2026-07-28 —

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
or `main` — which is also what makes the promotion push possible at all. A
pull request whose checks are red still reports `MERGEABLE`.

Worth knowing before the plan ever changes: enabling branch protection on
`main` would block the promotion push along with everything else. If that day
comes, `main` needs an explicit push allowance for the owner, or the promotion
has to move to a merge queue — protection and fast-forward promotion do not
coexist by default.

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
  "Merge method") and on the remote after merging. Repo-wide *"Automatically
  delete head branches"* is now safe to switch on and would replace both:
  it was off only because the old promotion pull request had `dev` as its
  head, and there is no such pull request any more.
- **Local worktree cleanup is verified and deferred when necessary.** After a
  squash merge, run
  `scripts/close-worktree.sh --repo /home/marvin/Projects/reprise --worktree <path> --pr <number>`.
  It checks that GitHub reports the PR merged into `dev`, that its source branch
  and head match the clean local worktree exactly, and that the worktree is not
  locked. If any process still uses the worktree as its current directory, the
  cleanup is queued under `~/.local/state/reprise-worktree-gc/pending/`; the
  weekly `reprise-worktree-gc.timer` completes it after the process exits.
  `docs/automation/worktree-cleanup.md` describes report and installation
  commands. This does not replace remote branch deletion.
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
