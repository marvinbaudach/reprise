---
slug: the-tests-stop-waiting-d
worktree: /home/marvin/Projects/reprise-the-tests-stop-waiting-d
branch: feature/the-tests-stop-waiting-d
phase: refactored
codex_session:
created: 2026-10-05
---
# The tests stop waiting — strand d: CI polish

Mother plan: `docs/plans/the-tests-stop-waiting.md`. Read its "Why", "The cut"
and "Decisions from the grill" before starting. Touch only the files this
strand owns.

**Wave 2.** Cut this strand's worktree from the `dev` that strand a produced — not before.

## Strand d — CI polish (wave 2, cut after a lands)

**Owns:** `.github/**`, `scripts/ci-quality.sh`, `scripts/check-release.sh`,
`scripts/tests/qa-linters.sh` (inherits it from a).

Contract pins every task must keep green (all exit 0 today):
`.github/tests/ci-path-routing.sh`, `.github/tests/repo-wide-gates-are-unrouted.sh`,
`scripts/tests/github-flow.sh`, `scripts/tests/qa-linters.sh`,
`.github/tests/dependabot-targets.sh`. Edit a pin only where the task says so.

1. **Android debuginfo off.** Add `CARGO_PROFILE_DEV_DEBUG: 0` and
   `CARGO_PROFILE_TEST_DEBUG: 0` to `android-unit-suite`'s env (`ci.yml:126-131`),
   as every container job has. Affects the `cargo test -p reprise-view -p
   reprise-android-ffi` step (`ci.yml:156`) only; the release build in
   `check-android-suite.sh:145` is unaffected. `DISPLAY_TEST_JOBS: 1` stays
   (pinned by `github-flow.sh:48` and `qa-linters.sh:213`). No pin changes.
2. **Caches are written on `dev` only.** Only Dependabot PR runs reach the
   suites on PRs; they hold 9.6 GB of PR-scoped entries that `dev` can never
   read, while `dev` currently holds no `v0-rust`/`cargo-Linux` entry at all.
   - `Swatinem/rust-cache@v2` (`ci.yml:258`): `with: save-if: ${{ github.ref == 'refs/heads/dev' }}`
     (the "exactly once" count pin is unaffected).
   - Every `actions/cache@v6` in `ci.yml` (146, 199, 305) and `cross-target.yml`
     (92, 103): split into `actions/cache/restore@v6` + `actions/cache/save@v6`
     with `if: github.ref == 'refs/heads/dev'`, same key and path block (keeps
     the `github-flow.sh:50-51` path pins).
   - setup-java's `cache: gradle` has no save switch and its block is pinned
     verbatim — left alone; named here so nobody "forgets" it.
   - `release.yml` caches run on `main` only — untouched.
3. **The GNOME suite does not repeat the core suite.** In
   `.github/scripts/ci-paths.sh` `emit_routes`, after the loop: `core=true` ⇒
   `gnome=false` (`display` stays `gnome || core`, computed before the
   override). The aggregator (`require-ci-results.sh`) then sees route false ⇒
   skipped — no aggregator change. Add `expect_routes` cases to
   `ci-path-routing.sh` for a mixed core+gnome path set and for
   `reprise-view` + `reprise-core`. The `--diff` main/schedule expectation
   `true true true true` changes to `true false true true` — say so in the
   commit. A gnome-only change still runs the GNOME suite.
   Known residue: core tests `reprise-gnome` with `--workspace` features, so a
   failure that only appears in the `-p` feature set is no longer caught when
   both route. Accepted; the display shards build the same workspace selection
   after strand a.
4. **Dependabot Cargo PRs regenerate their Flatpak sources.** New job (or a
   step in `dependabot-automerge.yml`) for `dependabot[bot]` PRs that touch
   `Cargo.lock`: fetch `flatpak-cargo-generator.py` at a pinned commit with a
   recorded sha256, install `aiohttp tomlkit`, run
   `flatpak-cargo-generator.py Cargo.lock -o flatpak/cargo-sources.json`, and
   push the result to the PR branch with `REPRISE_AUTOMERGE_TOKEN` (a token
   push re-triggers CI; a `GITHUB_TOKEN` push would not). The generator only
   parses `Cargo.lock` — no PR code runs with the token. Registry-only lock
   files make it offline and deterministic. Contract test in `.github/tests/`
   pinning the trigger, the actor guard and the pinned generator hash.
   52 of the 70 sampled failed Dependabot runs were this one check.
   First verify the token may push to `dependabot/**` branches (it is used for
   `gh pr merge` today, which needs less). If it cannot, fall back to
   containment only: the `changes` job runs the Flatpak-sources check for
   Dependabot PRs and sets `suite_skip` when it fails, so the suites stop
   burning ~20 runner-minutes on a PR that is red anyway — and say so in the PR.
5. **Display shard count from measurement.** From strand a's
   `workflow_dispatch` run: choose the smallest `N ∈ {1, 2, 4}` whose slowest
   shard (build + partition proof + tests) stays ≤ 8 min, i.e. clearly under
   the Android job's 11.2 min median. Edit the matrix and the shard pin/message
   in `ci-path-routing.sh` (~230) accordingly; everything else derives from
   `strategy.job-total`. `DISPLAY_TEST_JOBS: 4` per shard stays.
6. **No script runs twice per gate pass.** `qa-linters.sh:262-286` re-executes
   `github-flow.sh`, `.github/tests/flatpak-cargo-sources.sh`,
   `worktree-gc.sh`, `worktree-gc-schedule.sh` and `check-architecture.sh`, all
   of which base-contracts and the merge gate already run on their own. Drop
   those five from the tail and call them directly from
   `scripts/check-release.sh` (its only path to them was via `qa-linters.sh:13`).
   The remaining tail entries stay.
7. **Release metadata once in CI.** Add `Release metadata` to
   `MERGE_READINESS_SKIP_GATES` in `scripts/ci-quality.sh` (base-contracts runs
   it at `ci.yml:79`); `ci-path-routing.sh` already verifies every skip entry
   names a real gate.

Verification: every contract test above, `scripts/check-shell.sh`, and a
`workflow_dispatch` run of the branch (CI-infra changes route to no suite).

## Measured inputs (2026-10-05)

- **Before**, run 37312084327 (push, dev `037eb5da48`, before #1116): four display shards
  of 7.97, 8.18, 7.98 and 8.00 min. The old runner compiled per test, which took ~3 min of
  build and ~4 min of execution per shard. 979 display tests, `failed: 0`, 5 measurement
  tools skipped.
- **After**, run 37334157056 (`workflow_dispatch`, dev `8d11e44770`, after a, b and c):
  four display shards of 5.98, 5.88, 5.83 and 3.85 min. Shard 4 ran on a faster runner.
  Per shard, setup (containers, system dependencies, checkout) takes ~1 min and the
  `cargo test --no-run` build ~4.1–4.2 min (2m23 on the fast runner). Executing the 243–244
  tests then takes only 30–48 s at `DISPLAY_TEST_JOBS: 4`, so all four shards together
  spend ~170 s executing.
- **Task 5 extrapolation** (not measured), slowest shard: N=1 ≈ 1 + 4.2 + ~3 ≈ 8.2 min,
  which is at the 8 min ceiling. N=2 ≈ 1 + 4.2 + ~1.5 ≈ 6.7 min. N=4 is measured at 6.0 min.
  The rule therefore points at N=2, unless a measured N=1 run stays under 8 min.
- **Android JVM unit suite:** 13.85 min in the after run and 13.82 min before. The median of
  eight successful runs on 2026-10-05 is ~13.4 min. Task 5's 11.2 min matches only one
  run, 37261721361.
- **Routing:** a dispatch routes `gnome=false` by design (`.github/scripts/ci-paths.sh:11-13`).
  The core suite's complete workspace gate covers the GNOME crate, so the skipped GNOME
  job is not a gap.
