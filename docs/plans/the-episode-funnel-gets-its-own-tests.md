---
slug: the-episode-funnel-gets-its-own-tests
worktree: /home/marvin/Projects/reprise-the-episode-funnel-gets-its-own-tests
branch: feature/the-episode-funnel-gets-its-own-tests
phase: planned
codex_session:
created: 2026-09-01
---
# The episode funnel gets its own tests

## Why

Follow-up to #759 ("The waveform click sets the start point"), already merged
into `dev`. Its review left one accepted finding unapplied because the PR landed
before it was addressed. **This plan adds tests only. No behaviour changes.**

A waveform click stores an item-bound start mark
(`pending_start_mark: Cell<Option<(QueueItem, i64)>>`) instead of starting
playback. `take_pending_start_mark` (`seek_start.rs`) is called from two funnels:

- `present_track` (`player_controller.rs`) — local tracks
- `prepare_external_playback` (`external_media.rs`) — episodes / external media

The first review of #759 found a HIGH bug in exactly this area: the mark was
consumed on only one start path, and the plan's stated reason for believing
otherwise was false. The fix moved consumption to the two funnels above.

The two bypass regressions that pin that fix
(`stopped_track_mark_applies_when_the_same_track_starts_directly`,
`stopped_track_mark_cannot_survive_a_different_direct_start`) both exercise the
**local-track** funnel only, via `seek_or_start` → `play_track_id` →
`present_track`.

The episode funnel has no coverage outside `start_current_item` — the one path
that already worked correctly before the fix. The pre-existing
`stopped_queued_episode_marks_the_clicked_position_until_play` drives playback
through `controller.toggle_pause()`, i.e. exactly that already-working path.

This matters because `external_media.rs`'s `resume_ms` wiring across two call
sites is where most of #759's code actually lives, and because the failure class
it guards — a wrong belief about which paths reach the consumption site — has
already bitten once in this change.

## Tasks

All in `crates/reprise-gnome/src/ui/playback/seek_start_tests.rs`.

**1 — The episode funnel applies the mark on a non-`start_current_item` route.**
Arm a mark on an episode, then start that episode through `play_up_next_at` (on
an `Episode`) or `play_podcast_row_with_context`. Assert the start happens at the
marked position.

**2 — The episode funnel drops a mismatched mark.** Arm a mark on one episode,
start a *different* item through such a route, and assert the mark is dropped —
including that it does not later fire when the marked episode becomes current
again.

**3 — `play_up_next_at` gets coverage at all.** It is currently untested for
either item type. Task 1 may satisfy this; if it does, say so rather than adding
a redundant test.

**4 — An advance that lands back on the same item.** The plan text of #759
asserts a mark must not survive an `advance_playback` that returns to the same
item (repeat-one, or a queue whose next entry is the same track). Only the
`reset_to_stopped` + `toggle_pause` variant of that invariant is pinned today.

## The shape these tests must have

Both halves per test, as the existing ones do: after the click, **zero**
`play()` / `play_uri()` calls and an empty `sought_positions`; then the start,
and only then the play **and** the seek. Half one alone passes vacuously if the
setup stops reaching the `Stopped`-with-current-item state.

**Each new test must be observed red before it is trusted.** That is how the two
existing bypass regressions were validated. A test that passes immediately
against current code is not automatically wrong, but it is not evidence either —
say so explicitly instead of keeping it quietly.

**Do not weaken or delete an existing assertion.** If a test can only be made to
pass by changing behaviour, that is a finding to report, not a task to carry out:
this code is merged and was reviewed twice.

## Verification

The `seek_start` display tests, run the way this repo's gate runs them — each in
its own process with `--exact --ignored` under a display server, as
`scripts/check-display-tests.sh` does. Running the group in one shared process
aborts for environment reasons that say nothing about the code. Every test in the
group, old and new, must pass.

## Parallelität

**No cut. One strand.** One test file, one concern.
