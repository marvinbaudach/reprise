# MCP playback control — design

**Date:** 2026-07-23
**Status:** approved-pending-review

## Goal

Make the running Reprise app fully drivable by an agent through `reprise-mcp`,
so the user never has to touch the GUI to control playback. Add to the MCP:

1. **Transport** — start (play), pause, stop, next, previous.
2. **Targeted play** — play a specific track or a specific playlist by id.

"Without GUI" means *without operating the GUI*: the GTK app still runs (it may
be backgrounded) and remains the audio host — GStreamer playback and the MPRIS
server live only in that process. The MCP is a **controller, not a player**; it
produces no audio itself.

## Non-goals (explicitly deferred)

- **Headless playback engine** (audio without the GTK app). That would mean
  lifting the playback engine out of `reprise-gnome` into a headless worker — a
  separate, much larger project. This design is a prerequisite/phase-1 of that
  but does not build it.
- Volume, seek, shuffle/loop control from the MCP (MPRIS already supports these
  on the app; can be added later with the same pattern — YAGNI for now).

## Architecture

```
agent ──stdio──> reprise-mcp ──D-Bus session bus──> running Reprise app
                    │                                   (GStreamer + MPRIS)
                    ├─ transport  → org.mpris.MediaPlayer2.Player.{Play,Pause,Stop,Next,Previous}
                    └─ play by ids → org.reprise.Player1.PlayTrackIds(ax)
```

- **Transport** uses the **standard MPRIS** methods the app *already* serves —
  no app change needed. Mirrors `reprise-cli`'s `playback` command (Beschluss 3:
  a thin `zbus` session-bus client is the one sanctioned exception to
  "surfaces depend on reprise-core only").
- **Targeted play** needs a primitive MPRIS does not have (play an explicit list
  of library tracks). We add **one** small, Reprise-specific D-Bus method rather
  than shoehorning it into MPRIS. The MCP resolves "play playlist X" to track
  ids itself (it already reads the library DB), so the app stays dumb: it
  receives only a list of ids and plays them.

### Approach chosen (A) vs alternatives

- **(A, chosen)** Custom `org.reprise.Player1` interface with a single
  `PlayTrackIds(ax)` method on the existing MPRIS object. Minimal app surface
  (one primitive), MCP does playlist→ids resolution. Clean separation.
- (B) MPRIS `Playlists` (`ActivatePlaylist`) + `OpenUri` — two mechanisms, more
  app-side mapping (MPRIS playlist ids ↔ Reprise playlists), and `OpenUri`'s
  replace/enqueue semantics are underspecified. Rejected.
- (C) DB-mediated "play intent" via the change_log outbox. Rejected — breaks the
  established "MPRIS is the playback IPC, the DB is library sync" separation and
  is timing-fragile.

## Changes by crate

### `reprise-core` (`media_integration.rs`)
- Add `MprisCommand::PlayTrackIds(Vec<i64>)` to the existing command enum. This
  is the only core change; the enum is already the shared vocabulary between the
  platform MPRIS server (producer) and the GTK app (consumer).

### `reprise-platform-linux` (`mpris/mod.rs`)
- Add a third interface, `org.reprise.Player1`, on the existing
  `/org/mpris/MediaPlayer2` object (same bus name
  `org.mpris.MediaPlayer2.reprise`), with one method:
  `PlayTrackIds(ids: Vec<i64>)` → `dispatch(MprisCommand::PlayTrackIds(ids))`,
  exactly like the existing Player methods dispatch their commands.
- Ignore an empty id list (no-op) rather than clearing playback.

### `reprise-gnome` (`ui/mpris_mirror.rs`)
- Handle `MprisCommand::PlayTrackIds(ids)` →
  `player.play_from_view(ids, 0, PlayOrigin::library())`. `play_from_view` is the
  existing "seed the queue from a list of ids and start playing" primitive
  (empty seed already resets to stopped, so a bad list is safe). A dedicated
  "agent/external" `PlayOrigin` is optional polish, not required.

### `reprise-mcp`
- **`playback.rs`** (new): a `zbus` session-bus client mirroring the CLI's
  `commands/playback.rs`. Two operations:
  - `transport(action)` → `org.mpris.MediaPlayer2.Player.{Play|Pause|Stop|Next|Previous}`.
  - `play_track_ids(ids)` → `org.reprise.Player1.PlayTrackIds`.
  - Absent player (`ServiceUnknown`/`NameHasNoOwner`) → a clear "no running
    Reprise app on the session bus — is the app running?" error, per the CLI.
  Run inside `spawn_blocking` with `zbus::blocking` (the MCP already offloads
  DB work the same way; the CLI's blocking client is the proven reference).
- **Two tools** (`server.rs`):
  - `music_playback_control` — `{ action: "play"|"pause"|"stop"|"next"|"previous" }`
    → `transport(action)`.
  - `music_play` — exactly one of `{ track_ids: [i64] }` **or**
    `{ playlist_id: i64 }`. A `playlist_id` is resolved to its ordered track ids
    via a core facade (add `playlists::track_ids(conn, id)` if missing);
    `track_ids` is used directly. Then `play_track_ids(ids)`.
- **`capability.rs`** — add `CAP_PLAYBACK_CONTROL =
  "agent.capability.playback:control"`, **default `true`** (like
  `library:read`), read **live** on each call (revocation takes effect
  immediately; no startup-snapshot, since it is not a fail-closed write). Both
  tools are gated by it; a revoked cap yields the standard capability-denied
  tool error.
- **`data.rs`** — the playlist→ids resolution and any read validation; the D-Bus
  side stays in `playback.rs`.
- `zbus` becomes a direct dependency of `reprise-mcp` (Linux desktop tool; not
  part of the core cross-target CI check, which only covers `reprise-core`).

## Data flow (targeted play a playlist)

1. Agent calls `music_play { playlist_id: 7 }`.
2. MCP checks `playback:control` (live). If denied → capability error.
3. MCP resolves playlist 7 → `[101, 102, 103]` via the core facade (DB read).
4. MCP calls `org.reprise.Player1.PlayTrackIds([101,102,103])` over the session
   bus.
5. App dispatches `MprisCommand::PlayTrackIds` → `play_from_view([...], 0,
   library())` → queue seeded, playback starts.
6. Tool returns a short confirmation (e.g. "Playing 3 track(s)").

## Error handling

- **App not running** → "no player" error (both tools), actionable message.
- **`music_play` with neither/both of `track_ids`/`playlist_id`** → invalid-input
  tool error.
- **Unknown `playlist_id` / empty resolved list** → invalid-input tool error
  ("playlist has no playable tracks"), no D-Bus call.
- **Capability denied** → standard capability-denied tool error.
- **Missing/absent tracks in the id list** → passed through; the app's
  `play_from_view`/player already handle unplayable rows.

## Testing

- **core:** unit test that `MprisCommand::PlayTrackIds` round-trips (the enum is
  otherwise exercised by existing tests).
- **platform-linux:** a test that the `org.reprise.Player1.PlayTrackIds` method
  dispatches the right command (following the existing MPRIS interface tests'
  pattern — they assert dispatch, not a live bus).
- **reprise-mcp:** JSON-RPC fixtures for `music_playback_control` and
  `music_play` (arg validation, capability-denied path, playlist→ids resolution
  against a throwaway DB). The actual D-Bus round-trip is not unit-tested (needs
  a live player) — same boundary the CLI draws; a manual/integration check drives
  a real running app.
- **Non-regression:** existing MPRIS/transport behavior and the app's
  `play_from_view` are untouched in semantics.

## Rollout note

Playback control reaches the live library and starts audio; `playback:control`
is default-on but revocable live by setting
`agent.capability.playback:control = 0`. Takes effect (like the other MCP
capabilities) for the next MCP server start.
