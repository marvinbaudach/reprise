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
5. Before a planned promotion, run and review the opt-in exploratory UX mission
   deck described in `scripts/cua-explore/README.md` on the exact clean `dev`
   candidate. It is a maintainer-owned advisory check, not ordinary CI.
6. Promote a tested development snapshot by fast-forwarding `main` to `dev`:

   ```sh
   git fetch origin
   git push origin origin/dev:main
   ```

   This is the one sanctioned direct push to `main`. Until Reprise is publicly distributed,
   agents may perform this promotion autonomously without separate owner authorization.
   Public distribution means availability through AUR, Flathub/GNOME Software, or another
   public app channel. This standing permission expires immediately when public distribution
   begins; after that, only the repository owner decides when promotion happens, though an
   agent may execute the exact promotion the owner explicitly authorizes. Immediately before
   pushing in either case, the agent live-reads both remote refs, proves `main` is an ancestor
   of `dev`, and verifies successful `Quality gate` and `From dev` checks on the exact current
   `dev` SHA. After pushing, the agent reads both refs back and requires exact equality. The
   push is safe precisely because it cannot be anything but a fast-forward: git rejects a
   non-fast-forward push by default, so a `main` that has drifted away from `dev` announces
   itself here instead of being papered over.

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

The squash commit message takes the pull request's short narrative title;
GitHub appends the `(#N)` reference. A title says what changed in the product
or project, for example `The queue keeps its place after filtering`. Trim the
auto-collected list of branch commits out of the body and leave the reason,
verification, and limitations instead. Do not add attribution footers.

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

Three repository rulesets enforce the branch boundary:

- `dev-pr-boundary` (`20937610`) requires a pull request, squash merging, an
  up-to-date `Quality gate`, and rejects deletion and non-fast-forward updates.
- `dev-owner-only-merge` (`20937968`) restricts updates to the repository owner.
  The owner's ruleset bypass is why `gh pr merge --admin --squash` is the
  deliberate merge command; it does not bypass `dev-pr-boundary`.
- `main-promotion-gates` (`20938111`) rejects deletion and non-fast-forward
  updates and requires `Quality gate` plus `From dev` on the promoted SHA.
  Neither check can be bypassed.

The pull-request `Quality gate` is intentionally quick. It proves routing and
reports the check context the `dev` ruleset requires, but it does not claim the
product suites ran. After the squash merge creates the authoritative `dev` SHA,
the `dev` push runs every path-selected suite and emits both promotion checks.
That is the evidence `main` accepts.

The local gate remains mandatory before commit and before asking GitHub to
merge:

```sh
MERGE_READINESS_BASE_REF=origin/dev scripts/check-merge-readiness.sh
```

Run it on the branch, against the branch you are merging into, after merging
the latest target branch in. It refuses to run on a dirty worktree and on a
branch that is behind its base. The fast pull-request check does not replace
this local evidence.

Two consequences worth stating plainly:

- **Branch cleanup is manual.** `.github/workflows/delete-merged-branch.yml`
  only runs if Actions run. Delete the branch locally (`git branch -D`, see
  "Merge method") and on the remote after merging. Repo-wide *"Automatically
  delete head branches"* is now safe to switch on and would replace both:
  it was off only because the old promotion pull request had `dev` as its
  head, and there is no such pull request any more.
- **Local worktree cleanup is verified and deferred when necessary.** After a
  squash merge, run
  `scripts/close-worktree.sh --repo ~/Projects/reprise --worktree <path> --pr <number>`.
  It checks that GitHub reports the PR merged into `dev`, that its source branch
  and head match the clean local worktree exactly, and that the worktree is not
  locked. If any process still uses the worktree as its current directory, the
  cleanup is queued under `~/.local/state/reprise-worktree-gc/pending/`; the
  weekly `reprise-worktree-gc.timer` completes it after the process exits.
  `docs/automation/worktree-cleanup.md` describes report and installation
  commands. This does not replace remote branch deletion.
- **Ruleset changes must update this document in the same task.** The IDs and
  requirements above are operational facts, not a desired future state.

## What CI enforces

Every pull request runs `.github/workflows/ci.yml`, but only its routing
job and always-reporting `Quality gate`; base contracts and product suites are
skipped for every author. Relevant cross-target compilation is skipped on pull
requests too, and the Showroom no longer builds before merge.

Every push to `dev` ignores all skip intent. It runs base contracts plus the
path-selected Android, GNOME, Core/workspace and cross-target suites. The
resulting squash SHA receives the authoritative `Quality gate`; the separate
dev-only workflow supplies `From dev`.

An exact fast-forward of that same SHA to `main` by the repository owner reuses
the dev evidence and reports a short `Quality gate` instead of recompiling the
same tree. Reuse requires all of: a push event, `refs/heads/main`, the repository
owner as actor, and exact equality with `origin/dev`. A non-owner or non-equal
main push fails the reuse predicate and runs the selected suites. The Showroom
remains main-only and still builds and deploys when its paths changed.

The Action uses an isolated Arch Linux container because Reprise requires GTK
4.22 and libadwaita 1.9. Tests use temporary XDG directories, a private D-Bus
session, Xvfb, and the fake audio sink through the existing project gates. The
standing gate runs the rule-named ignored GTK tests with four isolated workers.
The full ignored display inventory remains available manually through
`scripts/check-display-tests.sh`; it is not part of every merge.
