---
slug: start-watchdog-covers-cue-and-crossfade-c
worktree: /home/marvin/Projects/reprise-start-watchdog-covers-cue-and-crossfade-c
branch: feature/start-watchdog-covers-cue-and-crossfade-c
phase: shipped
codex_session:
created: 2026-11-02
---
# Strand c — the remaining test starts go through the hang-aware helper

Part of `docs/plans/start-watchdog-covers-cue-and-crossfade.md`. Read the mother
plan first: it holds the context, decisions D3 and D4, and the merge order.

## File ownership

Write only these files:

- `crates/reprise-platform-linux/src/player/tests/segment_support.rs`
- `crates/reprise-platform-linux/src/player/tests/segment_start_tests.rs`
- `crates/reprise-platform-linux/src/player/tests/stream_generation_tests.rs`
- `crates/reprise-platform-linux/src/player/tests/cava_tests.rs`
- `crates/reprise-platform-linux/src/player/tests/segment_prefeed_tests.rs`

No product code changes in this strand.

## Tasks

C1. D3, in `segment_support.rs`: add a variant of `start_over_when_hung` that takes
    a `reset` closure and returns the stream generation read just before the
    attempt that succeeded. The closure runs before every attempt, including the
    first. Keep `start_over_when_hung` with its current signature as a thin
    wrapper (no-op reset, return value dropped), so callers in other files stay
    untouched. Its hang predicate, patience and attempt count stay as they are.
C2. Wrap these starts and keep every assertion exact:
    - `segment_start_tests::play_22_a_zero_start_does_not_flush_*`: the reset
      zeroes the SEGMENT probe counter, and the test still asserts exactly one.
    - `stream_generation_tests`, all four tests: the generation assertions compare
      against the returned baseline. "Strictly greater than the previous start"
      stays strict, with the previous start's value taken from the baseline of the
      attempt that succeeded.
    - `cava_tests::ac_29_flac_playback_emits_spectrum_at_display_cadence`: the
      reset clears the frame count. The 5 s window and the threshold of at least
      40 frames count from the successful attempt.
    - The three plain starts in `segment_prefeed_tests` (`:41`, `:57`, `:72`).
    Leave `play_22_flac_cue_tracks_start_reliably_*` and
    `play_22_a_cue_track_starts_without_blocking_on_its_file` unprotected (D4).
C3. Prove that the reset is needed. Temporarily force one hang into a wrapped
    exact-count test, for example by making its first attempt fail through a
    test-only begin closure. The test must still pass with the reset, and fail
    with a no-op reset. Do not commit the forced hang. Describe the check in the
    PR body.
C4. Loop the five touched test files 30 times, 10 of them under load (12 busy
    loops), with no failure. Then run the full gate battery from AGENTS.md.

## Verification boundary

C4 runs only tests in the files this strand owns. The crossfade generation test
is re-checked against strand b's change after the merge (mother plan,
cross-check 1).
