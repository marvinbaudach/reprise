---
slug: an-agent-started-song-inherits-the-library-queue
worktree: /home/marvin/Projects/reprise-an-agent-started-song-inherits-the-library-queue
branch: feature/an-agent-started-song-inherits-the-library-queue
phase: refactored
codex_session:
created: 2026-08-29
---
# An agent-started song inherits the library queue

An agent starting a single song over MCP leaves the listener with a dead end:
the song plays, and when it ends there is nothing after it. Starting the same
song by double-clicking it in the library leaves the whole library queued behind
it. The two should agree. One change, in the GNOME app's MPRIS command handler.

## What is actually true today

The queue is not empty — it holds exactly one track.

`music_play` resolves its parameters to a bare `Vec<i64>`
(`crates/reprise-mcp/src/server.rs:501-523`) and hands it to
`playback::play_track_ids`, which calls D-Bus `PlayTrackIds`
(`crates/reprise-mcp/src/playback.rs:227-233`). `RepriseControl::play_track_ids`
dispatches `MprisCommand::PlayTrackIds`
(`crates/reprise-platform-linux/src/mpris/control.rs:40-44`), and the GNOME side
handles it in one expression:

```rust
MprisCommand::PlayTrackIds(ids) => self.play_from_view(
    ids,
    0,
    crate::ui::playback::play_origin::PlayOrigin::library(),
),
```
(`crates/reprise-gnome/src/ui/mpris_mirror.rs:493`)

`play_from_view` does `queue.set_tracks(ids, start_index)`
(`crates/reprise-gnome/src/ui/playback/queue_transport.rs:427`,
`crates/reprise-core/src/queue.rs:73`). One id in means a one-track queue: Now
Playing and nothing behind it, which is what the Queue view renders as empty.

Row activation takes a different route to the same primitive. `wire_activate`
calls `queue_ids_for_activation`
(`crates/reprise-gnome/src/ui/track_list/track_list_activation.rs:160`), which
runs `queries::query_track_ids_browsed` over the row's *current*
source/sort/filter/browse view and returns `(all_ids, clicked_position)`. The
clicked track lands at its real index in a full-view queue, so Next walks
forward through the library and Previous walks back into it.

There is no CLI path to fix separately. `reprise playback`
(`crates/reprise-cli/src/commands/playback.rs:22-27`) only knows PlayPause,
Next, Previous and Status — it cannot start a named track at all. The GNOME app
takes `HANDLES_OPEN`, not `HANDLES_COMMAND_LINE`
(`crates/reprise-gnome/src/main.rs:119`), so a path on the command line is a
file-manager open and lands in `file_open.rs:221`. Anything that starts a
*specific* song from outside the app — MCP today, a future `reprise play` — goes
through `PlayTrackIds`, so that handler is the whole seam.

## The change

Only `MprisCommand::PlayTrackIds` changes. `play_from_view` stays exactly as it
is: its other three callers — row activation, `file_open.rs:221`, and
`toggle_pause`'s `StartRandom` snapshot — all pass a list that is already the
intended context, and must keep exact-list semantics.

### The rule

- **Exactly one id** → seed the flat library and start at that track's index,
  the way a double-click in the big library does.
- **More than one id** → unchanged. An explicit multi-track list *is* the
  context, and this is also how `music_play`'s `playlist_id` arrives.

Extract the decision as a pure function next to the handler — requested ids plus
the library id list in, either `(library_ids, index)` or `(requested_ids, 0)`
out — and give it rule-named unit tests, matching the shape
`missing_activation_notice` uses in `track_list_activation.rs`.

### What "the flat library" means here

Not the currently visible view. The agent-played track need not appear in it
(the user may be on a playlist, an artist page, or a filtered search), and an
index into a list that does not contain the track is meaningless. Seed
`ViewSource::Library` with an empty filter and a default `BrowseFilter` —
the big library, sorted the way the user last sorted it:

- **Sort**: read `sort_field` / `sort_dir` from `reprise_core::library::session::load(&db)`
  (`crates/reprise-core/src/library/session.rs:74-75`, persisted across
  restarts). The controller already holds `self.conn`, so no new provider
  closure across the TrackList↔player seam is needed. Falling back to
  `SortState::default()` (`artist` / `asc`) when the session has no sort is
  fine — that is the same default the list itself starts from.
- **AI exclusion**: use `queries::query_track_ids_browsed_ai` with the real
  flag, read via `library::settings::get_bool(&conn, "filter.exclude_ai", false)`
  (as `browse_bar.rs:140` does). `query_track_ids_browsed` hardcodes `false`,
  and only the `Library` source honours the flag — seeding with it hardcoded
  would queue tracks the library view deliberately hides.

### When the track is not in that list

`ids.iter().position(|&id| id == requested)` returning `None` means the track is
missing, AI-excluded, or past `QUEUE_LIMIT`. Fall back to today's behaviour —
`play_from_view(vec![id], 0, library())` — so the requested song still plays.
Never seed a list and guess an index; that plays the wrong track. Keep the
`is_queue_capped` warning the activation path logs, and log the query error and
fall back the same way if the query fails.

`PlayOrigin::library()` stays correct for both branches, and
`notify_queue_changed()` already fires inside `play_from_view`, so the Queue
view and the sidebar counter need no extra work.

### Known limitation, to be named in the doc comment

`resolve_play_ids` flattens `track_ids` and `playlist_id` into one `Vec<i64>`
before the D-Bus call, so the handler cannot tell "play this track" from "play
this one-track playlist" — the latter gets expanded too. Carrying that intent
across the D-Bus signature is a wider change than this asks for; state the
limitation in the handler's doc comment rather than papering over it.

## Tests

Existing tests to keep green — all three assert the wire, not the queue, so
none of them should need changing; confirm rather than assume:

- `crates/reprise-mcp/tests/playback_roundtrip.rs:537` — explicit id list
  reaches `PlayTrackIds` verbatim and in order.
- `crates/reprise-mcp/tests/playback_roundtrip.rs:565` — a playlist resolves to
  ordered ids on the wire.
- `crates/reprise-platform-linux/src/mpris/control.rs:118` — an empty list is a
  no-op; a non-empty one dispatches verbatim.

New:

- Rule-named unit tests on the pure decision function: one id present in the
  library expands with the right index; one id absent falls back to the
  single-track list; two ids stay verbatim; an empty list stays empty.
- A GNOME-side test over the handler against a real seeded DB: `PlayTrackIds`
  with one id leaves a queue longer than one with that track current, and the
  queue order matches the session sort.
