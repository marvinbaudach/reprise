---
slug: the-deleted-playlist-leaves-the-device-too
worktree:
branch:
phase: planned
codex_session:
created: 2026-09-01
---
# The deleted playlist leaves the device too

Deleting a library playlist while a phone is attached can leave that device
permanently unsyncable. The dead `SelectionSource` stays selected, `plan_mirror`
answers every later plan with an empty plan plus `MirrorBlocker::MissingPlaylist`,
and because the plan is empty the orphaned `.m3u` on the phone is never queued
for removal either. The sync page shows *"1 selected playlist no longer exists"*
and the only way out is deselecting an entry that no longer has a name.

Worse, `is_busy()` also gates starting a sync (`device_sync_planned.rs:351,400,460`),
changing the target (`device_sync_target_actions.rs:101`) and changing the
selection (`device_sync_compact.rs:22`) — a device stuck busy is not merely
unpruned, it is unusable.

Full diagnosis: `docs/plans/sync-playlist-delete-blocks-the-device.findings.md`.

## What is actually wrong

`DeviceSyncRuntime::library_playlists_changed`
(`crates/reprise-gnome/src/ui/device_sync/device_sync_runtime.rs:264-331`) is the
only place in the workspace that prunes deleted playlists out of a device's
selection (`sources.retain(...)`, `:295`). It does two different jobs in one
pass and applies one guard to both:

```rust
.filter(|device| !device.is_busy())     // :281
```

- **Pruning** the selection — must always happen. A source that no longer exists
  is not a decision anyone can act on later.
- **Replanning** (`recompute_delta_silent`) — must stay off a running sync,
  which owns its plan.

Guarding both with `is_busy()` drops a deletion made during a run on the floor,
and nothing re-runs the prune when the device goes idle: `finish_sync` and
`recompute_delta_silent` replan, they never re-validate the selection.

Pruning a busy device is safe, and this is load-bearing: `sync_now` hands
`DeviceSyncMachine::new` a **cloned** `mirror_plan`
(`device_sync_planned.rs:483-484`), the run loop (`:164-183`) never reads
`device.settings` again, and `DeviceSyncMachine` (`machine.rs:188-302`) holds no
settings reference at all. An in-flight run structurally cannot observe the
prune, and the write-back is synchronous with no `await` in between.

Beyond that, the prune is purely edge-triggered: it fires only on a playlist
CRUD event. Any device whose stored selection is *already* stale — which is the
state the reporter's phone is in right now — stays broken after the fix until
some unrelated playlist change happens to run the prune. That is the reason for
task 4.

Two smaller defects ride along:

- `self.notify()` fires only `if recomputed` (`:328`), and `recomputed` is set
  only when `recompute_delta_silent` returns `Ok`. A pruned-but-not-replanned
  device never reaches the page. There is no other route — no timer exists in
  `ui/device_sync/` outside smoke tests, and every other `notify()` call site
  reacts to an unrelated event.
- `refresh_contents_after_sync` returns early when the device is gone or its
  session is closed (`device_sync_runtime.rs:539-541`) **without** resetting
  `sync_phase`. `finish_sync`'s success branch leaves the phase at `Finishing`
  (`device_sync_planned.rs:272-277`) and relies entirely on that post-sync
  inspection to drop it to `Idle`, so a phone unplugged in exactly that moment
  stays busy — and therefore unusable — for the rest of the session.

## Decisions taken in the grill

These were settled and are not open for reinterpretation during implementation:

1. **A running sync is left alone.** It finishes writing the `.m3u` its plan
   contains. The next run diffs the surviving `device_playlists` inventory row
   against the now-empty writes and removes the file on the device then. The
   machine must not start reading `device.settings` mid-run — that would destroy
   the very property that makes task 2 safe.
2. **`is_busy()` termination is limited to the one proven path.** Resetting
   `sync_phase` on disconnect was dropped from the plan: it frees nothing while
   `machine` is still `Some`, because `is_active()` *is* `machine.is_some()`.
   That case depends on the cancelled device I/O returning, and is documented,
   not fixed here.
3. **No timeout on `backend.inspect()`.** A hung MTP mount is a real third
   failure mode but needs a policy (how long, what the card shows meanwhile,
   what happens to a resumable run) this bug does not supply. Separate plan.
4. **Self-healing lives at the two load sites, not in `recompute_delta_silent`.**
   A function named "recompute the delta" must not acquire a hidden write to the
   settings table on the hot path.

## Tasks

TDD throughout: each test goes in first and must fail for its stated reason
before the fix lands. The seam already exists — `FakeBackend` plus
`DeviceSyncRuntime::with_backend` in
`crates/reprise-gnome/src/ui/device_sync/device_sync_compact_tests.rs`, which
already covers the connected-idle and unplugged cases
(`library_playlist_deletion_refreshes_the_connected_device_projection`,
`library_playlist_deletion_prunes_an_unplugged_devices_saved_selection`), and
already drives real runs through `sync_now` (`:378, 512, 565, …`). The new tests
are siblings of those, not a new harness.

### Task 1 — one pruning function, in the core

The retain logic is about to have three call sites, so it becomes a shared
function in `crates/reprise-core/src/device_sync/settings.rs`, next to
`DeviceSelection`:

```rust
/// Returns the selection without sources that no longer exist, or `None`
/// when nothing had to be dropped.
pub fn without_missing_sources(
    selection: &DeviceSelection,
    available: &HashSet<SelectionSource>,
) -> Option<DeviceSelection>
```

Returning a new value rather than mutating in place keeps the repo's immutability
rule, and `None` is what tells every caller whether it has to persist and notify.
`DeviceSelection::EntireLibrary` can never dangle and always answers `None`.

Unit tests in the core: a selection with only live sources returns `None`; a
selection with one dead source returns the remaining ones; a selection whose
sources are all dead returns an empty `Sources(vec![])` (not `None`, not
`EntireLibrary`); `EntireLibrary` returns `None`.

### Task 2 — prune every device, replan only the idle ones

Two failing tests first, in `device_sync_compact_tests.rs`. `is_busy()` is two
states behind one name and both must be covered:

- `library_playlist_deletion_prunes_a_running_syncs_selection` — start a run via
  `sync_now` against a backend that blocks (the pattern in
  `device_sync_inflight_tests.rs`), delete the playlist, call
  `library_playlists_changed()`.
- `library_playlist_deletion_prunes_during_the_finishing_window` — put the
  device's `sync_phase` at `Finishing`, then the same steps. This is the state
  the user actually meets: the sync is done, the post-sync verification is still
  running, and they have moved on to the library.

Both assert:

1. the persisted selection (`load_or_create_settings(...).selection`) no longer
   contains the deleted source;
2. once the device is idle and replanned, `device.mirror_plan.blockers` contains
   **no** `MirrorBlocker::MissingPlaylist`, and `playlist_removals` carries the
   deleted playlist's inventory row — seed it with `upsert_device_playlist`,
   which the test module already imports.

Assertion 2 is what speaks for the second half of the report — the phone learns
about the deletion — and must not be dropped for convenience.

Then, in `library_playlists_changed`:

- drop `!device.is_busy()` from the collect filter (`:281`) and carry the busy
  flag into the tuple alongside `rememberable`;
- prune via `without_missing_sources` and persist for every device, keeping the
  existing `rememberable` gate — a transient device's settings must never reach
  `save_settings` (`settings.rs:69-71`);
- drop `&& !device.is_busy()` from the write-back's `find` (`:311-316`); with the
  filter gone it would otherwise discard the pruned selection for exactly the
  devices this task is about;
- call `recompute_delta_silent` **only** for non-busy devices.

No catch-up pass: once the prune is unconditional, the run's own
`refresh_contents_after_sync` → `recompute_delta_silent` replans against an
already-clean selection when it finishes. Do not add one.

Leave a comment at the write-back saying why the in-flight `.m3u` write is
allowed to stand (decision 1 above) — the next reader will otherwise try to
"fix" it.

### Task 3 — a failed replan still reaches the page

Failing test `a_failed_replan_still_publishes_the_pruned_selection`: drop one of
the tables `recompute_delta_silent` reads (`device_files` via `load_device_files`
is the cleanest) so the replan fails deterministically, register a subscriber
with `runtime.subscribe(...)` (`device_sync_runtime.rs:756`), delete a selected
playlist, call `library_playlists_changed()`, and assert the subscriber fired
and the published selection is pruned. A backend fake cannot produce this
failure — `recompute_delta_silent` only fails on DB errors and on "device is not
connected" — which is why the injection happens at the database.

Fix: notify when *anything* changed, not only when a replan succeeded — the
prune itself is a reason to notify. Keep the `Err` arm's `tracing::warn!`; it is
the only trace a failed replan leaves.

### Task 4 — a stale selection heals on its own

Failing test `a_stored_selection_drops_a_deleted_playlist_on_connect`: write a
selection referencing a playlist id that does not exist in the library, bring the
device up, and assert that the stored selection is pruned and the page carries no
`MissingPlaylist` blocker — without any playlist CRUD happening.

Fix: call `without_missing_sources` right after the settings are loaded, at both
sites, persisting when the result is `Some` and the device is rememberable:

- `load_device_memory` (`device_sync_memory.rs:48`), reached from
  `device_sync_device_list.rs:116` when a device connects;
- `load_remembered_device_memories` (`device_sync_memory.rs:20-45`) at startup.

This is what turns the prune from edge-triggered into state-based, and it is what
frees devices that are already broken today.

### Task 5 — the finishing window terminates

Failing test `an_unplugged_device_does_not_stay_busy_after_a_sync`: run a sync to
success, disconnect the device before the post-sync inspection resolves, and
assert the device is no longer busy (through the public surface: it is startable
again / `device_busy` is false).

Fix: in `refresh_contents_after_sync` (`device_sync_runtime.rs:535-545`), set
`sync_phase = PlannedSyncPhase::Idle` on the early return for a disconnected
device or closed session, before returning. `machine` is already `None` at that
point (`finish_sync` clears it first), so this reset fully frees the device.

Add a short comment naming what is *not* fixed: a device whose run is cancelled
by a disconnect stays busy until the cancelled I/O returns, because
`is_active()` is `machine.is_some()`. That is decision 2, and the comment is
what stops the next reader from re-deriving it.

## Verification

- `cargo test -p reprise-core device_sync` — task 1's unit tests.
- `cargo test -p reprise-gnome device_sync` — the five new runtime tests fail
  first, each for its stated reason, and pass after.
- The two existing deletion tests stay green. If either needs editing, the
  change went further than intended — stop and say so.
- `cargo clippy --workspace --all-targets` clean.
- Manual, with the phone attached: start a sync, delete a selected playlist
  while it runs, let the run finish. The page must lose the row without a
  restart and show no "no longer exists" blocker; the next run must delete the
  `.m3u` on the device. Separately: with a device whose stored selection is
  already stale, simply attaching the phone must clear it.

## Parallelität

**No cut. One strand.**

The GNOME layer cannot compile without task 1's core function, so a "core" and a
"wiring" strand would not be two parallel strands but one forced order — strand B
could not go green before strand A landed, which is exactly the failure mode the
cut rule exists to prevent.

Beyond that, tasks 2, 3 and 5 all change
`crates/reprise-gnome/src/ui/device_sync/device_sync_runtime.rs`, and the tests
for tasks 2 through 5 all land in
`crates/reprise-gnome/src/ui/device_sync/device_sync_compact_tests.rs`. There is
no file group that splits two ways without an overlap.

File ownership for the single strand:

- `crates/reprise-core/src/device_sync/settings.rs`
- `crates/reprise-gnome/src/ui/device_sync/device_sync_runtime.rs`
- `crates/reprise-gnome/src/ui/device_sync/device_sync_memory.rs`
- `crates/reprise-gnome/src/ui/device_sync/device_sync_compact_tests.rs`
- plus the core's own test module for task 1

No post-merge cross-checks: with one strand, every verification step reads only
files that strand owns.
