---
slug: the-tests-stop-waiting-d
worktree: /home/marvin/Projects/reprise-the-tests-stop-waiting-d
branch: feature/the-tests-stop-waiting-d
phase: reviewed
codex_session:
created: 2026-10-05
---
# The tests stop waiting — strand d: CI polish

This strand's mother plan was retired once strands a, b and c had landed
(`docs/plans/README.md`). What it carried that still matters here is below:
"Measured inputs" and "Deviations". Touch only the files this strand owns.

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
   - `release.yml` was assumed to run on `main` only and left untouched. It also runs on
     pull requests; see Deviations.
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

## Deviations

What the branch does differently from the tasks above, and why. User decisions
are dated.

- **Task 2, cache policy.**
  - Only `dev` writes the CI caches, as planned. A nightly-on-main writer was added
    and then reverted (2026-10-05): nothing waits on that run, a cold nightly costs
    no runner minutes, and its entries share the 10 GB quota with the ones dev reads.
  - Each explicit `actions/cache/save` hangs on the outcome of the step that filled
    its path, so a failed download is never frozen under the exact key. The Cargo
    registry therefore has its own `cargo fetch --locked` step per job (the Arch
    container jobs fetch `--target x86_64-unknown-linux-gnu`), and the save sits
    right behind it. cargo-xwin's save hangs on its install step. This adds a step
    the plan did not name and is unverified until a dispatch run.
  - rust-cache also sets `cache-on-failure: true`, which the plan did not.
  - `release.yml` was declared untouched ("runs on `main` only"), but it also runs on
    pull requests. Its Flatpak-runtime and Android Cargo caches are now a restore plus
    a save that runs on push events only.
  - `.github/tests/ci-cache-writes.sh` pins all of this over every workflow. The
    `setup-*` built-in caches (`npm`, `gradle`) have no save switch and stay as an
    accepted residue, pinned by count.
- **Task 4, route BOTH (2026-10-05).** The regeneration push and the containment
  fallback both exist.
  - The token's `contents:write` pre-check was not done: only a real push proves it.
  - Containment is not `suite_skip`. A Dependabot pull request whose Flatpak sources
    are stale loses its suites, keeps `base-contracts`, and the Quality gate stays red:
    `ci.yml` `changes` and `cross-target.yml` `suite-skip` emit a separate `contained`
    output, and `require-ci-results.sh` takes it as a twelfth argument and fails on it.
    `suite_skip` would skip `base-contracts` and turn the gate green.
  - Hardening beyond the plan: the push validates with the base commit's validator, the
    push job runs no `uses:`, the regenerate job checks out by commit, setup-uv is pinned
    by commit, and the token reaches git as a masked HTTP header instead of a URL.
  - `scripts/check-flatpak-cargo-sources.sh` now validates content (crates.io URL and
    checksum), which the plan's ownership did not list; the push is only as safe as that check.
  - `ACTOR` for suite routing is `pull_request.user.login || github.actor` in `ci.yml` and
    `cross-target.yml`. Without it the token push would skip every later run's suites.
- **Task 2, accepted residue and consequences (D1, D5, R16, R17).**
  - The exception to "only `dev` writes" is exactly three built-in caches: `setup-node`'s `npm` (twice in
    `ci.yml`) and `setup-java`'s `gradle`, plus `npm` in `pages.yml`. They have no save switch, are small, and
    are pinned by count in `ci-cache-writes.sh`. `astral-sh/setup-uv` is **not** an exception: it caches on
    every ref by default, so each `ci.yml` step now saves on `dev` only, and the contract requires either
    `save-cache` on the writer's condition or `enable-cache: false`.
  - A save sits directly behind the step that fills its path, and the contract pins the position. The Flatpak
    runtime verification is the one step allowed in between; a failed verification must not be cached.
  - **Accepted (R16).** After this change only a push saves `release.yml`'s caches, so its pull-request runs
    start mostly cold. The Flatpak runtime key has no hash and the install uses `--or-update`, so a hit is never
    refreshed. A Flatpak job is skipped on a main push that does not publish, so saves are rare and expire
    after 7 idle days.
  - **Post-merge check (R17).** A run on `main` cannot read `dev`'s caches, so once `main`'s own entries expire
    the nightly builds cold, against `core-suite`'s 60-minute timeout. Nobody has measured a cold `core-suite`.
    After the merge, read the duration of the first cold nightly's `core-suite` and raise the timeout or give
    the nightly a writer if it comes close.
- **Task 3 and the Android job.** `android-unit-suite` runs `cargo fetch --locked` without `--target`, unlike
  the Arch container jobs (`--target x86_64-unknown-linux-gnu`), so its registry cache holds every target.
- **Task 4, shape.** The plan said "a new job (or a step) … install `aiohttp tomlkit`". The branch has two jobs,
  `regenerate` (no secret) and `push` (holds the token), joined by an artifact.
  - The generator's dependencies come from a hashed lock, `.github/flatpak-cargo-generator.lock`, cut at
    2026-09-28 (more than a week before the change). `uv run --locked` refuses a file whose hash differs and a
    lock that does not match the generator; nothing is resolved at run time. An earlier revision only froze
    resolution at the day of writing, which is no cooldown.
  - The generator output stays byte-identical to the committed `flatpak/cargo-sources.json`.
  - The generator, the lock and the uv version are pinned together: moving one is a reviewed change.
  - Every action in the workflow is pinned by commit, and the `push` job runs none.
  - `ci-path-routing.sh` and `release-workflow.sh` pins were edited, although the plan says to edit a pin
    only where the task says so: the first for the containment route, the second for the Flatpak cache step.
- **Task 4, stale sources fail the Quality gate for every pull request (R6).** `changes` runs
  `check-flatpak-cargo-sources.sh` on every `pull_request` run, and `ci-paths.sh --contain` no longer asks who
  wrote it. Before, a human pull request skipped `base-contracts` as suite reuse, so nothing ran the check and
  its gate was green while `dev` went red after the merge. This changes behaviour for every PR, not only for
  Dependabot's. `cross-target.yml`'s `suite-skip` job runs the check only when it is not suite reuse,
  because a suite-reuse run skips the compilation anyway and `ci.yml` has already judged it.
- **Task 4, Dependabot stops rebasing (T3).** After the bot's commit Dependabot treats the pull request as
  edited and stops rebasing it. A conflicting or stale bump is recovered with `@dependabot recreate`, which
  rebuilds the branch and re-triggers the regeneration. The workflow header says so too.
- **Task 4, review round 3.** The `push` job is pinned command by command in the contract, which also runs its
  real steps against a throwaway repository with only `git push` stubbed. That covers the loop guard against the
  commit step, symlinks at `flatpak` and at the sources file, a stray path, and tracing never printing the
  base64 header. Suite routing's `ACTOR` is pinned per deciding step.
- **Task 5.** N=2 is extrapolated, not measured. This branch's dispatch run is the confirmation.
- **Task 6.** `github-flow.sh` and `.github/tests/flatpak-cargo-sources.sh` stay in the
  `qa-linters.sh` tail: the merge gate has no other call for them. CI skips
  `flatpak-cargo-sources.sh` in its contract loop and drops its direct `github-flow.sh` call, so
  nothing runs twice. The stale comment in the merge-readiness script was updated; no
  parallel strand owns that file any more.
