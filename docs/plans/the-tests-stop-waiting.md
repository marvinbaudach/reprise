---
slug: the-tests-stop-waiting
worktree:
branch:
phase: planned
codex_session:
created: 2026-10-05
strands: a,b,c,d
merge_order: a,b,c,d
---
# The tests stop waiting

Make the test suites and pipelines spend their time on assertions, not on
sleeps, process start-up and rebuilds. Four packages, one strand each:

| Strand | Package | Lever |
|---|---|---|
| a | Display runner | per-test overhead 5.8 s → ~1 s |
| b | Flakes and sleeps | wall-clock races out of the default suites |
| c | Test cleanup | dead, vacuous and duplicated tests |
| d | CI polish | Android profile, cache hygiene, routing, Dependabot, shard count |

This plan **deletes only tests that prove nothing** (named one by one below).
It follows `the-ci-stops-repeating-itself.md` (all three strands shipped:
#774, #777, #800) and keeps its verdict: deleting tests is not a speed lever.

## Why — the measurements (2026-10-04)

### CI (30 `ci.yml` push runs on `dev`)

| Job | median | p90 |
|---|---|---|
| Core and workspace quality suite | 12.3 min | 14.8 |
| Android JVM unit suite | 11.2 min | 13.6 |
| GNOME quality suite | 7.8 min | 8.6 |
| Display-test shard (×4) | 7.7 min | 8.2 |

- Full-run wall clock: median **14.0 min**. Android finished last in **17 of 25**
  successful runs — it is the critical path.
- Core: 6.9 min with a warm rust-cache, 13.6–15.1 min when all 467 crates
  compile; 4 of the last 8 full runs were cold.
- Display shard 2 spent **212 s for 242 tests whose own run time sums to 64 s**.
  Every shard recompiles `reprise-gnome` (301 crates, ~2 min). Shard 1 adds
  2.8 min for the partition proof.
- `stats_23_hiding_more_top_artists_does_not_stall_the_frame_clock` took
  **62.9 s** (runs 37217393510, 37187636027); the next slowest display test 3.4 s.
- One real flake in 30 runs: `podcasts::ytdlp::download_tests::download_passes_audio_only_output_arguments`
  → `HelperStartFailed` (run 37007062525, green on re-run, ~25 min lost).
  One PR-run flake: `bars_fullscreen_render_budget_diagnostic`, p95 16.449 ms
  vs 16 ms (run 37104930985).
- Dependabot: 50 of 59 PR runs failed; 32 of them on "Verify Flatpak Cargo
  sources" (`flatpak/cargo-sources.json` not regenerated). All suites still ran.

### Local probe (loadavg 10–17, so absolute numbers are inflated)

Per display test, median of 5 × 3 tests:

| Variant | Wall |
|---|---|
| today: `cargo test -p reprise-gnome <name> -- --ignored --exact` in the wrapper | **5.8 s** |
| prebuilt test binary executed directly, same wrapper | 4.0 s |
| the wrapper running `true` | 3.2 s |
| `xvfb-run --wait=0` + direct binary | **0.74–1.2 s** |

- The 3.2 s are `/usr/bin/xvfb-run` (Arch, `xorg-server-xvfb 21.1.24`):
  `STARTWAIT=3` (line 32) and an **unconditional `sleep "$STARTWAIT"`** (line 172).
- `cargo test --exact` also runs `tests/gnome_conformance.rs`, which holds none
  of the 972 ignored tests; they all live in the `reprise` bin unit-test target.
- Feature unification: after `cargo test --workspace --no-run`,
  `cargo test -p reprise-gnome --no-run` recompiles **74 crates** and
  `-p reprise-platform-linux` **36** (rusqlite `trace` and `reprise-core/test-fixtures`
  are switched on by dev-dependencies of other workspace members). Paid once
  per fresh target dir — i.e. once per agent worktree and once per CI job.
- stats_23: 84–103 s locally, **~95 % in the fixture**: `insert_artist` opens a
  fresh SQLite connection per `listen_events` row (11,476 rows, 2.9 ms each).
  One connection + one transaction builds the same data in ~1 s.

## The cut

Strands a, b, c are file-disjoint and run concurrently (wave 1). Strand d needs
a's result twice — it edits `scripts/tests/qa-linters.sh`, which a owns, and
its shard-count decision needs a's timings from a real CI run — so d is cut
from the `dev` that a produced (wave 2).

Merge order: **a, b, c** (any order among themselves; a first so d can start),
then **d**.

### Decisions from the grill (2026-10-05)

1. Four strands in two waves, never more than three building at once — not b
   and c folded into one "test code" PR.
2. One cargo selection for the display runner, the merge gate's "Workspace
   tests" selection, everywhere — no CI-only `-p reprise-gnome` switch. The
   extra crates per CI shard are paid back by d's shard reduction.
3. Only an Xvfb that never reports a display is retried. A GTK init failure
   after a reported display is a real failure (noisy over blind).
4. Everything else as recommended in the draft: the production seams in b
   (including the user-visible CLI retry note), c's deletion list, d's
   Dependabot regeneration, the GNOME-skip residue, and d's shard rule.

## Strands

The tasks live in one file per strand; each carries its own status block.

- `the-tests-stop-waiting-a.md` — display runner
- `the-tests-stop-waiting-b.md` — flakes and sleeps
- `the-tests-stop-waiting-c.md` — test cleanup
- `the-tests-stop-waiting-d.md` — CI polish (wave 2)

## Pre-flight

- Strand a's local measurement (task 6) and one CI run of a's branch
  (`workflow_dispatch`, because `scripts/` changes route to no suite) are the
  inputs for d's shard count.

## Post-merge cross-checks

1. After a, b, c: `scripts/check-display-tests.sh --list` count = (count before
   a) − 6: the five re-tagged tests plus the deleted `probe_composed_css_errors`,
   which was itself an ignored display test. Display tests that other commits
   add or remove in between shift the expectation by their own count. The full
   CI display sweep is green with `failed: 0`.
2. After a: a full `workflow_dispatch` CI run — the landing run of a
   `scripts/`-only change proves nothing (routes to no suite).
3. After d: another `workflow_dispatch` run; compare per-job medians with the
   table above over the next 10 dev pushes.

## Out of scope

- Raising local `DISPLAY_TEST_JOBS` above 1.
- rust-cache beyond `core-suite` (pinned "exactly once" by
  `.github/tests/ci-path-routing.sh`; a recorded decision of the predecessor
  plan).
- The 114 `settle_layout()` budgets, `play_recorder_shutdown_tests.rs:214`,
  `stream_proxy_tests.rs:359`.
- New coverage tests — first measure with `cargo llvm-cov`; the static gap list
  (scanner move detection, `library_doctor/local_rules.rs`,
  `tag_write_job/recovery.rs`, `device_sync/device_case.rs`) is name matching
  only.
- Building release artifacts before CI is green (B3 of the predecessor plan).
- Dropping `:app:assembleDebug` (`check-android-suite.sh:158`, 1.1 min on the
  critical path): nothing consumes the APK, but it is the only pre-promotion
  proof that dexing and packaging still work.
- Dependabot Gradle PR failures — real test failures
  (`AlbumCoverBackfillRefreshTest`), not a pipeline problem.
- `check-shell.sh:29-31`'s second shellcheck pass — it checks three extra style
  codes only, not a duplicate.

## Parallelität

- **Strand a** — display runner. Owns `scripts/check-display-tests.sh`,
  `scripts/tests/qa-linters.sh`, `TESTING.md`. Tasks a1–a6.
- **Strand b** — flakes and sleeps. Owns
  `crates/reprise-gnome/src/ui/stats/stats_{bands,songs}_card_tests.rs`,
  `crates/reprise-core/src/podcasts/{ytdlp,ytdlp_test_support,ytdlp_range_tests,ytdlp_process_tests,pipeline_youtube_projection_tests,store_tests}.rs`,
  `crates/reprise-mcp/tests/{source_discovery,source_management}.rs`,
  `crates/reprise-gnome/src/**/add_dialog_followers.rs`,
  `crates/reprise-cli/src/retry.rs`, `crates/reprise-cli/tests/{busy_retry.rs,common/**}`,
  `crates/reprise-android-ffi/src/playback_writer_lock_tests.rs`,
  `crates/reprise-core/src/library/{scanner_import_errors_tests,watcher}.rs`,
  `crates/reprise-core/src/artist_portrait/mod.rs`. Tasks b1–b5.
- **Strand c** — test cleanup. Owns
  `crates/reprise-core/src/library/library_doctor/remote/{diagnostics,mod,arbitration}.rs`,
  `crates/reprise-core/src/playback/bass_pressure_tests.rs`,
  `crates/reprise-core/src/queries/autocomplete.rs`,
  `crates/reprise-platform-linux/src/{location,waveform}.rs`,
  `crates/reprise-gnome/src/ui/style/composed_css_tests.rs`,
  `crates/reprise-gnome/src/ui/strings_podcasts.rs`,
  `crates/reprise-gnome/src/**/{song_visualizer_tests,sidebar_device_card_mirror_tests,concerts_visual_tests,diagnostic_trail_tests}.rs`,
  `crates/reprise-gnome/tests/gnome_conformance.rs`. Tasks c1–c8.
- **Strand d** — CI polish, wave 2. Owns `.github/**`, `scripts/ci-quality.sh`,
  `scripts/check-release.sh`, `scripts/tests/qa-linters.sh` (after a).
  Tasks d1–d7.
- **Disjointness:** a ∩ b ∩ c = ∅ by the globs above (b and c share no file;
  `artist_portrait/mod.rs` is b only, `placeholder_measurement.rs` is untouched).
  d overlaps a in `qa-linters.sh` → wave 2.
- **Merge order:** a, b, c (wave 1, any order), then d.
- **Post-merge cross-checks:** the three listed above. No strand-local
  verification reads a file another strand owns: a compares its `--list` with
  its own before-state, c checks only that its five tests left the list.
