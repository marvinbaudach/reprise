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

In `restore_session_queue` (`ui/playback/session_player.rs`), after the
snapshot has been validated and stored, replace the *displayed current item*
without changing the restored queue:

1. If `current_up_next.is_some()` — an explicit Play-Next item the user left
   pending — restore that unchanged. Explicit intent beats randomness.
2. Otherwise pick a random start:
   - `reprise_core::queries::query_random_live_track_ids(conn)` already
     returns every live track id in random order (`ORDER BY RANDOM()`), and is
     the same call the existing `ToggleAction::StartRandom` path uses
     (`ui/playback/queue_transport.rs:337`).
   - Empty result → `library_has_tracks.set(false)`,
     `sync_transport_enabled(false)`, `sync_clear_track()` — same guard as
     `StartRandom`.
   - Non-empty → arm the complete ordered id list as a pending startup
     greeting and show `ids[0]` in the bar, **stopped**. The restored queue,
     order, position, repeat, shuffled flag, and playback origin remain
     untouched.
   - Query failure → arm no greeting and show the restored queue's current
     track as before.
3. Clear `restored_placement_intact` only while a greeting is armed. The
   random track is *not* where startup routing placed the view, so START-3's
   "first Play must not centre a second time" shortcut must not apply. Every
   ordinary restored item keeps the shortcut.
4. Play/Pause from `Stopped` consumes an armed greeting through
   `play_from_view(ids, 0, PlayOrigin::library())`; this explicit user action
   is the first point where the queue may be replaced. Next and Previous
   discard the greeting and resume the restored queue. Every other explicit
   playback path clears the greeting so it cannot be consumed later.

Playback state stays `Stopped`; nothing starts playing on launch.

### Why the queue is re-seeded only on Play

`Queue::current()` is `ids[order[pos]]` — a position can only ever name a
track that is already in `ids`. Showing a library-wide random track while
leaving the restored queue in place therefore needs a pending greeting: Play
consumes the exact displayed snapshot, while Next and Previous explicitly
dismiss it before following the restored queue. Re-seeding during restore
would make a launch-and-close permanently overwrite a curated persisted
queue. Re-seeding on Play keeps bar, transport, and MPRIS consistent without
treating startup as user intent.

Repeat mode, the shuffled flag, the `up_next` list, and playback origin survive
startup unchanged. An explicit Play starts a new library context, matching the
existing `StartRandom` path.

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
