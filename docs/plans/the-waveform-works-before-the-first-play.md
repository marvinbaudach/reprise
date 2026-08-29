---
slug: the-waveform-works-before-the-first-play
worktree: /home/marvin/Projects/reprise-the-waveform-works-before-the-first-play
branch: feature/the-waveform-works-before-the-first-play
phase: planned
codex_session:
created: 2026-08-29
---
# The waveform works before the first play

Reprise starts, the last track sits in the bar with its title and its cover —
and the seek bar is dead. Clicking it does nothing, dragging it does nothing; it
only wakes up once the track is actually playing. This plan makes a click on the
waveform in that restored state do the obvious thing: start the item at the
clicked position.

## What is actually true today

The symptom is one user-visible thing sitting on top of three independent gaps.
Fixing any one of them alone leaves the bar just as dead — a sensitive widget
that computes a target of 0 ms, or a correct target the runtime refuses.

- **The widget is switched off while stopped.** `refresh_sensitivity`
  (`crates/reprise-gnome/src/ui/player_bar/player_bar.rs:744-759`) ends with
  `self.waveform.widget().set_sensitive(state != PlaybackState::Stopped && self.seek_enabled.get())`.
  Session restore ends on `Stopped` (`sync_state(PlaybackState::Stopped)`,
  `crates/reprise-gnome/src/ui/playback/session_player.rs:114`), so after every
  cold start the waveform is insensitive. GTK skips insensitive widgets when
  picking, which is exactly the reported "the handle does not move at all" — the
  click never reaches the `GestureDrag`
  (`crates/reprise-gnome/src/ui/player_bar/waveform_seek.rs:278`).
- **The bar does not know how long the track is.** `restore_session_queue`
  restores the metadata and calls `sync_track`, `sync_cover` and
  `sync_lyrics_track` (`session_player.rs:141-152`) but never `sync_position`.
  `PlayerBar::duration_ms` (`player_bar.rs:117`) therefore stays 0, and
  `connect_seek` (`player_bar.rs:585-595`) turns every 0..1 fraction into 0 ms.
  `set_state` even zeroes it actively: `if state == PlaybackState::Stopped {
  self.set_position(0, 0); }` (`player_bar.rs:357-359`). The compact card shows
  the same hole from the other side — its waveform is unconditionally sensitive
  (`compact_player.rs:230`), so the click *does* arrive, and its `connect_seek`
  drops it because `current_duration_ms` is 0 (`compact_player.rs:335-341`).
- **The runtime has nothing loaded to seek in.** Restore rebuilds queue and
  metadata, not a loaded track: `Transport::seek` returns
  `Rejected::NothingToPlay` while `self.current` is `None`
  (`crates/reprise-runtime/src/transport_controls.rs:238-245`), and the bar's
  seek closure routes through `controller.seek` → `player.seek_to`
  (`player_controller_wiring.rs:104-114`, `mpris_mirror.rs:353-364`), which logs
  `seek failed` and returns.

So "only after the song has started playing" is precise: playback is the event
that makes all three true at once — it loads the track in the runtime, ticks a
duration into the bar, and moves the state out of `Stopped`.

**The restored podcast episode has the same dead click at a different layer.**
`restore_session_episode` ends on `PlaybackState::Paused`
(`external_media_session.rs:159`) and seeds a duration, so the widget is already
sensitive and the target is already correct — but nothing is loaded in the
pipeline either, so the seek fails the same way. Its start path is
`resume_restored_episode` (`external_media_session.rs:171`), which
`toggle_external_pause` calls first (`external_media.rs:470-473`).

### What already exists and must be reused

- **Starting the restored track** has one canonical path: `toggle_pause` →
  `toggle_action(...)` → `ToggleAction::StartCurrent(change)` →
  `present_queue_item(item, StartPlayback::Yes, change)`
  (`queue_transport.rs:272-324`), including the playability check, the
  `advance_playback` fallback and the `restored_placement_intact` centring rule.
- **Start-then-seek** is solved for external media: the podcast path seeks
  immediately after the start (`external_media.rs:344-347`) and retries on a
  later position tick when that first attempt was too early
  (`external_media_position.rs:91-93`). The retry logic is a small pure state
  machine, `ResumePolicy` (`external_media_state.rs:598-618`): `new(resume_ms)`,
  `initial_seek_finished(succeeded)`, `position_tick(duration_ms) -> Option<i64>`.
  A local track needs exactly this and nothing more — a seek issued before the
  pipeline has pre-rolled is the *same* failure that machine already handles.
  The ticker runs every 500 ms and only while the pipeline is `Playing`
  (`crates/reprise-platform-linux/src/player.rs:29,222`), which is the worst-case
  delay of the retry.

## The change

Five tasks. 1–4 are the fix, 5 is the proof.

### Task 1 — the bar learns the length at restore

In `restore_session_queue`'s `Some((id, summary))` branch
(`session_player.rs:128-153`), after `sync_cover` (line 147), add
`self.sync_position(0, summary.duration_ms);`.

The position inside the function is load-bearing, not cosmetic:
`sync_state(PlaybackState::Stopped)` runs at line 114, i.e. **before** this
branch, and `PlayerBar::set_state` zeroes the duration on `Stopped`
(`player_bar.rs:357-359`). A `sync_position` placed before line 114 would be
wiped one line later. `sync_position` feeds both surfaces
(`now_playing_wiring.rs:435-439`), so the compact card gets the length for free.

### Task 2 — the waveform stays live while something is loaded

Extract the rule out of `refresh_sensitivity` into
`crates/reprise-gnome/src/ui/player_bar/player_bar_state.rs`, next to the
existing `bar_should_be_sensitive`:

```rust
pub(in crate::ui) fn waveform_should_be_sensitive(
    state: PlaybackState,
    seek_enabled: bool,
    has_loaded_length: bool,
) -> bool {
    seek_enabled && (state != PlaybackState::Stopped || has_loaded_length)
}
```

`refresh_sensitivity` calls it with `self.duration_ms.get() > 0` as
`has_loaded_length`. Live radio keeps `seek_enabled == false`
(`player_bar_external.rs:64`) and therefore stays insensitive — that guard does
not move.

`duration_ms` is the deliberate signal: it is the same value `connect_seek` needs
to compute a target, so the widget is sensitive exactly when a click can produce
a meaningful millisecond, and it self-clears on a real stop. (Checked against the
live library: 0 of 1999 tracks carry a missing or zero duration, so this is not a
practical false-negative.)

That self-clearing exposes an ordering bug the extraction must fix: `set_state`
calls `refresh_sensitivity()` *before* `set_position(0, 0)`
(`player_bar.rs:357-359`), so a real stop would leave the waveform sensitive
against a duration that is about to become 0. Make `set_position` call
`refresh_sensitivity()` itself, but only when the stored `duration_ms` actually
crossed between zero and non-zero — a position tick arrives twice a second and
must not become an unconditional sensitivity sweep.

### Task 3 — a seek while stopped starts the local track at that position

1. **Extract the start.** Pull `toggle_pause`'s `ToggleAction::StartCurrent`
   branch (`queue_transport.rs:292-324`) into a method `start_current_item(…)`
   and call it from both places, so the playability check, the `advance_playback`
   fallback and the centring argument stay identical for the two entry points.
   No behaviour change for `toggle_pause`.
2. **A new entry point, not a change to `seek`.** Add
   `PlayerController::seek_or_start(position_ms)` and wire the two bar closures
   to it (`player_controller_wiring.rs:104` for the full bar, `:242` for the
   compact card). `PlayerController::seek` (`mpris_mirror.rs:353`) stays exactly
   as it is: MPRIS keeps its current semantics, and a remote `Seek` while
   stopped stays the no-op it is today. Only a click on a Reprise seek bar
   starts playback.
3. **`seek_or_start` behaviour.** Its branch order is exactly this, and the
   first clause matters: a restored episode sits on `Paused`, not `Stopped`, so
   gating on `Stopped` alone would silently skip task 4.

   1. a restored, not-yet-started external session is waiting
      (`restored_resume_request(&external).is_some()`) → task 4's path;
   2. else `state == PlaybackState::Stopped` with a current queue item →
      `start_current_item` plus the pending seek below;
   3. else → delegate to `seek` unchanged.

   In branches 1 and 2: start the item, then attempt
   `player.seek_to(position_ms)` immediately and hand the outcome to a
   `ResumePolicy` (`ResumePolicy::new(position_ms)` + `initial_seek_finished(ok)`)
   held on the controller, e.g. `pending_local_seek: RefCell<Option<ResumePolicy>>`.
   On the first position tick with a known duration, `position_tick(duration_ms)`
   yields the target once; apply it with `player.seek_to` and only then run the
   existing post-seek work (`update_mpris_position`, `notify_mpris_seek`,
   `lyrics.external_seek`). Clear the pending policy on any track change, stop or
   external-media start, so a stale target can never land on the next track. The
   tick seam is the `PlayerEvent::Position` handler
   (`player_event_handling.rs:180` → `now_playing_wiring.rs:435`), the same place
   the external path drains its retry (`external_media_position.rs:91-93`).

Reuse `ResumePolicy`; do not write a second copy of that state machine. If its
module placement makes reuse awkward, move the type into its own small module
and re-export it.

### Task 4 — the same click on a restored episode

When a restored-but-unstarted external session is present — detect it the way
`resume_restored_episode` already does, via `restored_resume_request(&external)`
(`external_media_session.rs:171-177`) — `seek_or_start` starts it through
`resume_restored_episode()` instead of `start_current_item`.

The clicked position must **replace** the stored resume position, not compete
with it: `start_podcast_source` seeds the session with
`ResumePolicy::new(resume_ms)` and seeks to `resume_ms` (`external_media.rs:290,
344-347`). After the start, overwrite that session's resume target with the
clicked position and issue the immediate `seek_to` for it, so exactly one target
exists and the existing retry drain (`external_media_position.rs:91-93`) carries
the clicked value rather than the stored one.

Radio is untouched: `seek_enabled == false` keeps its waveform insensitive.

### Task 5 — tests

- `player_bar_state.rs` unit tests for `waveform_should_be_sensitive`:
  `Stopped + length + seek_enabled → true`, `Stopped + no length → false`,
  `Playing + no length → true`, `seek_enabled == false → false` in every state.
- A `ResumePolicy` test for the local reuse: an initial seek reported as failed
  leaves the target pending, the first tick with `duration_ms > 0` yields it
  exactly once, the next tick yields `None`.
- **The test that would have caught this bug**, at the controller seam the
  existing controller tests already use (`crate::test_db::open()` plus the fake
  `PlaybackBackend`, as in `audio_effects.rs:60-125` and
  `external_media_artwork.rs:100-140`): with a restored current item and
  `PlaybackState::Stopped`, `controller.seek_or_start(30_000)` must start that
  item **and** leave the fake backend having been asked to seek to 30 000 ms.
  The sensitivity test alone would go green against a bar that still does
  nothing — this one must not.
- **The same test for task 4**, at the same seam: with a restored episode session
  carrying a stored `resume_ms`, `controller.seek_or_start(30_000)` must start
  that episode and leave the backend seeking to **30 000 ms, not to the stored
  `resume_ms`**. Without this assertion task 4's failure mode is silent — the
  episode starts, jumps to the stored resume position, and looks plausible.

## What this deliberately does not do

- **No pre-roll at startup.** Loading the pipeline to `PAUSED` during restore
  would make the seek work without playing, but `session_player.rs` states the
  opposite invariant in its module doc and asserts it
  (`debug_assert!(!restore_should_start_playback())`), it would change the MPRIS
  status the app reports at launch, and it would open the audio device on every
  start.
- **No restored playback position.** The session remembers the track, not where
  it was. That is a separate feature (persistence plus its own edge cases) and
  stays out of this plan; the click supplies the position here.
- **No change to the play-count heuristic.** `should_count_play` counts the
  furthest position reached (`crates/reprise-core/src/library/stats.rs:244`), so
  a click at 80 % records a play — exactly as "press play, then drag" does
  today. This plan adds an entry point, not a second counting rule.
- **No change to MPRIS semantics.** `seek` is left alone; only the bar closures
  move to `seek_or_start`.

## Verification

- `cargo test -p reprise-gnome` (scoped — no workspace build, no release build).
- Manual in the real app after a cold start: the restored track's waveform is
  clickable, one click starts it at the clicked position, and the first position
  tick does not snap it back to 0. Repeat with a restored podcast episode.
- Control arm: with an empty queue and no restored item the waveform must stay
  insensitive, and live radio must stay insensitive — the fix must not make a bar
  with nothing loaded clickable.

## Parallelität

**One strand — no cut.** The gaps are one behaviour: a strand that only made the
widget sensitive would ship a slider that moves and does nothing, and its own
verification could not go green without the other half. The file groups are
nearly disjoint (`player_bar*` + `session_player.rs` vs. `queue_transport.rs` +
`mpris_mirror.rs` + `external_media*`), but the one test that proves the reported
symptom is fixed (task 5's controller test) reads both sides. Splitting would
push that test into a post-merge cross-check — for ~150 lines of change that
costs more than the parallelism buys.
