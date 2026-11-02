---
slug: start-watchdog-covers-cue-and-crossfade
worktree:
branch:
phase: planned
codex_session:
created: 2026-11-02
strands: a,b,c
merge_order: c,a,b
---
# A hung start recovers on CUE tracks and crossfades too (#1239, remainder)

Mother plan. The work runs in three strands, `-a`, `-b` and `-c`. Each strand file
carries its own tasks and file ownership. This file is frozen once the plan phase
ends.

## Goal

#1263 and #1267 gave whole-file local starts a watchdog. A `playbin3` start that
stays in READY while its `filesrc` stands still is started over from `Null`. It
is allowed 4, 8 and 12 s of standstill. After three attempts it is reported as
`PlaybackFailureKind::StartNeverFinished`, and the frontend skips the track. #1239
stays open for the starts the watchdog does not see, for test starts that are
still unprotected, and for the missing evidence that a real hang (not an
injected one) is recovered. This plan closes all three and then closes #1239.

**Out of scope, as follow-up issues filed in task D:**
- The GStreamer root cause: decodebin3 is still in READY when the stream arrives,
  under CPU load, with many pipelines in one process (GStreamer 1.28.6).
- `restart` calls `set_state(Null)` on the main context while it holds the
  playbin lock. A source blocked inside a kernel read, such as a dead network
  share, makes that call wait, and the UI waits with it. The fix would rebuild the
  playbin and tear the stuck one down on a reaper thread.

## What the code does today (dev 5c433c6529)

- `start_watchdog::covers(uri, live, segment)` (`player/start_watchdog.rs:98`)
  returns `false` for every CUE segment, so `play_resolved_uri`
  (`player.rs:412`) never arms the watchdog for one.
- A CUE start runs on the same main playbin. `try_play` returns early into
  `segment::start_segment` (`player.rs:375`), which calls `gate.begin_start` to
  set `pending_start_ms`, sets `Paused`, and bumps the stream generation
  (`segment.rs:454-474`). `SegmentGate::complete_start` (`segment.rs:277`) seeks
  to the segment start on the preroll `ASYNC_DONE` and then sets `Playing`.
  `SegmentGate::reset` runs only from `Player::reset_transition`
  (`player.rs:326`), not on a state change, so the gate's pending start survives
  a `Null` → `Paused` restart. A hang leaves the playbin at `Ready` with `Paused`
  pending, which is exactly what `judge` already treats as hung.
- The crossfade secondary (`crossfade.rs:167 run`) is a separate playbin on its own
  thread. It is set to `Playing` (`:222`) and the volume ramp starts at once,
  without checking its state. If the secondary hangs, the ramp fades the outgoing
  track out over silence. `promote` (`:260`) then makes the hung pipeline the main
  one, and the listener hears nothing until they skip. The existing abort path,
  where the secondary refuses `Playing` (`:222-226`), sets the secondary to
  `Null` and clears `crossfading`. The outgoing track then plays out and the
  ordinary next-track start follows.
- `start_over_when_hung` (`player/tests/segment_support.rs:399`) repeats `begin()`
  up to 5 times when the playbin has not left READY within 5 s. It returns
  nothing. Callers live in many test files, including some that strands a and b
  own.

## Decisions (settled in the grill, 2026-11-02)

- **Scope.** CUE starts, the crossfade incoming start, the remaining unprotected
  test starts, and the soak evidence. Nothing else.
- **D1 — CUE uses the same watchdog with the same restart.** Only `covers` changes:
  CUE segments are no longer excluded. The restart stays `Null`, then the pending
  state (`Paused` for a segment start). The gate's pending start survives it, so
  `complete_start` finishes the start on the next preroll. Strand a's first test
  proves this. **If it does not hold, the strand stops and reports instead of
  rebuilding the restart.** Neither a per-start restart closure nor a second
  watchdog is in scope.
- **D2 — Crossfade waits for preroll and otherwise falls back.** After setting the
  secondary to `Playing`, the crossfade thread waits for it to preroll before the
  ramp starts. The bound is the shorter of 4 s and the time left until the
  outgoing track ends. Past the bound, the fade is abandoned through the existing
  abort path: secondary to `Null`, `crossfading` cleared, one `warn!` naming the
  reason. The outgoing track plays to its end at full volume, then the ordinary
  next-track start follows, and the watchdog covers it. A pipeline that has not
  prerolled is never promoted. The wait runs on the crossfade thread, never on the
  main context.
- **D3 — A new test helper resets and returns its baseline.** Strand c adds a
  variant next to `start_over_when_hung`. It takes a `reset` closure, which runs
  before every attempt and clears the test's counters and collected events. It
  returns the stream generation read just before the attempt that succeeded.
  `start_over_when_hung` stays, as a thin wrapper over the variant with a no-op
  reset, so no caller in another strand's files changes. Exact assertions stay
  exact. Nothing is loosened to `>=`.
- **D4 — Two tests stay deliberately unprotected.**
  `play_22_flac_cue_tracks_start_reliably_*` measures start reliability itself.
  The FIFO in `play_22_a_cue_track_starts_without_blocking_on_its_file` can be
  read only once. After D1 the product watchdog watches both. The FIFO test must
  not have its stall mistaken for a hang. Strand a checks whether the stall stays
  under the watchdog's standstill bound. If it does not, the test raises
  `harness.player.start_watchdog.deadline`. What the test measures stays the same.
- **D5 — Soak evidence with a control arm.** An `#[ignore]`d soak test: a production
  `Player` makes N sequential local starts, whole-file and CUE (default 300 per
  arm, overridable by env), while burner threads saturate the CPU at twice the
  core count. Every start must end in `StateChanged(Playing)` or
  `StartNeverFinished`, and none may stay silent past the give-up bound. The test
  prints the number of restarts the watchdog performed and any give-ups. The
  control arm raises the watchdog deadline out of reach. Both arms' numbers go
  into strand a's PR. The gates do not run it.
- **D7 — UX rule PLAY-25, as a proposal.** The ID is reserved now, so the new tests
  in strands a and b carry `play_25` in their names. The rule text is written in
  task D, after the merge, with the `<!-- REVIEW: rule proposal -->` marker.
  Proposed text: "A local track whose start never completes is started over up
  to three times and then skipped. This includes CUE tracks. When the incoming
  track of a crossfade has not started in time, the crossfade gives way to a plain
  track change." Status: `[planned]`, until the user activates it.

## The cut

| Strand | File | Purpose |
|---|---|---|
| a | `docs/plans/start-watchdog-covers-cue-and-crossfade-a.md` | CUE watchdog, soak |
| b | `docs/plans/start-watchdog-covers-cue-and-crossfade-b.md` | crossfade fallback |
| c | `docs/plans/start-watchdog-covers-cue-and-crossfade-c.md` | test protection |

File ownership is disjoint. The globs are listed in each strand file, and no file
appears in two strands. A strand may read any file, but it writes only the files
it owns, and its verification reads only behaviour those files produce.

**Merge order: c, a, b.** Every order compiles. c goes first because it only
hardens tests, which makes the dev suites under a and b less flaky. a goes before
b because the rule text names the CUE behaviour first.

## Post-merge cross-checks (on dev, after all three have landed)

1. `stream_generation_tests::crossfade_promotion_carries_a_newer_generation_*`
   (c's file) still passes 30 times with b's preroll wait in front of the
   promotion.
2. `play_22_flac_cue_tracks_start_reliably_*` (untouched by every strand) passes 30
   times, 10 of them under load (12 busy loops), now that a's watchdog covers CUE
   starts.
3. The whole `reprise-platform-linux` test suite passes 10 times in a loop under
   load.

## Task D — record and close (after the cross-checks, on dev)

D1. Write PLAY-25 into `docs/ux-rules.md`, after PLAY-24, as a `[planned]` rule
    proposal with the text above. Its rule-named tests are strand a's and strand
    b's `play_25_*` tests. Follow the rules document's introduction for the
    proposal format.
D2. Comment on #1239 with the soak numbers from strand a's PR, both arms. File the
    two out-of-scope follow-up issues: the root cause, with the bare-`playbin3`
    reproducer numbers from the issue body, and the main-thread `Null` wait. Then
    close #1239 by hand. A promotion does not close issues here.
