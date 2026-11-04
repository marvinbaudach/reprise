---
slug: start-watchdog-covers-cue-and-crossfade-b
worktree: /home/marvin/Projects/reprise-start-watchdog-covers-cue-and-crossfade-b
branch: feature/start-watchdog-covers-cue-and-crossfade-b
phase: shipped
codex_session:
created: 2026-11-02
---
# Strand b — a crossfade never promotes an incoming track that has not started

Part of `docs/plans/start-watchdog-covers-cue-and-crossfade.md`. Read the mother
plan first: it holds the context, decisions D2 and D7, and the merge order.

## File ownership

Write only these files:

- `crates/reprise-platform-linux/src/crossfade.rs`
- `crates/reprise-platform-linux/src/player/tests/crossfade_transition_tests.rs`

Do **not** write `start_watchdog.rs`, `player.rs`, `segment_support.rs` or
`start_watchdog_tests.rs`. Do not import from `start_watchdog`. The 4 s bound is
a constant in `crossfade.rs`, with a comment that names `START_DEADLINE` as its
counterpart. The hang injector is this strand's own: write it in
`crossfade_transition_tests.rs`, and do not reuse `hang_starts`.

## Tasks (test first)

B1. In `crossfade_transition_tests.rs`, add a hang injector for the crossfade
    secondary. It catches the secondary playbin's gain element through
    `deep-element-added`, or through whatever hook the secondary already
    exposes to tests, and blocks its sink pad with a BLOCK_DOWNSTREAM probe that
    `Null` releases. That is the same technique as `hang_starts`, written
    locally.
B2. Add `play_26_a_crossfade_whose_incoming_track_hangs_falls_back_to_a_plain_track_change`.
    With crossfade enabled and the next track queued, the secondary hangs.
    Assert: no promotion happens, the outgoing track keeps full volume and plays
    to its end, the queued track then starts through the ordinary start and plays,
    and no `PlayerEvent::Error` is emitted. Run it and see it fail: today the hung
    secondary is promoted.
B3. Add `play_26_a_crossfade_whose_incoming_track_prerolls_slowly_still_fades`. A
    secondary that is delayed but healthy, prerolling inside the bound, fades and
    is promoted as before. Delay it, for example with a pad probe that holds the
    first buffer for 1 s.
B4. Implement D2 in `crossfade.rs` `run`. After `set_state(Playing)`, wait for
    preroll with `secondary.state(timeout)` on the crossfade thread. The bound is
    the shorter of 4 s and the time left until the outgoing track ends. Past the
    bound, take the existing abort path (secondary to `Null`, `crossfading`
    cleared) and emit one `tracing::warn!` with the reason. The ramp and `promote`
    run only after a successful preroll. While the thread waits, the
    `generation` check must still stand it down when a manual jump happens.
B5. Loop `crossfade_transition_tests` 30 times, 10 of them under load (12 busy
    loops), with no failure. Then run the full gate battery from AGENTS.md.

## Verification boundary

B5 runs only the tests in the file this strand owns. Whether
`stream_generation_tests::crossfade_promotion_*` (strand c's file) still holds
with the new wait is a post-merge check in the mother plan, not a condition here.
