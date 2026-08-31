---
slug: the-playlist-card-says-why-it-is-locked
worktree: /home/marvin/Projects/reprise-the-playlist-card-says-why-it-is-locked
branch: feature/the-playlist-card-says-why-it-is-locked
phase: shipped
codex_session:
created: 2026-08-29
---
# The playlist card says why it is locked

## Goal

While a device sync is running, the sync page's playlist selection says *why* it
cannot be changed instead of just going grey, and no control in that card fails
silently. Separately, a device whose page is on screen is never skipped by the
staleness refresh.

## Why (measured, not assumed)

Reported symptom: a playlist created through the MCP server could not be
selected on the sync page; the row was visible but disabled.

Reconstructed from the user's own database (`~/.local/share/reprise/reprise.db`):

```
change_log  2453 | playlist 3 | create | foreign writer | 2026-08-29 11:15:49
sync_runs     60 | cancelled  | 11:07:40 → 11:16:00     | planned 271, copied 79
```

The playlist was created 11 seconds before a running sync was cancelled, and the
device has `sync_automatically = 1`, so that run had started by itself on
connect. `DeviceState::view()`
(`crates/reprise-gnome/src/ui/device_sync/device_sync_runtime.rs:138-145`) passes
`self.is_active()` into `SyncPageState::update_controls`, which sets
`editable: !active` (`crates/reprise-core/src/device_sync/page.rs:77`), and
`PlaylistCard::update` binds every row's sensitivity to it
(`device_sync_playlist_card.rs:180`). So the whole card is grey for a run's
entire duration — and nothing says so. The following runs in the log (#61 started
11:16:12 and cancelled after 11 seconds, #62 started 11:16:34) are the user
cancelling to get at the selection.

The freeze itself is deliberate — `library_playlists_changed` documents it: "An
active sync owns its plan until completion and must not be disturbed by a
concurrent library edit." What is missing is the explanation, and two silent
failures around it.

### The silent failures

- The **"Choose playlists" button** is never desensitised (`device_sync_playlist_card.rs:44-56`
  builds it; the update loop only touches the row buttons). It stays clickable
  during a sync, but `save_picker` → `settings_for_update`/`update_settings`
  returns `Err("device synchronization is active")`
  (`device_sync_compact.rs:22-24`), and `device_sync_picker.rs`'s Save handler
  only closes the dialog `if …is_ok()`. The user presses Save and the dialog
  just sits there.
- A row toggle during a sync reaches
  `device_sync_page_actions.rs:35`, which logs a warning and leaves the
  `ToggleButton` visually flipped until the next update resets it.

### The staleness guard (separate defect, same branch)

`DeviceSyncRuntime::recompute_if_stale`
(`device_sync_runtime.rs:357-373`, added by #728) finds its device with
`… && device.connected`. The page's own presentation gate is
`device_sync_remembered::apply` → `connected || session_state == Remembered`.
The two predicates disagree, so a remembered but disconnected device — whose
page is fully drawn and interactive since #726 — is excluded from every
immediate refresh path: the external-change hook
(`window_external_changes_wiring.rs:24`), the page's `connect_map`
(`device_sync_page.rs:365`) and `picker_snapshot`
(`device_sync_picker_runtime.rs:40`) all go through it. It returns `Ok(())` with
no trace, so the skip is invisible. Only the periodic device-list poll
(`device_sync_device_list.rs:155`) eventually re-projects.

## Task 1 — one predicate for "this page is interactive"

`crates/reprise-gnome/src/ui/device_sync/device_sync_remembered.rs`

- Extract the condition currently inlined in `apply` into a small public
  function over the two facts it needs, e.g.
  `pub(super) fn page_is_readable(connected: bool, session_state: &DeviceSessionState) -> bool`,
  and have `apply` call it. Do not re-spell the condition anywhere else.

`crates/reprise-gnome/src/ui/device_sync/device_sync_runtime.rs`

- `recompute_if_stale` uses that function instead of `&& device.connected`.
  Keep the `library_dirty` check and the early `Ok(())` for a device that is not
  in `device_states` at all.
- When a device *is* found but the predicate excludes it, emit a
  `tracing::debug!` naming the device and the reason before returning `Ok(())`.
  A silent skip on a surface the user is looking at is what made this defect
  invisible; do not leave it silent.

## Task 2 — the card says why it is locked

`crates/reprise-gnome/src/ui/device_sync/device_sync_strings.rs`

- Add the copy following the existing `EJECT_BLOCKED_SYNCING` /
  `eject_tooltip(blocked)` idiom in this file — a `N_!` constant plus a small
  helper that returns the plain label when unlocked and the reason when locked.
  Wording: the selection is locked while this device is synchronizing, and
  cancelling the sync is how to change it. Keep it one sentence.
- Follow whatever this repo already requires for a new translatable string
  (the file is an existing gettext source; do not invent a new mechanism).

`crates/reprise-gnome/src/ui/device_sync/device_sync_playlist_card.rs`

- Keep the `choose` button on the `PlaylistCard` struct so `update` can reach
  it. Bind its sensitivity to `device.page.controls.editable` exactly like the
  row buttons — an escape hatch whose Save silently refuses is worse than a
  disabled button.
- Add a dedicated label to the card under the header that is visible only while
  `!editable` and carries the reason. Do not overload the existing `summary`
  label — it carries the track-count and size reading and must keep doing so.
  Use `set_visible` for the toggle, not by adding/removing the widget.
- Give the row buttons and the `choose` button a tooltip naming the reason while
  locked, and clear it when unlocked — this is what `eject_tooltip` already does
  for the eject button, so match that shape. A disabled GTK4 button still shows
  its tooltip; that is the point.
- Nothing about `editable`'s meaning changes. Do not touch
  `SyncPageState::update_controls` or make the toggles operable during a sync.

## Task 3 — tests

- A display test in the style of
  `crates/reprise-gnome/src/ui/device_sync/device_sync_playlist_rows_display_tests.rs`:
  with `controls.editable == false`, the card shows the reason and both the row
  buttons and the "Choose playlists" button are insensitive; with
  `editable == true`, the reason is not shown and both are sensitive. Mark it
  `#[ignore = "requires a display; run via xvfb-run"]` like its neighbours if it
  needs GTK init.
- A runtime test in the style of
  `crates/reprise-gnome/src/ui/device_sync/device_sync_remembered_tests.rs` or
  `device_sync_presence_tests.rs`: a remembered, disconnected, `library_dirty`
  device is re-projected by `recompute_if_stale`, where before the change it was
  skipped. This is the regression that would have caught the guard.
- Keep every existing test green; several construct `SyncPageControls` and
  `PlaylistCard` directly.

## Out of scope

- Making the selection editable during a running sync, or queueing a selection
  change to apply afterwards. The freeze stays; only its explanation is added.
- Surfacing `set_playlist_selected`'s error as a toast. With the controls
  correctly desensitised the error path is no longer reachable from the UI.
- Anything in `reprise-core`. This change is presentation plus one predicate in
  the GTK runtime.

## Verification

- `cargo test -p reprise-gnome device_sync` — green, including the new tests.
- `cargo clippy -p reprise-gnome --all-targets -- -D warnings`.
- Do not run the app.

## Parallelität

Not cut. Task 2 is one card widget plus its strings, task 1 is two functions in
two files, and task 3's tests sit next to both — the file groups overlap on
`device_sync_playlist_card.rs` and `device_sync_runtime.rs` the moment tests are
written, and the whole change is well under a single Codex run.
