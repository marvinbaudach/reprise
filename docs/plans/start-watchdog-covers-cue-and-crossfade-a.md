---
slug: start-watchdog-covers-cue-and-crossfade-a
worktree: /home/marvin/Projects/reprise-start-watchdog-covers-cue-and-crossfade-a
branch: feature/start-watchdog-covers-cue-and-crossfade-a
phase: planned
codex_session:
created: 2026-11-02
---
# Strand a — the start watchdog covers CUE segment starts

Part of `docs/plans/start-watchdog-covers-cue-and-crossfade.md`. Read the mother
plan first: it holds the context, decisions D1, D4, D5 and D7, and the merge
order.

## File ownership

Write only these files:

- `crates/reprise-platform-linux/src/player/start_watchdog.rs`
- `crates/reprise-platform-linux/src/player/tests/start_watchdog_tests.rs`
- `crates/reprise-platform-linux/src/player/tests/start_watchdog_soak_tests.rs` (new)
- `crates/reprise-platform-linux/src/player/tests.rs` (only the one `mod` line for the new file)
- `crates/reprise-platform-linux/src/player/tests/segment_boundary_tests.rs`
  (only the FIFO test, task A3)

Do **not** write `player.rs`, `segment.rs`, `crossfade.rs` or
`segment_support.rs`. If D1's assumption fails and the fix would need any of them,
stop and report.

## Tasks (test first)

A1. In `start_watchdog_tests.rs`, add
    `play_25_a_hung_cue_start_is_started_over_and_plays_from_its_segment_start`.
    Use `hang_starts(player, 1)` on a CUE item: a whole-file FLAC with a segment
    that starts after zero, built from the fixtures the segment tests already use.
    Assert, in order: the watchdog restarted (the hang probe was released by
    `Null`), `StateChanged(Playing)` arrived, the first heard buffer is at the
    segment start within the segment tests' tolerance, and no
    `PlayerEvent::Error` was emitted. Run it and see it fail, because `covers`
    excludes segments.
A2. Add `play_25_a_cue_start_that_never_finishes_is_given_up_after_three_attempts`:
    every attempt hangs, and exactly one `StartNeverFinished` arrives. Use the
    150 ms test deadline the file already uses.
A3. Drop `segment.is_none()` from `covers` and update its doc comment and its unit
    test. A1 and A2 go green. If A1 shows the segment start is lost across the
    restart (no seek to the segment start, or no `Playing`), stop and report (D1).
A4. D4, the FIFO test in `segment_boundary_tests.rs`. Check whether its stall can
    reach the watchdog's standstill bound (4 s). If it can, set
    `harness.player.start_watchdog.deadline` high enough in that test alone, and
    say why in a comment. What the test asserts stays the same.
A5. D5, the soak test in the new `start_watchdog_soak_tests.rs`, `#[ignore]`d.
    A production `Player` with a fakesink makes N sequential starts per arm,
    alternating whole-file and CUE items. N defaults to 300 and is overridden by
    `REPRISE_START_SOAK_STARTS`. Burner threads (2× `available_parallelism`) spin
    for the whole run. Per start, wait for `StateChanged(Playing)` or
    `StartNeverFinished`. A start that gets neither within the give-up bound plus
    5 s fails the test. Count restarts with a counter the test can read. A
    test-only hook in `start_watchdog.rs` is fine, but grepping logs is not. Print
    one summary line per arm (starts, restarts, give-ups, slowest start). The
    control arm sets `start_watchdog.deadline` out of reach and only reports, it
    does not fail, because hangs are the point of that arm. Run both arms once and
    put the two summary lines in the PR body.
A6. Loop `start_watchdog_tests` and `segment_boundary_tests` 30 times, 10 of them
    under load (12 busy loops), with no failure. Then run the full gate battery
    from AGENTS.md.

## Verification boundary

A6 runs only tests in the files this strand owns. The cross-check against
`play_22_flac_cue_tracks_start_reliably_*` is a post-merge check in the mother
plan. Do not run it here as a pass/fail condition.
