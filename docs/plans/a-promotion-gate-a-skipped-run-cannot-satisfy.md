---
slug: a-promotion-gate-a-skipped-run-cannot-satisfy
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-20
---
# A promotion gate a skipped run cannot satisfy

`main` is protected by two required checks, `Quality gate` and `From dev`. On
2026-08-20 the dev tip carried a green `Quality gate` from a run in which **no
suite executed at all**, next to a second run on the same commit in which the
core suite was red. The ruleset would have let the promotion through on the
first stamp.

This plan closes that, and closes the routing hole that let two CI breakages sit
undetected long enough to surface together.

**All line references against `origin/dev` = `40655644fc`.** The shared main
checkout is older; read with `git show origin/dev:<path>`.

## What holds today

Three facts, each verified against the repository and the live rulesets:

1. `.github/scripts/ci-paths.sh --suite-skip` returns `true` for **every**
   `pull_request` event, and for a `push` to `main` whose head equals the dev
   tip and whose actor is the repository owner. With `suite_skip=true` the
   `changes` job emits `android=false gnome=false core=false`, every suite job
   is skipped, and `require-ci-results.sh` prints *"External suites skipped for
   a PR or exact owner promotion"* and exits 0. The `Quality gate` check goes
   green in about eight seconds.

2. `--diff` maps `scripts/*`, `.github/*`, `docs/*`, `quality/*`, `showroom/*`
   and a list of root files to **no** suite. A push to `dev` touching only those
   runs `base-contracts` and nothing else — even though the core suite is where
   `scripts/tests/*.sh` and the showroom suite actually execute.

3. The rulesets require:

   | Ruleset | Target | Required checks |
   |---|---|---|
   | `dev-pr-boundary` | `refs/heads/dev` | `Quality gate` |
   | `main-promotion-gates` | `refs/heads/main` | `Quality gate`, `From dev` |

   `From dev` comes from `.github/workflows/dev-promotion-source.yml`, which runs
   **exclusively** on pushes to `dev`. A pull-request run cannot produce it.

## Why the obvious fix is wrong

The tempting change is "a skipped run must not stamp `Quality gate`". It breaks
the repository: `dev-pr-boundary` requires exactly that check, and every
pull-request run has `suite_skip=true`. Renaming the stamp for skipped runs
would leave every PR into `dev` permanently unmergeable.

The distinction therefore has to hang on the **target**, not on the event.
`From dev` already demonstrates the shape: a check only a real push to `dev` can
produce, required only where it matters.

## Part A — a second stamp that carries its provenance

### A1. A new job in `.github/workflows/ci.yml`

```yaml
  verified-suites:
    name: Verified suites
    needs: [changes, base-contracts, android-unit-suite, gnome-suite, core-suite]
    if: always() && github.event_name != 'pull_request'
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v7
      - name: Require suites that actually ran
        env: # same nine values the quality job passes
        run: .github/scripts/require-verified-suites.sh ...
```

The `if:` matters as much as the script. On a pull request the job does not
exist, so the check never appears — no red noise on PRs into `dev`, and nothing
a PR run can fabricate.

### A2. `.github/scripts/require-verified-suites.sh`

Succeeds only when **all** of these hold for this exact commit:

- `suite_skip == false`
- `base-contracts` concluded `success`
- `core-suite` concluded `success` — not `skipped`
- `android-unit-suite` concluded `success` — not `skipped`
- `gnome-suite` concluded `success` **or** `skipped`

GNOME is the one exception, and deliberately: it is routed on GTK paths and is
legitimately absent from most commits. Core and Android are what the promotion
is asserting about.

Every other combination exits non-zero with a message naming which suite did not
run, so the failure reads as *"nothing verified this"* rather than *"something
broke"*.

### A3. Ruleset change (repo admin, not code)

`main-promotion-gates` requires `From dev` + **`Verified suites`**.
`Quality gate` may stay or go; it no longer carries weight there.
`dev-pr-boundary` is **untouched**.

### A4. The promotion procedure this implies

A dev tip only becomes promotable once a run on that commit really executed core
and Android. For a merge that routes them, the push run does it. For anything
else — docs, a CI fix, a showroom change under today's routing — it takes:

```
gh workflow run ci.yml --ref dev     # workflow_dispatch routes android+core
```

Document this in `RELEASING.md` as the promotion step, not as folklore.

### A5. Contract test

`.github/tests/promotion-needs-a-verified-run.sh`, in the style of the existing
`dev-promotion-source.sh`:

- the `verified-suites` job exists and is named `Verified suites`
- its `if:` excludes `pull_request`
- `require-verified-suites.sh` rejects `suite_skip=true`
- it rejects `core_result=skipped` and `android_result=skipped`
- it accepts `gnome_result=skipped`

Table-driven over the script, the way `require-ci-results.sh` is already covered.

## Part B — route the paths the core suite actually runs

`emit_routes` in `.github/scripts/ci-paths.sh` gains, **before** the catch-all
`.github/* | docs/* | …` arm that currently swallows them:

```sh
    scripts/tests/* | scripts/cua-e2e/* | scripts/cua-common/* | \
        scripts/cua-explore/* | scripts/ci-quality.sh | showroom/*)
        core=true
        ;;
```

Ordering is the whole trick: `case` takes the first matching arm, and the
existing `scripts/*` arm would otherwise win.

`showroom/*` belongs in this list by the same rule as the others — the showroom
suite runs inside the core job. It is called out here because it was not part of
the original question and widens the change: every showroom commit now costs a
core-suite run.

What stays unrouted: `docs/*`, `quality/*`, the root files, and the rest of
`.github/*` and `scripts/*`. Those do not execute in a suite.

### Contract test

Extend `.github/tests/ci-path-routing.sh`:

- `scripts/tests/cua-e2e.sh` → `core=true`
- `showroom/src/App.tsx` → `core=true`
- `docs/plans/foo.md` → nothing
- `scripts/check-release.sh` → nothing (still unrouted)

## What this does not fix

A dev tip whose last run routed only *some* suites still carries results for the
others from an earlier commit. Part A refuses to promote such a tip, so the hole
cannot reach `main` — but `dev` itself can still read green while an unrouted
suite is stale. Reading dev's health still means walking each job back to the
commit where it last really ran, rather than trusting the latest conclusion.

Teaching the gate that walk — so it could accept a tip whose suites passed on an
earlier, unchanged commit — was considered and set aside as a larger moving
part. Part A makes it unnecessary for `main`: a dispatch costs nine minutes and
answers the same question without a new mechanism to trust.

## Evidence this is real

- Run `32323808817` on `51e9c6c` — core suite failure, `jq: command not found`.
- Run `32323812522` on the **same commit** — every suite skipped, `Quality gate`
  success. The promotion PR #579 would have merged on this.
- Both CI breakages fixed on 2026-08-20 (#583, #584) lived in `scripts/`, which
  routes nothing. Neither the PR run nor the merge push executed the suite that
  catches them; only a manual `workflow_dispatch` did.

## Order of work

1. Part B first — it is small, self-contained, and makes Part A's dispatch
   requirement rarer.
2. Part A2 + A5 (script and its test) before A1, so the job is wired to
   something already proven.
3. A1, then A3 by hand on the ruleset, then A4 in `RELEASING.md`.
4. Verify end to end: push a docs-only commit to `dev`, confirm
   `Verified suites` is absent and the promotion PR is blocked; dispatch,
   confirm it goes green and the block lifts.
