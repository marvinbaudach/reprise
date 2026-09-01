---
slug: the-ci-stops-repeating-itself-a2
worktree: /home/marvin/Projects/reprise-the-ci-stops-repeating-itself-a2
branch: feature/the-ci-stops-repeating-itself-a2
phase: planned
codex_session:
created: 2026-08-31
---
# Strand A2 — the display tests get their own sharded job

Mother plan: [`the-ci-stops-repeating-itself.md`](the-ci-stops-repeating-itself.md).
**Read its "27 gate lines that must not move" section before touching anything.**

**Wave 2.** Its worktree is cut from the `dev` that strand `a1` produced — not
before. `a1` and `a2` share four files and `a2` depends on `a1`'s `gate()`
mechanism.

## File ownership

```
scripts/check-merge-readiness.sh      (inherited from a1 — only the skip list moves)
scripts/ci-quality.sh                 (inherited from a1)
.github/workflows/ci.yml              (inherited from a1)
scripts/tests/qa-linters.sh           (inherited from a1)
.github/scripts/ci-paths.sh
.github/scripts/require-ci-results.sh
.github/scripts/check-gnome-ci.sh
.github/tests/ci-path-routing.sh
scripts/check-display-tests.sh
```

## A2.1 — `check-display-tests.sh` learns `--shard K/N`

Sort the discovered list first, so the split is identical across builds, then
select round-robin: `index % N == K-1`. Neighbouring tests come from the same
module and cost alike, so round-robin balances better than contiguous chunks.

Compose with the existing modes rather than replacing them: `--shard` applies
**after** the measurement-tool drop and after `--rule-named` / `--css` filtering,
so `--shard` alone shards the full 852.

Guard the argument: `K` and `N` positive integers, `1 <= K <= N`, exit 2
otherwise. An out-of-range shard that silently runs zero tests is exactly the
failure this plan must not introduce — the script already exits 1 on an empty
list, keep that reachable.

Leave `DISPLAY_TEST_JOBS` at 4. The script's own comments record tests losing
contiguous blocks to "display never came up" **under load**, needing three retry
attempts, and one run exhausting a 16 GB tmpfs. Sharding adds real cores;
raising per-runner concurrency buys flakiness.

`run_display_offset` needs no change: each shard is its own runner, so the
`$$ % 16` band cannot collide across shards.

## A2.2 — the `display-tests` matrix job

```yaml
  display-tests:
    name: Display tests ${{ matrix.shard }}/4
    needs: changes
    if: >-
      needs.changes.outputs.suite_skip != 'true' &&
      needs.changes.outputs.display == 'true'
    strategy:
      fail-fast: false
      matrix:
        shard: [1, 2, 3, 4]
    # same pacman list, checkout, cargo cache and env as gnome-suite
    steps:
      - run: scripts/check-display-tests.sh --shard ${{ matrix.shard }}/4
```

`fail-fast: false` is required, not stylistic: the script's whole design is a
balance sheet of *every* failure, and a matrix that cancels its siblings throws
that away.

Then remove `scripts/check-display-tests.sh` from
`.github/scripts/check-gnome-ci.sh`, and add `Rule-owned display tests` to the
skip list in `scripts/ci-quality.sh` — the new job becomes the single owner of
all 852.

**Expected: −708 s from `core-suite`, −846 s from `gnome-suite`; the display
suite itself lands at ~9 min (5.5 min cold build + 3.5 min tests per shard).**

## A2.3 — the `display` route, in the same commit as A2.2

Without this, a `crates/reprise-core`-only change runs **zero** display tests,
where today it runs 583. This is the one way this plan can silently lose
coverage — A2.2 and A2.3 must land together.

`ci-paths.sh` emits a fourth route, derived rather than hand-classified:

```
display = gnome || core
```

Chosen over widening `gnome=true` wherever `core=true`: it is surgical, leaves
`gnome-suite`'s trigger conditions exactly as they are, and keeps the logic in
the tested script instead of in YAML.

Then, in the same commit:

- **`.github/tests/ci-path-routing.sh`** — its `expect_routes` helper reads three
  lines and must read four; every existing expectation gains its `display`
  value. The job-aggregation assertions (lines ~111-143) gain `display-tests` in
  the `Quality gate` `needs:` list.
- **`.github/scripts/require-ci-results.sh`** — it hard-checks `(( $# != 9 ))`
  and requires `result == skipped` whenever a route is `false`. It needs the
  display route and result as two further arguments, and the `ci.yml` call site
  updated to match. A matrix job's aggregated `result` is `success` only when
  every shard succeeded, which is the semantics we want. Check the `suite_skip`
  branch too: it loops over all routes expecting `false` and all results
  expecting `skipped`.

`qa-linters.sh:121-122` and `ci-path-routing.sh:201` need **no** edit — the
`gate "Rule-owned display tests" -- scripts/check-display-tests.sh --rule-named`
line is still there verbatim, only skipped at runtime.

## A2.4 — two caches, and only two

1. `cache: gradle` on `actions/setup-java` in `android-unit-suite`. It has no
   Gradle cache at all today and spends 8.1 of its 12.0 min in the JVM suite.
   Once `core-suite` drops to ~12 min this job is **co-critical**, so this is
   load-bearing.
2. `Swatinem/rust-cache@v2` in **`core-suite` only**. Not `gnome-suite`, not
   `display-tests`, not the APK job.

The store is already at **20.91 GB across 148 entries** and strand `b` adds a
multi-GB Flatpak entry. One `target/` cache is a measurement, four would be
thrashing. `rust-cache` caches dependencies and not workspace crates, which is
what keeps it affordable — do not override that.

The archlinux container installs the **rolling** `rust` package. Confirm
`rust-cache` puts `rustc -Vv` in its key rather than assuming it; a mismatched
restore is worse than none.

**Measure over three consecutive dev pushes before expanding.** If the hit rate
is poor or the store evicts working entries, revert this task alone — A1, A2.1-3
and strand `b` all stand without it.

## Verification

Locally:

1. `scripts/check-display-tests.sh --shard 1/4` (and 2,3,4) — the four
   `failed: N of M` lines must sum to exactly **852** with no test in two shards.
   Diff the union of `== display test:` names against an unsharded `--list`.
2. `scripts/check-display-tests.sh --shard 0/4`, `--shard 5/4`, `--shard 1/0` →
   exit 2, no tests run.
3. `.github/tests/ci-path-routing.sh` green, including a new expectation that
   `crates/reprise-core/src/lib.rs` yields `display=true`.
4. `scripts/tests/qa-linters.sh` green — unchanged assertions on lines 121-122
   must still pass.
5. `scripts/check-shell.sh` green.
6. Reason through `require-ci-results.sh` for all combinations: display routed
   and green, display routed and red, display unrouted and skipped, and the
   whole `suite_skip=true` branch.

Before landing:

7. `gh workflow run ci.yml --ref <branch>`. `workflow_dispatch` sets
   `suite_skip=false` and `emit_routes` with no arguments yields
   `android=true, gnome=false, core=true`, so `display=true` — the dispatch run
   exercises `base-contracts`, `core-suite` and all four shards. Confirm the
   shard test counts sum to 852, `Quality gate` passes, and `core-suite` has
   lost its display phase. `gnome-suite` is **not** exercised by dispatch; it is
   first proved by the dev run after landing.

## Not in this strand

Deleting any test — the 269 non-rule-named display tests now run in this job
like the rest. Raising `DISPLAY_TEST_JOBS`. `rust-cache` anywhere but
`core-suite`. Anything in `release.yml`.
