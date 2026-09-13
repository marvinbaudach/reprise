# Deleting a playlist can permanently block a connected device

Reported 2026-09-01: "wenn ich mit angeschlossenem MTP und sync eine playlist
lösche dann renne ich in einen Fehler. ich würde erwarten, dass die Playlist
dann sofort auch in der sync-Seite entfernt wird und synchronisiert zum handy
was gelöscht wurde."

Status: root cause identified from code. Not yet reproduced on the device —
no app log was available (Reprise logs to stderr via `REPRISE_LOG`, and the
session was not started from the `.desktop` unit, so `journalctl` held nothing).

## What the user sees

The sync page blocker copy `"{count} selected playlist no longer exists"`
(`crates/reprise-view/src/device_sync.rs:55-58`, rendered by
`blocker_summary`, `:246-272`). The card keeps listing the deleted playlist,
the sync button stays blocked, and nothing is removed on the phone.

## Root cause: the selection prune is edge-triggered, and it has holes

The prune is the single point of failure. Three holes were examined; two
survive.

- **Busy skip** — the defect behind the reported error. The prune never runs
  for a busy device and never catches up. Detail below.
- **Silent replan failure** — a real second defect, but it explains only the
  stale row, not the error. `notify()` fires only `if recomputed`
  (`device_sync_runtime.rs:328`), and `recomputed` is set only when
  `recompute_delta_silent` returns `Ok`; its `Err` arm merely logs
  `could not refresh device playlists`. There is no other route to a refresh:
  no timer exists in `ui/device_sync/` outside smoke tests, and every other
  `notify()` call site reacts to an unrelated event (connect, rename, target
  change, sync progress) — none retries the failed replan. So a scan hiccup on
  a connected device prunes the selection but leaves the deleted playlist on
  screen until some unrelated event happens to notify.
- **`rememberable` gate** — RULED OUT, harmless. When `persistent_id` is
  `None`, `load_device_memory` returns `DeviceSettings::transient(...)` and
  never touches the DB (`device_sync_memory.rs:51-54`); such a selection is
  purely in-memory for the session, so the missing persist at
  `device_sync_runtime.rs:300` cannot resurrect anything. (`adopt_detected_
  device_name` can write a transient device's settings under its `mtp://` key,
  but `list_remembered_devices` filters `WHERE device_serial NOT LIKE 'mtp://%'`
  (`settings.rs:166-168`), so that row is never read back.)

### The busy skip

The only place that removes a deleted playlist from a device's selection is
`DeviceSyncRuntime::library_playlists_changed`
(`crates/reprise-gnome/src/ui/device_sync/device_sync_runtime.rs:264-331`),
reached from the sidebar's delete handler via `notify_playlists_changed`
(`sidebar_playlist_notifications.rs:31`). It prunes with

```rust
// device_sync_runtime.rs:295
sources.retain(|source| available_sources.contains(source));
```

after filtering the device list down to idle devices:

```rust
// device_sync_runtime.rs:281
.filter(|device| !device.is_busy())
```

`sources.retain(...)` against the available playlists exists **only** at that
one call site (verified by grep across the workspace). So:

1. Playlist deleted while the device is busy → the dangling `SelectionSource`
   stays in `device_settings.selection_json`.
2. Nothing re-runs the prune when the device later goes idle. The skip has no
   catch-up path — `finish_sync` and `recompute_delta_silent` replan, they do
   not re-validate the selection.
3. Every following plan is dead on arrival: `plan_mirror` pushes
   `MirrorBlocker::MissingPlaylist` and returns an **empty** plan
   (`crates/reprise-core/src/device_sync/mirror.rs:173-183`), so
   `playlist_removals` is never computed either.

The state is permanent for that device until the user manually deselects the
(now nameless) entry.

## Why the phone never learns about the deletion

Note the deletion propagates at the *next sync run*, not instantly: the
`.m3u` on the phone disappears when a sync is next started, not at the moment
the playlist is deleted in the library.

The removal machinery itself is correct. `plan_playlists`
(`mirror.rs:548-624`) derives `playlist_removals` from the `device_playlists`
inventory table, whose rows survive a library deletion (no FK to `playlists`,
`crates/reprise-core/src/db_device_sync.rs:60-71`). Diffing that inventory
against `plan.playlist_writes` would queue the orphaned `.m3u` for deletion.

But the `MissingPlaylist` blocker returns before any of that runs — deliberately,
per `mirror_tests.rs:465-483`
(`a_deleted_selected_playlist_blocks_instead_of_becoming_an_empty_source`).
The blocker is the correct guard; the defect is that the selection is allowed
to stay stale, so the guard never lifts.

## Why "busy" is a wide window, not a race

`is_busy()` is `is_active() || sync_phase == PlannedSyncPhase::Finishing`
(`device_sync_runtime.rs:230`). Every reset to `Idle` funnels through either
`finish_sync` reaching `Effect::Finished` (`device_sync_planned.rs:301`) or
`recompute_delta_silent` (`device_sync_compact.rs:227`), and both are gated on
unbounded async device I/O:

- After a **successful** sync, `finish_sync`'s `Completed` branch
  (`device_sync_planned.rs:256-277`) clears `device.machine` but leaves
  `sync_phase == Finishing`. It only drops to `Idle` as a side effect of the
  post-sync `backend.inspect()` verification (spawned at
  `device_sync_runtime.rs:554`), which has no timeout.
- On **disconnect**, `apply_devices` (`device_sync_device_list.rs:62-83`)
  resets `connected`, `session_state`, `scanning`, `storage` — but not
  `sync_phase` or `machine`. It only calls `cancel_device_run`
  (`device_sync_runtime.rs:791-803`), which *requests* interruption.
- **Cancel / backend error** mid-sync: same dependency on the in-flight
  `effects::perform(...).await` returning.

A hung or unplugged MTP mount therefore leaves `is_busy() == true` for the rest
of the session. `sync_log::close_orphaned_runs` only cleans persisted run-log
rows at construction (`device_sync_runtime.rs:334-338`); it cannot recover a
live `DeviceState`. This is why the report reads as "sync ist sehr
fehleranfällig" rather than as a narrow race.

## Not the cause

`0c59405a00` ("Stop a deleted playlist from haunting the device sync page",
#715) is a GTK list-widget bug only — `PlaylistCard::update()` handed the button
instead of its implicit `GtkListBoxRow` to `list.remove()`. Its two tests are
display-only and neither touches busy state, `mirror.rs`, or device-side
removal. The core path was never covered.

## Fix shape

1. **Give the prune a catch-up.** Re-validate a device's selection against the
   available playlists when it leaves the busy state (in `finish_sync` and on
   reconnect), not only on the library-change edge. Alternatively, drop the
   busy filter from the *pruning* step and keep it only for the replanning
   step. That second option is **safe**: `sync_now` recomputes the delta and
   hands `DeviceSyncMachine::new` a *cloned* `mirror_plan`
   (`device_sync_planned.rs:483-484`); the run loop
   (`device_sync_planned.rs:164-183`) never touches `device.settings` again,
   and `DeviceSyncMachine` (`machine.rs:188-302`) holds no settings reference
   at all. An in-flight sync structurally cannot observe a mid-run prune. Only
   the concurrent mutation of `device.settings`/`mirror_plan` next to the run's
   own bookkeeping needs care.
2. **Make busy terminate.** `sync_phase` should reach `Idle` on disconnect and
   on cancel without waiting for device I/O, and the post-sync verification
   needs a timeout. Otherwise every future feature that checks `is_busy()`
   inherits the same stuck state.
3. **Always notify.** Call `notify()` even when `recompute_delta_silent`
   fails, so a failed replan cannot leave a deleted playlist on screen.
4. Regression tests at runtime level (a fake `DeviceBackend` is already the
   test seam, see `device_sync_compact_tests.rs`): delete a playlist while the
   device is busy, then assert the selection is pruned and the plan carries a
   `playlist_removals` entry rather than a `MissingPlaylist` blocker.

## Open

Confirm on the real device once an app log exists:

```
REPRISE_LOG=info,reprise=debug,reprise_core=debug cargo run -p reprise-gnome 2>&1 | tee /tmp/reprise-sync.log
```

Expected on the busy path: no `could not persist device selection` line, and
the sync page showing the "no longer exists" blocker. If instead the log shows
`playlist deletion failed` (`sidebar_export.rs:194`) the failure is in
`playlists::delete` itself (e.g. SQLITE_BUSY against the sync worker) and this
document describes a second, independent defect.
