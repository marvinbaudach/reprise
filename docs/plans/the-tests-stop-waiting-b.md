---
slug: the-tests-stop-waiting-b
worktree: /home/marvin/Projects/reprise-the-tests-stop-waiting-b
branch: feature/the-tests-stop-waiting-b
phase: planned
codex_session:
created: 2026-10-05
---
# The tests stop waiting — strand b: flakes and sleeps

Mother plan: `docs/plans/the-tests-stop-waiting.md`. Read its "Why", "The cut"
and "Decisions from the grill" before starting. Touch only the files this
strand owns.

## Strand b — flakes and sleeps

**Owns:** the files named in its tasks — test files plus four narrow production
seams (`podcasts/ytdlp.rs`, `reprise-cli/src/retry.rs`,
`library/watcher.rs`, and the cli test harness).

Rule-named tests keep their names (`stats_23_*`, `stats_24_*`, `pod_6_*`,
`pod_10_*`, `pod_22_*`, `pod_13_*`, `pod_7_*`).

1. **stats fixtures.** `stats_bands_card_tests.rs:554-584`: one
   `test_db::connection` + `unchecked_transaction()` + prepared statement +
   `commit()` for the whole fixture (pattern:
   `delete_tracks_large_block_display_tests.rs:20-35`). Same for
   `stats_songs_card_tests.rs:64,79` (~104 opens per fixture). Optionally build
   the DB/snapshot once per test and clone the `StatsSnapshot` (it is `Clone`).
   Target: stats_23 tests under 5 s each.
2. **yt-dlp ETXTBSY.**
   - One shared helper writes fake executables **through a child process**
     (`sh -c 'printf "%s\n" "$1" > "$2" && chmod 755 "$2"' _ body path`, argv,
     not stdin), waited to exit — the test process never holds a write fd a
     sibling fork could inherit.
   - Route all writers through it: `ytdlp_test_support.rs:12-19` (38 call
     sites), the duplicate in `ytdlp_range_tests.rs:8-15`,
     `pipeline_youtube_projection_tests.rs:44-46`,
     `reprise-mcp/tests/source_discovery.rs:154`, `source_management.rs:137`,
     `reprise-gnome/.../add_dialog_followers.rs:292` (crate-local copies where a
     shared helper cannot cross the crate boundary).
   - Retry test (`ytdlp_process_tests.rs:106-122`) without a clock: extract
     `spawn_retrying_busy(cmd, retries, delay, on_busy)` from `ytdlp.rs:255-266`;
     `run()` passes the constants and a no-op. Test A: `on_busy` drops the held
     writer on the first call, `delay = ZERO` → `Ok`. Test B: never released,
     `retries = 2` → `ExecutableFileBusy`, `on_busy` called exactly twice.
3. **busy_retry.** `with_retry` (`retry.rs:13-59`) prints
   `note: database busy, retrying (attempt N/5)` to stderr before each backoff.
   `busy_retry.rs`: the holder signals on a channel after its INSERT, only then
   the CLI is spawned (piped stderr); the holder commits when the first retry
   line arrives. Assert exit 0 **and** at least one retry line — today the test
   can pass without ever exercising the retry.
   The note is user-visible CLI output: English, one line per retry, on stderr
   only (stdout stays machine-readable). Before adding it, check that no CLI
   test or doc expects an empty stderr on a successful write.
4. **Writer-lock budgets.**
   - `reprise-android-ffi/src/playback_writer_lock_tests.rs` (`:23,114,152,171`):
     the holder blocks on a release channel instead of sleeping 1.5 s; the
     transport runs on a helper thread and must report completion
     (`recv_timeout(10 s)`) **while the writer is still held**; no
     `elapsed < 300 ms`.
   - `reprise-core/src/podcasts/store_tests.rs:447-556` (`pod_6_…`): a test-local
     busy handler counts entries; the holder commits only after the counter is
     ≥ 1, signalled by a channel; the `release_at` timing asserts go.
5. **Clock sleeps.**
   - `scanner_import_errors_tests.rs:59` (1.1 s): back-date `first_seen`/`last_seen`
     by 100 s with an UPDATE between the 4th and 5th scan.
   - `artist_portrait/mod.rs:367` (1.1 s): back-date the cached file's mtime with
     `File::set_times` right after `store_image`.
   - `library/watcher.rs:454-466`: `ignore_path_at`/`is_ignored_at` taking an
     `Instant`; the public functions delegate with `Instant::now()`; the test
     uses synthetic instants.
6. **Not in this strand** (needs a seam whose benefit is unclear; listed so the
   reviewer does not report them as forgotten): `play_recorder_shutdown_tests.rs:214`
   (2.5 s negative wait), `stream_proxy_tests.rs:359` (1 s, mechanism
   uncertain), the 114 `settle_layout()` 200 ms budgets (each needs its own
   condition).

Verification: the touched test binaries pass 20 × in a row under `heavy-run`
(`cargo test -p <crate> <filter>` per item), the stats_23/24 display tests pass
via `scripts/check-display-tests.sh` (or their exact names in the isolated
wrapper), and each deterministic rewrite gets a mutation proof (break the
production behaviour → the test fails).

