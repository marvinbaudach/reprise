---
slug: the-ci-stops-repeating-itself
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-31
strands: b,a1,a2
merge_order: b,a1,a2
---
# The CI stops repeating itself

Cut the measured 45.9 min from a dev push to a published release down to ~22,
without deleting a single test.

This is the mother plan: shared context, the cut, and the checks no strand can
make alone. The work lives in `-b.md`, `-a1.md` and `-a2.md`.

## Why — the measurement, not a feeling

Reference runs `33379129770` / `33336825284` (sha `0c962a02`), end-to-end commit
`afee939a3`. Everything below is read off job and step timestamps.

The 45 minutes are **two serial stages**:

| Stage | Trigger | Wall clock |
|---|---|---|
| 1. `CI` on the push to `dev` | `push: dev` | **29.4 min** |
| 2. `Release` after the dev→main promotion | `push: main` | **15.6 min** |
| | 21:03:23 → 21:49:18 | **45.9 min** |

Serial by construction: `release.yml`'s `gate` job polls for CI's `Quality gate`
check-run with a 60-minute deadline before either build starts. **That
serialization stays** — see "Out of scope".

### Stage 1 — `core-suite` is the critical path

| Job | Duration |
|---|---|
| Core and workspace quality suite | **29.3 min** ← critical path |
| GNOME quality suite | 22.5 min |
| Android JVM unit suite | 12.0 min |
| Core and Android FFI cross-target | 4.9 min |
| Base and contract checks | 1.6 min (of a 15 min timeout) |

`core-suite` gate timings: Shell 34 s · Project quality 15 s · Worktree GC
(+schedule) 86 s · Gettext 1 s · Script self-tests 122 s · Architecture 51 s ·
11 static contract scripts ~51 s · fmt 4 s · clippy 139 s · doc 25 s ·
**workspace tests 503 s** · Linux platform tests 63 s · **rule-owned display
tests 708 s** · runtime bus 4 s · audit 7 s.

### The duplication, proved

`check-merge-readiness.sh` is a strict superset of `check-gnome-ci.sh`: same 10
contract scripts, `cargo fmt`, clippy (workspace ⊃ 3 packages), `cargo doc`,
`cargo test` (workspace ⊃ `reprise-view` + `reprise-gnome`), the identical
`reprise-platform-linux --test-threads=1` run and runtime-service bus tests.
Display tests are the only difference, and they are a subset:

```
core:  583 display tests (--rule-named)
gnome: 852 display tests (unfiltered)
in core but NOT in gnome:   0     ← strict subset
in gnome but NOT in core: 269
```

`core-suite` spends **708 s re-running tests `gnome-suite` already ran.** Path
routing does not save this: **all four jobs ran on every one of the last 14 dev
pushes.**

### Nothing that matters is cached

`target/` is never cached — 523 crates compiled in `core-suite`, 383 in
`gnome-suite`, every run. `android-unit-suite` has **no Gradle cache at all**
and spends 8.1 of its 12.0 min in the JVM suite. The Flatpak job re-downloads
`org.gnome.Platform//50`, `org.gnome.Sdk//50` and
`org.freedesktop.Sdk.Extension.rust-stable//25.08` every run: 11.1 min.

### Why deleting tests is the wrong lever

`check-display-tests.sh` spawns **one `cargo test --exact` + one
`dbus-run-session` + one `xvfb-run` per test**, four at a time. Measured
~4.0 s/test in gnome, ~4.9 s/test in core, where the assertion is milliseconds.
A deleted display test buys 4 s; the structural work buys ~95 % of 57
worker-minutes. **No test is deleted anywhere in this plan.**

## The constraint that shapes strand A: 27 gate lines that must not move

`scripts/check-merge-readiness.sh` is **not a CI script**. It is the local
pre-push gate — `.githooks/pre-push` runs it directly — and `ci-quality.sh` is
one caller passing `--no-fetch`.

Its 27 `gate "..." -- ...` lines (all at column 0 today) are parsed and asserted
in five places:

| Consumer | Expression | Tolerates indentation? |
|---|---|---|
| `showroom/vite.config.ts:128` `parseGateNames` | `/^\s*gate\s+(["'])([^"']+)\1(?:\s\|$)/gm` | yes |
| `showroom/tests/chapter-two.test.mjs:45` | same regex, derived independently | yes |
| `showroom/tests/chapter-design.test.mjs:25` | `/^gate "[^"]+"/gm` | **NO — anchored, no `\s*`** |
| `scripts/tests/qa-linters.sh:91-122` | ~25 `require_pattern` on individual lines | line must stay verbatim |
| `.github/tests/ci-path-routing.sh:201` | `rg 'check-display-tests\.sh --rule-named'` | line must stay verbatim |

`vite.config.ts` **throws** when the count of `gate` invocation lines and the
count of extracted quoted names disagree; `chapter-two.test.mjs` asserts the gate
count appears nowhere as a literal. So the two regexes disagreeing is a red
build, not a warning.

**Deleting a gate line is out, and so is indenting one.** Wrapping gates in an
`if` block would break `chapter-design.test.mjs` while the other four consumers
stay happy — the worst kind of failure.

### The mechanism: skip inside `gate()`, change no gate line at all

Every one of the 27 lines stays byte-identical at column 0, all five consumers
keep working untouched, and `qa-linters.sh:121-122` and
`ci-path-routing.sh:201` keep passing without an edit. Built in **A1**:

```bash
skipped_here=()

is_skipped() {
  local name=$1 entry
  [[ -n ${MERGE_READINESS_SKIP_GATES:-} ]] || return 1
  while IFS= read -r entry; do
    [[ $entry == "$name" ]] && return 0
  done <<<"${MERGE_READINESS_SKIP_GATES}"
  return 1
}

gate() {
  local name=$1
  shift
  if [[ ${1:-} == -- ]]; then
    shift
  fi
  if is_skipped "$name"; then
    echo "== $name (skipped here; runs in another CI job) =="
    skipped_here+=("$name")
    return 0
  fi
  echo "== $name =="
  "$@"
}
```

Generalises the idiom the file already uses for
`MERGE_READINESS_SKIP_ANDROID_QUALITY` (lines 68-73). The variable is empty for
every local run, so the pre-push hook still runs all 27.

## Target shape

| Job | Change | Now | After |
|---|---|---|---|
| `base-contracts` | + worktree GC, GC schedule, script self-tests | 1.6 | ~5 |
| `core-suite` | 6 gates + display tests skipped; rust-cache | 29.3 | ~12 → ~8 |
| `gnome-suite` | display tests moved out | 22.5 | ~8 |
| `display-tests` ×4 | **new**, owns all 852 | — | ~9 |
| `android-unit-suite` | + Gradle cache | 12.0 | ~8 |
| **stage 1 critical path** | | **29.4** | **~12 → ~9** |
| `flatpak` | SDK cache | 21.8 | ~10 |
| `apk` | unchanged | 15.1 | 15.1 |
| **stage 2** | | **15.6** | **~15** |
| **end to end** | | **45.9** | **~22** |

Note what this exposes: once `core-suite` drops to ~12 min,
`android-unit-suite` at 12.0 min becomes **co-critical**. Its Gradle cache is
therefore part of reaching the target, not a nice-to-have.

## The cut

Two workflow files, not five measures: `ci.yml` is touched by every CI-side
change while `release.yml` is touched only by the Flatpak cache, so a per-measure
cut would collide on both files.

| Strand | Owns | Tasks |
|---|---|---|
| **b** | `.github/workflows/release.yml`, `.github/tests/release-workflow.sh` | Flatpak SDK cache |
| **a1** | `scripts/check-merge-readiness.sh`, `scripts/ci-quality.sh`, `.github/workflows/ci.yml`, `scripts/tests/qa-linters.sh` | `gate()` skip + move 6 gates |
| **a2** | same four, plus `.github/scripts/ci-paths.sh`, `.github/scripts/require-ci-results.sh`, `.github/scripts/check-gnome-ci.sh`, `.github/tests/ci-path-routing.sh`, `scripts/check-display-tests.sh` | display job + route + caches |

### Two waves, because a1 and a2 are not disjoint

`a1` and `a2` share four files, and `a2` needs `a1`'s `gate()` mechanism to skip
`Rule-owned display tests`. They therefore **cannot run concurrently** — the
disjointness rule that governs strands is not satisfied between them.

```
Wave 1 — concurrent, disjoint file sets
  strand b    release.yml                        → PR, lands first
  strand a1   gate() skip + move 6 gates         → PR, smoke run, lands

Wave 2 — only after a1 is on dev
  strand a2   display job + route + rust-cache   → worktree cut from the dev
              that a1 produced, then PR + smoke run
```

`/code` fans out **b and a1 only**. `a2`'s worktree is created after `a1` lands.

### Merge order

**`b, a1, a2`.** `b` first because it is trivially reviewable and carries the
largest single confirmed win (11.1 min) — it must not wait behind the review of
the risky work. `a1` before `a2` by dependency.

## Pre-flight, before any strand lands

The cache store is **already at 20.91 GB across 148 entries** — 50
`release-android-cargo`, 35 `cross-cargo`, 33 `android-cargo`, 22 `cargo-Linux`,
one per `Cargo.lock` hash, never pruned. Strand `b` adds a multi-GB Flatpak entry
and `a2` adds a `target/` entry to that same store.

Prune the stale entries first, keeping the newest per prefix:

```
gh api "repos/{owner}/{repo}/actions/caches" --paginate \
  -q '.actions_caches[] | [.id, .ref, .key] | @tsv'
gh api -X DELETE "repos/{owner}/{repo}/actions/caches/<id>"
```

### Measured 2026-08-31: do not prune — the premise above is wrong

The store had grown to **23.54 GB across 158 entries**, but every single entry
was **at most 5 days old**:

| Age | Entries | Size |
|---|---|---|
| 0 d | 36 | 3.91 GB |
| 1 d | 16 | 2.42 GB |
| 2 d | 46 | 6.84 GB |
| 3 d | 26 | 3.98 GB |
| 4 d | 21 | 3.03 GB |
| 5 d | 13 | 1.93 GB |

Nothing older than 5 days exists, so the entries are **not** "never pruned" —
GitHub's 7-day unused-entry expiry already prunes them, and the store simply sits
at a steady state of roughly one week of CI traffic. The absence of anything near
the 7-day mark also says size-based LRU is not currently biting: 23.54 GB is
under whatever this repo's real cap is, which is therefore well above the 10 GB
figure usually quoted.

So a manual prune would delete only live entries that the next CI run
re-downloads — it costs runner minutes and buys nothing. **Do not prune.**

It would not help strand `b` in any case. `b`'s entry dies of the 7-day expiry,
not of size pressure, and no amount of pruning extends that clock. Post-merge
check 4 is what actually settles whether `b`'s win is real.

## Why PRs prove nothing here, and what to do instead

`ci-paths.sh --suite-skip` returns `true` for every `pull_request`, so **a PR
runs none of the suites it changes.** For `a1` and `a2` the fix is a
`workflow_dispatch` run on the branch before landing:

```
gh workflow run ci.yml --ref <branch>
```

`workflow_dispatch` sets `suite_skip=false`, and `emit_routes` with no arguments
yields `android=true, gnome=false, core=true` — so with the derived route it
exercises `base-contracts`, `core-suite` and all four display shards on the
branch. It does **not** exercise `gnome-suite`; that one is first proved by the
dev run after landing.

## Post-merge cross-checks

Each reads state no single strand owns, so none may be a task's verification
step.

1. **End-to-end wall clock.** After all three land, one dev push, then
   `gh api repos/{owner}/{repo}/commits/<sha>/check-runs` and compare earliest
   `started_at` → latest `completed_at` against the 45.9 min baseline.
2. **Display coverage is whole.** Union the `== display test:` names across the
   four shard logs of a real run; assert exactly **852** and zero duplicates
   against the baseline captured from run `33379129770`.
3. **Cache budget.** `gh api repos/{owner}/{repo}/actions/cache/usage` after
   three dev pushes. `b` and `a2` spend from the same store; only with both
   landed can the eviction rate be judged. If it thrashes, revert the
   `rust-cache` step alone — not `a1`, not `b`.
4. **Strand `b`'s entry survives a realistic release gap.** Size is not the only
   eviction path: GitHub drops any cache entry unused for **7 days**, and `b`'s
   key is touched only by `release.yml` on `main`. Two back-to-back release runs
   will always show a hit and prove nothing about the steady state. Check for a
   hit on a release that follows the previous one by more than a week — list the
   entry with `gh api "repos/{owner}/{repo}/actions/caches" --paginate` and
   confirm it is still there, or read the hit/miss off the run's
   `Restore cached Flatpak runtimes` step. A miss here means the 11 min saving
   only ever materialises for clustered releases, which is a materially smaller
   win than the plan claims.
5. **`require-ci-results.sh` against reality.** Confirm on a real run that a
   `false` route still yields `skipped` for the display job, in the suite-skip
   (PR) case as well as a routed one.
6. **The local gate is still whole.** In a clean checkout of `dev`,
   `scripts/check-merge-readiness.sh` with no env set runs all 27 gates and
   skips none.

## Out of scope

- **Deleting any test.** The 269 non-rule-named display tests stay.
- **B3 — parallelising the Release stage.** Building Flatpak and APK during the
  dev push would remove all 15.6 min from the critical path, more than
  everything here combined, because the promotion is a fast-forward and the
  artifacts would be bit-identical. It is out because it means publishing
  artifacts built before CI was green — the exact guarantee `release.yml`'s
  `gate` job exists to provide. Raise it separately; do not implement it here.
- **`rust-cache` beyond `core-suite`**, the APK `target/` cache, raising
  `DISPLAY_TEST_JOBS` above 4 (the script documents flakiness under load),
  reducing `reprise_core`'s 2653 unit tests, `cross-target.yml`, `pages.yml`.
