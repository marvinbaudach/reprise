---
slug: the-pause-survives-a-skip
worktree:
branch:
phase: reviewed
codex_session:
created: 2026-09-02
strands: a,b
merge_order: a,b
---
# The pause survives a skip

A paused player stays paused when the track changes by hand — on the phone and
on the desktop. Today every manual change starts playback.

Reported by the user while swiping a paused card, reproduced on hardware
2026-09-02: `state=PAUSED` → swipe → `state=PLAYING`, new track audible.

## Decisions taken in the grill

- **One rule for all three doors.** Swipe, the transport buttons, and the
  external MediaSession path (Bluetooth/AVRCP, headset, notification, Android
  Auto, car head unit) behave identically. No special case at the point where
  they already converge.
- **The card stays complete.** After a paused skip the sheet shows cover, title,
  total duration and waveform, position `0:00` — a paused track, not a broken one.
- **Nothing is loaded into the backend while paused.** No trait change, no
  play-then-pause race.
- **Both platforms, two strands.** The desktop has the same defect in its own
  files.

## Why the chosen shape is small

Verified by reading the code, not assumed:

- `toggle_pause` (`crates/reprise-android-ffi/src/playback_session.rs:517-528`)
  **already** restores an unloaded paused track:
  `state == Paused && !current_loaded && queue.current().is_some()` → set Playing
  → `start_current()`. That path exists and is used.
- `adopt_current` already sets `current_loaded = false` (line 193).

So a paused skip that simply does not call `start_current` leaves the session in
exactly the state `toggle_pause` knows how to resume from. Nothing new has to be
invented for "press play afterwards".

The waveform needs no player either: it is precomputed per track and read from
the database (`crates/reprise-android-ffi/src/track_analysis.rs:61-111` via
`trackRenderBars`, drawn by `SpectralSeekTrack.kt:72`).

The single genuine gap is duration. `stream_events.rs:57-64` fills
`snapshot.duration_ms` only from `PlayerEvent::Position`, and the position ticker
runs only while `player.isPlaying` (`Media3PlaybackPort.kt:78-94`, line 80).
`playback_session.rs:192` resets it to 0 on every track change. The library
already knows the value — `Track.duration_ms` (`crates/reprise-core/src/models.rs:64`),
exposed as `TrackRow.duration_ms` (`crates/reprise-android-ffi/src/browse.rs:100`),
which is what the Titles list draws.

## The defect, precisely

`next()` → `move_playhead()` does two independent wrong things
(`crates/reprise-android-ffi/src/playback_session.rs`):

1. `adopt_current_for_play_intent()` (676) → `adopt_current()` (185-197)
   **hard-codes `snapshot.state = Playing`** at line 194 and resets position and
   duration to 0. `notify()` publishes that before the backend is touched.
2. `start_current()` (682, body 349-388) calls `backend.play_uri()` at line 370.

Fixing one half alone leaves a visible defect: a state that lies, or a backend
that starts anyway.

The end-of-track path is correct only by accident of routing:
`stream_events.rs:74/86/126` calls `adopt_current()` after the backend has
already advanced, so no second `play_uri()` is issued. It *also* stamps
`Playing` — right there, wrong everywhere else. `adopt_current` therefore cannot
be "corrected" in place without looking at every caller: `set_tracks` (182),
`previous_in_queue_order` (570), `move_playhead` (676), and the three
stream-event sites.

## Strands

Disjoint file ownership, no shared code between them.

- **A — Android** (`docs/plans/the-pause-survives-a-skip-a.md`):
  `crates/reprise-android-ffi/**`.
- **B — Desktop** (`docs/plans/the-pause-survives-a-skip-b.md`):
  `crates/reprise-gnome/src/ui/playback/**`.

`merge_order: a,b` — A is the reference implementation and settles how the intent
is threaded; B follows the same idea in a different architecture. There is no
code dependency, so they can be built concurrently; the order matters only for
review consistency.

## Post-merge cross-checks

Neither strand can make these on its own, so they are not tasks in either file:

1. Both platforms answer a manual skip while paused the same way. Compare the
   Android arms against the desktop behaviour by hand.
2. Neither platform changed the end-of-track behaviour: a track that runs out
   while playing still continues into the next one.
3. No platform gained a state flicker — the notification/MPRIS never shows
   Playing on the way through a paused skip.

## Verification, shared by both strands

**A control arm is mandatory.** Every arm must be shown to fail before the fix,
on the same device and the same build pair, both identified by **APK md5** — never
by version name. A previous session in this area had to retract a positive result
because the recording came from a foreign build.

Arms, each "change track while paused → still paused":

- **A1** in-app swipe
- **A2** the transport next/previous buttons
- **A3** `adb shell cmd media_session dispatch next` — note that
  `adb shell media dispatch` does **not** exist on this device
- **A4** the regression risk, all three doors: playing → change track → **still
  playing**
- **A5** end-of-track auto-advance still continues playing
- **A6** press play after a paused skip: the track starts, and duration and
  position are right

Read state with `adb shell dumpsys media_session | grep -oE 'state=[A-Z]+'`, and
verify the track actually changed before and after every arm. A session with an
empty queue (`queueTitle=null, size=0`) makes `next` a silent no-op while the tap
lands — that cost a full round of measurement on 2026-09-02.

Take the device lock for the whole measurement, and let a freshly installed build
finish its library scan before touching it; killing it mid-scan produces ANRs that
look like a regression and are not.
