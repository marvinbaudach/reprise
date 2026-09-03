---
slug: the-player-bar-greets-you-with-a-random-track
worktree: /home/marvin/Projects/reprise-the-player-bar-greets-you-with-a-random-track
branch: feature/the-player-bar-greets-you-with-a-random-track
phase: coded
codex_session:
created: 2026-09-03
---
# The player bar greets you with a random track

## Symptom

Starting the Reprise desktop app always shows the same track in the player
bar ("A Day to Remember").

## Cause — measured, not guessed

There is no hard-coded default track. The bar shows the **restored session
position**:

`window_bootstrap.rs:50` → `session_restore::load` → `restore_runtime`
(`session_restore.rs:60`) → `PlayerController::restore_session_queue`
(`ui/playback/session_player.rs:45`), which takes
`current_up_next.or_else(|| queue.current())` and pushes it into the bar via
`sync_track` / `sync_cover` / `sync_position`, state `Stopped`.

Read from the live DB (`~/.local/share/reprise/reprise.db`, key
`ui.session.v1`, app not running):

```
queue.ids     = 1968 entries (whole library), play_origin = library "Music"
queue.order   = shuffled permutation
queue.position= 4  ->  order[4] = 5 -> ids[5] = 1842
track 1842    = A Day to Remember - "Welcome to the Family"
```

So restore is faithful; the persisted position simply happens to sit on that
artist. The save path (`session_restore.rs:150`, `connect_close_request`)
writes the live snapshot back on a clean close, and the user confirms a
freshly played track *is* restored.

## Decision

The user wants the startup selection to be **random every start**, taken from
the library, instead of resuming the persisted position. Confirmed 2026-09-03.

## Change

`restore_session_queue` validates and stores the persisted queue unchanged,
then arms `pending_random_start` with the ordered result of
`query_random_live_track_ids`. Its first ID is the startup greeting shown in
the player bar while playback remains `Stopped`. An explicit pending
`current_up_next` item still wins, and an empty library clears the bar and
disables transport. Query failure falls back to the restored current item.

The shared `stopped_play_target()` decision keeps the bar, Play/Pause, MPRIS
Play, and queue transport on one lifecycle. `has_playable_item()` separately
answers whether transport is reachable; it is not used as proof that a
concrete stopped target exists. A small non-consuming projection supplies the
same selected item to marker and seek-start paths without cloning the complete
random ID vector.

The first Play consumes `pending_random_start` through `play_from_view`,
installing that exact library-random snapshot only after explicit user intent.
Next and Previous discard the greeting and continue from the untouched
restored queue. Restoring an episode also discards the greeting before its
paused metadata is presented. A launch that never plays therefore preserves
the queue body, order, position, repeat mode, shuffled flag, Play-Next list,
and playback origin exactly as persisted.

The track-list notification uses the same stopped-target projection. While a
greeting is armed, its track—not the hidden restored current item—owns the
shared marker and receives START-4's startup selection and centring treatment.
When the greeting is dismissed, transport sensitivity is resynchronised and
the marker returns to the restored queue's concrete current item; if either ID
is absent from the restored destination, the destination's own selection and
viewport remain untouched.

### Testability

Keep the pick injectable so a test can assert determinism instead of
depending on `ORDER BY RANDOM()` — e.g. a chooser closure on the controller,
defaulted to the real query. `fastrand` is already a `reprise-core`
dependency (used by shuffle) if an in-process pick is preferred over SQL.

## Tests to update deliberately

Any test asserting "a restart shows the persisted current track" is now
intentionally obsolete. Sweep at least:

- `crates/reprise-core/src/queue/snapshot.rs` (round-trip tests stay valid —
  they test `Queue`, not startup)
- the session smoke harness env hooks `REPRISE_SMOKE_SESSION_SEED`,
  `REPRISE_SMOKE_SESSION_REPORT`, `REPRISE_SMOKE_SESSION_PLAY`
  (`ui/session_restore.rs:19`)
- acceptance/ and quality/ suites mentioning session restore

New coverage:

- startup with a non-empty library shows a track and stays `Stopped`
- startup twice with a seeded chooser yields the two seeded tracks
- startup with an empty library clears the bar and disables transport
- a pending `current_up_next` still wins over the random pick
- launch-and-close preserves the complete curated queue and playback origin
- the first Play starts exactly the track displayed by the greeting
- chooser failure falls back to the restored queue without arming a greeting

## Open flag

The user reports "always the same track" *and* that a freshly played track is
restored correctly. Both cannot hold at once. Most likely they simply play
that artist often, or quit without playing. Randomising the start will mask
the difference permanently — if session saves ever fail to land, this change
hides it.
