# Handover — strand A2, "the display tests get their own sharded job"

Written 2026-09-01. Strand `a2` of `docs/plans/the-ci-stops-repeating-itself.md`.

**Worktree** `/home/marvin/Projects/reprise-the-ci-stops-repeating-itself-a2`
**Branch** `feature/the-ci-stops-repeating-itself-a2`
**Phase** `refactored`, rebased onto `origin/dev` and dispatch-proved.
The branch is pushed. Landing is blocked only by defects a2 does not own — see
"Dispatch runs" below.

The plan files for this whole task are **not in the main checkout**. All four
(`the-ci-stops-repeating-itself{,-a1,-b}.md`) live on `origin/dev`; `-a2.md` is
committed on the branch itself. During the session that produced this handover,
the untracked copies in the main checkout's `docs/plans/` disappeared — not by
any action of that session. Read the plan from the worktree, not from `main`.

## State right now

Rebased onto `origin/dev` at `047c8d74cf`, pushed, and dispatch-proved. Commits
on top of `origin/dev`, newest first:

```
a134088b7e test: assert has_tooltip alongside the recorded lazy-tooltip text
f1292d8be8 docs: record why the three display tests were red
6b1ce6a4b0 fix: satisfy clippy's needless_pass_by_value in LazyTooltip::set_text
068a3d4f92 fix: give the activation-id fixture a deterministic sort order
a140e6b317 fix: read the deferred tooltip text two display tests still assert
c30e1da9af ci: verify the shard partition before running it
2b99814a7f docs: record strand a2 dispatch results
1f81292a9d fix(ci): resolve display-test root without git
abe1e1cb5a docs: handover note for strand a2
bb2565aa31 docs: strand a2 phase refactored (third pass)
70e20aa331 fix(ci): derive workflow block indentation
d39926e892 test(ci): pin display matrix to shard axis
4f85fcd839 docs: strand a2 phase reviewed (third pass)
8e5445d23f docs: strand a2 phase refactored (second pass)
d9695890c2 fix(ci): close display shard review gaps
f6c9ca7218 docs: strand a2 phase reviewed (second pass)
a44a9be2ba docs: strand a2 phase refactored
4864be16dc test(ci): verify display shard partition
8e7f2f3e61 test(ci): bind display ownership contracts
a1bd111f86 docs: strand a2 phase reviewed
94f91db357 docs: strand a2 phase coded
76cda02357 ci: cache core builds and Android dependencies
78917dd50d ci: route display tests to sharded jobs
7518afe750 feat(ci): shard display test execution
c1ff114c98 docs: correct strand a2 expected display-test count to the discovered set
c91e4273c0 docs: plan strand a2 — the display tests get their own sharded job
```

The commits below `c30e1da9af` are the original three review rounds, unchanged by
the rebase apart from one conflict in `.github/scripts/check-gnome-ci.sh`: a2
removed the display-test block there, `dev` (#783) removed the runtime
service-bus block, and the resolution keeps neither. Everything above it was
added after the rebase, in response to what the dispatch runs found.

Tree clean, branch pushed. Landing is blocked only on
`feature/dev-gates-go-green` — see "Landing".

## The last run finished

The third refactor pass (S1, S3) completed and is committed. Both were
re-verified independently, not taken from the run's own summary:

- **S1**: adding `os: [a, b]` as a second matrix axis to `display-tests` now
  makes the routing contract test exit **1** (it exited 0 before the fix). The
  two pre-existing guarantees still hold - a literal `/4` back in the `--shard`
  argument and an unwired `DISPLAY_SHARD_COUNT` both fail.
- **S3**: the awk extractor moved out of the shell gate into
  `scripts/lib/extract-workflow-run-blocks.awk` and now derives the body
  indentation from the first content line. All 55 real blocks still extract
  byte-identically; +1 and +4 indented bodies extract correctly.
- The shell gate exits 0; the merge-readiness script still has a 0-byte diff.

Wake locks `ci-a2`, `ci-a2-refactor`, `ci-a2-refactor2` and `ci-a2-refactor3`
were all released.

## What the change does

Moves the 855 display tests out of `core-suite` and `gnome-suite` into a new
4-way sharded `display-tests` matrix job.

- **A2.1** `check-display-tests.sh` learns `--shard K/N` (sorted list, round
  robin `index % N == K-1`) and `--list`.
- **A2.2** the `display-tests` matrix job, `fail-fast: false`, environment
  byte-identical to `gnome-suite`. Display tests removed from
  `check-gnome-ci.sh`; `Rule-owned display tests` added to the skip list in
  `ci-quality.sh`.
- **A2.3** a fourth route, `display = gnome || core`, in `ci-paths.sh`;
  `require-ci-results.sh` grows from 9 to 11 arguments. **Same commit as A2.2 by
  design** — split them and a `crates/reprise-core`-only change runs zero display
  tests where it used to run 583.
- **A2.4** two caches: `cache: gradle` on `setup-java`, and `Swatinem/rust-cache@v2`
  in `core-suite` **only**. Deliberately its own commit so it can be reverted
  alone after the plan's three-dev-push measurement.

### Deviations from the plan, both deliberate

1. **The count is 855, not 852.** 852 was measured at baseline `0c962a02`; three
   display tests landed after it (#757, #759). Coverage grew, nothing was lost.
   `852` appears nowhere in the product — only in the plan. The verification was
   rewritten **relationally** (shard counts sum to the unsharded count, floor of
   `>= 852`) so it cannot rot again. Commit `1d1aaa2a81`.
2. **A2.4 replaced rather than added.** Codex swapped `core-suite`'s existing
   `actions/cache@v6` "Cache Cargo downloads" step for `rust-cache` instead of
   layering both. Defensible — rust-cache is a superset of those paths and also
   caches `target/` — but the plan's text said "add". Still worth a second
   opinion.
3. **`release.yml` was touched**, which the plan's "Not in this strand" excludes.
   One line, `${expected,,}` → `"${expected,,}"` inside `[[ != ]]`. It appeared
   because R6's new linting reaches `release.yml`. Confirmed behaviour-preserving
   (the value is regex-validated hex, so no glob metacharacter is reachable).
   **Open question for the user:** keep it, or scope `release.yml` out of the new
   linting to honour the plan's boundary. Reverting it without a carve-out turns
   the shell gate red.

## Review history — three rounds, all findings tracked

All three rounds are applied. Eight reviewers ran in total: four in round 1, two
in round 2, two in round 3. What remains open was deferred on purpose, not
missed - see the last subsection.

### Applied

| id | what |
|---|---|
| M1+m1 | `DISPLAY_TEST_JOBS: 4` assertion scoped to the `display-tests` block; two dead copies removed; stale comment fixed |
| M2 | every `MERGE_READINESS_SKIP_GATES` entry must byte-match a real `gate "<name>"` |
| m4 | shard-partition check in the `display-tests` job, `if: matrix.shard == 1` |
| R1 | **regression fix** — the M2 check was forward-only, so *deleting* an entry passed. Now asserts presence too |
| R3 | `display-tests` slice no longer depends on `quality:` being the next job |
| R4 | honest "could not parse" message when the skip-list literal is reformatted |
| R5 | shard divisor via `${{ strategy.job-total }}`; `4` written only in the matrix list |
| R6 | `check-shell.sh` now lints all 55 workflow `run:` blocks |
| R7 | skip-list splitting no longer reinterprets backslash escapes |

### Applied — round 3

- **S1 (major, fixed in `269c80d89b`).** `strategy.job-total` counts
  *jobs the matrix generates*, which equals the shard count only while the matrix
  has one axis. **Measured:** adding `os: [a, b]` leaves the routing suite green;
  `job-total` becomes 8, `shard` still runs 1–4, residues 4–7 are never assigned,
  and a quarter of the suite silently stops running. The partition-verify step
  cannot catch it — it loops `1..DISPLAY_SHARD_COUNT` itself, so it proves the
  partition function is self-consistent, never that real jobs cover it. Root
  cause: `ci-path-routing.sh:229-231` matches against the whole file instead of
  the job-scoped `$display_workflow` slice. The assertion now requires the matrix
  to have **exactly one axis**, not merely that `shard:` exists.
- **S3 (major, dormant, fixed in `07cce203cf`).** `check-shell.sh:66` hardcodes
  `substr(line, run_indent + 3)`, assuming a block body indented exactly +2. A
  +1-indented body is valid YAML that CI runs fine, but the extractor chops the
  first character, and the truncated text usually still parses as valid-but-wrong
  bash, so nothing complains. All 55 blocks were +2; the extractor now derives
  the indent from the first content line instead.

### Open — deliberately not applied

- **R2 (major).** `ci-paths.sh` classifies `.github/*` and `scripts/*` as
  no-route, so a change to the sharding logic yields `display=false` and the
  partition check never runs on the commit that changes it. Verified:
  `ci-paths.sh --paths .github/workflows/ci.yml scripts/check-display-tests.sh`
  → all four routes `false`. The nightly `schedule` forces all routes true, so
  drift surfaces within ~24h but not before merge. **Deferred by the user as a
  routing-policy question** — the plan deliberately scoped `display = gnome || core`.
- **S2 (major, dormant).** The run-block extractor is `shell:`-blind: it stamps
  `#!/usr/bin/env bash` on every block. A PowerShell block passes at exit 0
  because it parses as bash. The trigger is not `shell: pwsh` but
  `runs-on: windows-latest`, where pwsh is the default. No non-Linux runners exist
  today.
- **S4 (major, dormant).** `run: &anchor |` and `run: | # comment` drop the body.
  Fails *loudly* with a misleading `SC1070`, so it is a confusing-failure risk,
  not silent coverage loss. Neither shape exists today.
- **Minors:** the unjustified-`# shellcheck disable=` policy skips workflow
  blocks; composite actions (`.github/actions/*/action.yml`) are outside the glob;
  `run: |2-` indentation indicators hit the same drop as S4; `qa-linters.sh:47-48`
  pins `check-shell.sh` internals, which is outside the declared R-scope.

## Verify it yourself — do not trust the summaries

Everything below was proved by direct measurement in the originating session and
should be re-proved after any further change. **Read exit statuses directly; a
pipe reports the last stage's status and is always 0.**

Partition (fast, needs the crate built once):

```
cd <worktree>
./scripts/check-display-tests.sh --list > /tmp/all.txt
for k in 1 2 3 4; do ./scripts/check-display-tests.sh --shard $k/4 --list > /tmp/s$k.txt; done
# 855 == 855, no duplicates, byte-identical union
```

Guards — `--shard 0/4`, `5/4`, `1/0`, `abc/4` and `--bogus` must all exit **2**,
from any working directory. The script used to open with
`cd "$(git rev-parse --show-toplevel)"`, which made every run from outside the
worktree exit 1 before argument parsing; it now derives its root from
`${BASH_SOURCE[0]}` instead, so that trap is gone. The change was forced by CI,
not cosmetic — see "The safe.directory gap" below.

Mutation proofs (`GITHUB_ACTIONS=false .github/tests/ci-path-routing.sh`), each
must exit 1, restore with `git checkout --` afterwards:

- delete `Rule-owned display tests` from `MERGE_READINESS_SKIP_GATES`
- rename the gate label in `check-merge-readiness.sh`
- typo a skip-list entry
- remove `DISPLAY_TEST_JOBS: 4` from the `display-tests` job
- add `os: [a, b]` as a second matrix axis
- inject a shellcheck violation into an embedded `run:` block

Invariant: `git diff origin/dev...HEAD -- scripts/check-merge-readiness.sh` must
be **empty**. The file is untouched, which is the strongest form of the 27-gate
guarantee.

## The safe.directory gap — found by dispatch run 33547425822

The first dispatch run failed all four shards in `Run display-test shard`, before
a single test ran:

```
fatal: detected dubious ownership in repository at '/__w/reprise/reprise'
scripts/check-display-tests.sh: line 3: cd: null directory
```

`check-display-tests.sh` opened with `cd "$(git rev-parse --show-toplevel)"`.
Inside the `archlinux:latest` container git refuses the workspace, the command
substitution collapses to the empty string, and `cd ""` kills the script under
`set -e`. The two sibling gates that used to be its only callers both set
`git config --global --add safe.directory` first — `.github/scripts/check-gnome-ci.sh:8`
and `scripts/ci-quality.sh:8` — so the script had never been reached without that
setup. A2.2 makes the job invoke it directly, which is what exposed the gap.
**No local check could have caught this**: the guard is container-only.

Fixed by deriving the root from `${BASH_SOURCE[0]}`, exactly as
`check-gnome-ci.sh` does. `git` appeared on that one line only, so the script now
has no git dependency at all and needs no `safe.directory` of its own. This also
covers the `Verify display-test shard partition` step, which calls the script
directly too.

## Dispatch runs — what they proved and what still blocks

Two dispatch runs on the rebased branch.

**Run 1, `33547425822`** — all four shards died in `Run display-test shard`
before a test ran. Cause and fix in the section above.

**Run 2, `33549387663`** (after the fix) — the script starts everywhere.
`base-contracts` green, `gnome-suite` correctly skipped, **shard 4/4 fully
green**. Shards 1–3 each fail on exactly one test:

| shard | test | assertion |
|---|---|---|
| 1 | `ui::track_list::track_list_activation::tests::activation_ids_are_reused_until_the_track_model_generation_changes` | `track_list_activation_tests.rs:48` — `[1, 2, 3]` vs `[1, 3, 2]` |
| 2 | `ui::library_doctor::review_row::contract_tests::doc_9b_a_stale_row_names_its_reason_where_the_click_happens` | `review_row_contract_tests.rs:312` — `None` vs `Some("This file changed after the scan …")` |
| 3 | `ui::releases::releases_columns::tests::nr_33_release_link_cell_binds_and_clears_the_visible_affordance` | `releases_columns.rs:560` — `None` vs `Some("https://musicbrainz.org/release-group/mbid")` |

**All three belonged to `dev`, not to a2.** Two independent arguments, both
checked at the time. First: at the commits the two runs tested (`abe1e1cb5a` and
`1f81292a9d`), `git diff --name-only origin/dev...HEAD` contained no `crates/`
path and no `.rs` file at all, so the Rust code under test was byte-identical
with `origin/dev`. *That check no longer holds at the branch tip* — the fixes
below add Rust test changes on purpose; re-run it against those two commits, not
against `HEAD`. Second: each test was re-run locally three times with the shard
script's own invocation form (`dbus-run-session` plus `xvfb-run --server-num`,
`--ignored --exact`), failing deterministically with the same `left`/`right`
every time. No shared root cause in the environment, no Xvfb or font symptom, no
shard or ordering dependency.

They went unnoticed because the display tests had not actually executed in a
`dev` CI run for several pushes: `gnome-suite` aborted at NET-4b before reaching
them, and on `1f8eacadc8` it was skipped by routing. Sharding them into their own
job is what made them visible again.

### All three trace to one commit — `2772c33b7d` (#784)

- **Shards 2 and 3 are test debt.** #784 replaced eager
  `widget.set_tooltip_text(...)` with `ui/lazy_tooltip.rs`, which sets only
  `has-tooltip` and answers `query-tooltip` on demand — X11 makes the eager
  property a synchronous display round trip, too costly inside a virtualised
  `ListItem` bind path. The GTK `tooltip-text` property is therefore `None` by
  design, as `lazy_tooltip.rs`'s own test asserts. Two test files were not
  migrated with it. They now read the text through a `#[cfg(test)]` getter on
  `LazyTooltip`, so they still assert the *text* — DOC-9b says a refused row
  *names its reason*, and `has_tooltip()` alone would weaken that to "some
  tooltip exists".
- **Shard 1 was an accidental dependency on tie order.** #784 added
  `migrate_v82` in `db_sort_indexes.rs`, a partial index on
  `tracks(artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no)`. The
  three fixture tracks carried no artist at all, so every sort column tied and
  the row order fell out of whichever plan SQLite picked: a temp b-tree over the
  title index before v82 (`[1, 3, 2]`), the new index's rowid order after
  (`[1, 2, 3]`). The test's subject is caching — "activation ids are reused until
  the generation changes" — and the id arrays were only a by-product. The fixture
  now gives the three tracks distinct artists, so the order is determined.

### Finding for a separate strand — sort ties are not determined

`SORT_WHITELIST` in `crates/reprise-core/src/queries/clauses.rs` gives only
`album_canonical` a final `, id`. The other eleven entries — `title`, `artist`,
`album`, `track_no`, `genre`, `year`, `duration_ms`, `rating`, `play_count`,
`added_at` — have no tiebreaker, so equal keys order by whatever plan the query
planner picks, and adding an index can silently reorder rows the user sees. That
is what #784 did here. **Not fixed in a2, deliberately:** the parallel
`sort_key_columns()` in the same file drives tag-editing decisions and would have
to change in lockstep, which is a product change with no place in a CI-sharding
strand. Fixing one column would also be arbitrary when eleven share the flaw.

### Blocked on defects a2 does not own

- **NET-4b** — `docs/ux-rules.md:2982` carries `- **NET-4b** [active] [android]`,
  but the UX traceability gate matches only `(core|gtk|e2e|web|manual)` on its
  line 31, so the rule never enters the level map and is reported as untested.
  `core-suite` aborts there. Still unfixed on `origin/dev` at `1f8eacadc8`.
  Another session holds the `fix-dev-gates` wake lock; local notes live in
  `docs/plans/dev-gates-go-green.md`.
- **Android lint** — `ArtistPhotoProgressBarTest.kt:420`,
  `ViewModelConstructorInComposable`. Fails on `dev` itself at `1f8eacadc8`.
- **The three display tests above.**

`quality` fails as a consequence of these; `require-ci-results.sh` reports them
correctly, which is itself evidence the 11-argument signature works.

### The partition check now runs — and runs first

`Verify display-test shard partition` was `if: matrix.shard == 1` and sat *after*
the shard's test step, so one failing display test left it `skipped`. It is this
strand's own proof that the four shards partition the suite, and across runs 1
and 2 it never once executed. It now sits **ahead** of the test step, where it
depends only on the listing.

It first executed in run 3, `33554394762`, and again in run 4,
`33555440025`, which is the one to cite: run 4 tests the branch tip
`a134088b7e`, the commit that actually lands. Both show
`Verify display-test shard partition: success` with all four display shards
green. Run 3's own final state reads `cancelled` — run 4's concurrency group
superseded it *after* every display job had already completed — so quote run 4,
not run 3, or the Actions UI will make the claim look unsupported.

## Landing

**a2's own work is finished and proved.** What is left is not a2's.

Proved on the rebased branch:

- **Run 6 `33561062481` is fully green** — every job: routing,
  `base-contracts`, all four display shards, `Verify display-test shard
  partition`, `core-suite`, `android-unit-suite` and `quality`. `gnome-suite`
  is correctly skipped by routing. This is the run to cite.
- Partition locally: 856 tests, 4 x 214, union byte-identical to the unsharded
  listing, no duplicates. The five argument guards exit 2 from any directory.
- All six mutation proofs exit 1, re-run after each change to `ci.yml`.
- `check-shell.sh` (137 files, 55 workflow run blocks), the routing contract
  suite and `qa-linters.sh` all exit 0.
- `git diff origin/dev...HEAD -- scripts/check-merge-readiness.sh` is 0 bytes.

### Four foreign defects had to clear first

Sharding the display tests is what made them visible: they had not run in a
`dev` CI run for several pushes, and `core-suite` aborted before its later gates.

- **NET-4b** and the **Android lint** were already fixed on
  `feature/dev-gates-go-green`; that branch landed first, as **#788**
  (`1b68764703`), and `dev` went green with it.
- **Three display tests** from #784 (`2772c33b7d`) belonged to nobody. a2 took
  them — see above.
- **A clippy toolchain drift**, found by run 5: `chunks_exact_to_as_chunks` on
  `crates/reprise-core/src/device_sync/snapshot.rs:275`, a file byte-identical
  with `dev`. The CI container's `archlinux:latest` carries clippy 1.98, which
  has the lint; the local toolchain is 0.1.97, which does not, so it cannot be
  reproduced or verified locally. It blocked the "Rust lint" gate and therefore
  every run that routes `core`. Rewritten as `.into_iter().step_by(2)` — the
  same elements, no new API, no MSRV question — and proved element-identical
  against the old expression before committing.

### `--rule-named` is not unexercised — it is deliberately retired

Worth writing down, because it looks like a coverage hole and is not.
`scripts/check-display-tests.sh --rule-named` backs the "Rule-owned display
tests" gate in `check-merge-readiness.sh:143`, and it no longer runs in CI at
all. Run 6's `core-suite` log says so plainly:

```
== Rule-owned display tests (skipped here; runs in another CI job) ==
```

That is A2.2 working as designed: the entry sits in
`MERGE_READINESS_SKIP_GATES` in `scripts/ci-quality.sh`, and the tests it would
select are a strict subset of the 856 the sharded job now runs in full. Nothing
is lost by skipping it, and the M2 mutation proof keeps the skip-list entry
honest — it must byte-match a real `gate "<name>"` line, in both directions
(R1 closed the delete-and-still-pass hole).

So there is nothing to re-run before landing. Two of the three tests this branch
changed (`doc_9b_…`, `nr_33_…`) are rule-named, and they were exercised by
shards 2 and 3 in run 6.

### Order

1. ~~Land `dev-gates-go-green` first.~~ Done — #788, `1b68764703`.
2. ~~Rebase a2 and dispatch.~~ Done — run 6 `33561062481`, fully green.
3. **`land.sh`** — a2 is last in the mother plan's `merge_order` (`b, a1, a2`);
   `b` (#777) and `a1` (#774) already landed. `land.sh` finds the plan by
   `^branch: <BR>$`, which only `-a2.md` carries, so no `--plan` is needed.
4. **Post-merge cross-checks** from the mother plan — the comparisons no strand
   could make alone, which a2 landing is what makes due.

### Review

The commits added after the third refactor pass — the `safe.directory` fix, the
step reorder and the four Rust test commits — were reviewed separately. One
finding was raised and applied: `text_of` reads only the recorded string, not
`widget.set_has_tooltip(...)`, so both contract tests now assert `has_tooltip()`
beside the text. Proved load-bearing by forcing `set_has_tooltip(false)` in
`lazy_tooltip.rs` and watching both tests fail at exactly the new assertions.

This fourth round left **no `phase:` marker commit**, unlike the first three.
That is deliberate, not an oversight: `land.sh` never reads `phase:` as a
precondition — it only *writes* `phase: shipped` before the merge
(`skills/pipeline/scripts/land.sh:115-117`) — and it finds the plan by
`^branch: <BR>$`. The plan file's frontmatter therefore still reads
`phase: refactored` from the third pass.

Accepted as-is: the `#[cfg(test)]` registry in `lazy_tooltip.rs` keys on
`widget.as_ptr()`, so a freed widget's address could in principle be reused. Not
reachable from these two tests — both hold their widget for the whole test and
rebind before every read — and the alternatives are worse: `gtk4::Tooltip` has no
public constructor, so the `query-tooltip` signal cannot be read back, and glib's
`set_data`/`data` are `unsafe fn`. Worth revisiting if a third caller appears.

## Traps that cost time in this session

- **A1's skip mechanism has never executed.** Its landing run `33444057593`
  skipped `core-suite`, `gnome-suite` and `android-unit-suite` — a change touching
  only `scripts/`/`.github/` routes to nothing. So the a2 dispatch run is the
  first real execution of `MERGE_READINESS_SKIP_GATES` inside `core-suite`, and a
  failure there may be A1's bug, not a2's. There is also **no post-A1 baseline**:
  the newest full run is pre-A1 with `core-suite` at 31.4 min, so the plan's
  `−708 s`/`−846 s` are projections, not measurements.
- **dev was red for an unrelated reason** — run `33455355973` at `496a9a5c3`
  failed only in `Android JVM unit suite`. Control arm; check it before blaming
  this branch.
- **The cache store was pruned** by a parallel session (145 entries / 21.16 GB,
  3 newest per prefix kept). Early runs may see prefix fallback rather than exact
  hits, which contaminates the first of A2.4's three-dev-push measurements.
- **The load governor blocks on script *names*.** `heavy-run-gate.sh` matched a
  plain `sed` of `codex-run.sh` and a `grep` of `check-display-tests.sh`. Build the
  path from string fragments to read those files, or go through
  `heavy-run heavy -- …`. `heavy-run` does preserve exit codes (verified: 2 → 2).
- **Background Bash is capped at 10 minutes.** A `timeout: 600000` on a
  `run_in_background` call killed the first Codex run mid-build. Detach long runs
  with `setsid nohup … &` and watch them with a persistent `Monitor`, not a
  backgrounded Bash call.
- **Codex will revert working code to report a blocker** unless told otherwise.
  The first run implemented `--shard` correctly, hit the stale 852, and threw the
  whole implementation away. Every refactor prompt now ends with "commit what you
  have and report the blocker — never revert working code".
