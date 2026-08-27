---
slug: external-changes-reach-device-sync
worktree: /home/marvin/Projects/reprise-external-changes-reach-device-sync
branch: feature/external-changes-reach-device-sync
phase: reviewed
codex_session:
created: 2026-08-26
---
# External changes reach the device-sync page

## The problem

A playlist written by a foreign process — the MCP server, over the same
database — appears in the sidebar within a second and never appears on the
device-sync page. Not in the page's playlist list, not in the `Choose
playlists` picker. `Check again` does not help, because it recomputes a delta
from a cache that nothing refreshed.

Observed 2026-08-26 while shooting the showreel: `mcp-playlist.py` created
`Like Immortal Disfigurement · 100` over real stdio JSON-RPC, the sidebar row
appeared live, and the device page went on offering only `Lorna Shore &
Similar · 300`, `Recently added`, `Recently played`, `Top rated`.

## Evidence

Every claim below is a line in the tree, not an inference.

| Fact | Where |
|---|---|
| `start(db_path, excluded, apply: Rc<dyn Fn(RefreshPlan)>)` — one apply hook | `external_changes/mod.rs:94` |
| Drain loop calls `apply(plan)` on the GTK thread | `external_changes/mod.rs:135-144` |
| `RefreshPlan { sidebar, track_list, conversion }` — three coarse bools | `external_changes/coalesce.rs:21-25` |
| `impact()` maps `playlist \| smart_playlist \| library` → `sidebar + track_list` | `external_changes/coalesce.rs:63-67` |
| The only subscriber: refreshes sidebar and track list, nothing else | `window/window_external_changes_wiring.rs:16-47` |
| Its call site passes only `track_list` and `sidebar` | `window/window_runtime_wiring.rs:615` |
| The window does hold the runtime: `pub device_sync: Rc<DeviceSyncRuntime>` | `window/window_runtime_setup.rs:29` |
| `picker_snapshot(&self, device_id)` reads named playlists from the **cache** | `device_sync/device_sync_picker_runtime.rs:44-50` |
| The cache is written only by `recompute_delta_silent` | `device_sync/device_sync_compact.rs:132`, writes at `:205` |
| `recompute_delta(self: &Rc<Self>, device_id: &str) -> Result<(), String>` | `device_sync/device_sync_compact.rs:126` |
| `device_states: RefCell<Vec<DeviceState>>` — "known" means *connected* | `device_sync/device_sync_runtime.rs:205` |
| `recompute_delta_silent` errors with `"device is not connected"` otherwise | `device_sync/device_sync_compact.rs:134-148` |
| The runtime already keeps `weak_self: RefCell<Weak<Self>>` | `device_sync/device_sync_runtime.rs:208`, set at `:317`, used at `:464` |

### What the grill added, and what it removed

**There is no playlist-specific signal, and there does not need to be.** A
playlist write and a track edit arrive as the same two bools. Marking dirty is
cheap enough that `plan.sidebar` is the right trigger; the expensive half is
deferred to the moment of display.

**A recompute is not cheap.** Per device, `recompute_delta_silent` runs
`load_device_files` (743 rows on the reference handset), `load_device_playlists`,
`load_mirror_playlist_snapshots`, a settings read, and often
`load_everything_playlist_snapshot`. Doing that per change_log batch is what the
design has to avoid.

**The runtime has no notion of visibility.** There is no `selected_device`, no
`visible` field. The answer has to come from the widget layer.

**The coalescer is already tested.**
`read_and_plan_surfaces_a_foreign_playlist_and_advances_the_cursor`
(`external_changes/tests.rs:39`) asserts a `playlist` entry yields
`RefreshPlan { sidebar: true, track_list: true, conversion: false }`. No new
coalescer test is needed.

**A whole leg was cut.** The draft proposed a second, independent guarantee:
`picker_snapshot` reading named playlists straight from
`load_mirror_playlist_snapshots`. That is wrong. `available` — the field the
picker filters on (`device_sync_picker_runtime.rs:49`) — is not on
`MirrorPlaylistSnapshot` at all; it is produced by the projection
(`reprise-core/src/device_sync/page.rs:18`, written by `project_sync_page`).
Reading raw snapshots would silently lose the filter, and doing it properly
would mean running `project_sync_page` in a second place — a duplicated
projection that can drift from the one `recompute_delta_silent` already runs.
Cut, with the reasoning recorded here so nobody re-proposes it.

## What must be true when this is done

1. A playlist created by a foreign writer appears in the device-sync page's
   playlist list without the user pressing anything — **including while the page
   is already open**, because that is the case the sidebar already handles and
   the case the feature is demonstrated in.
2. The same playlist appears in the `Choose playlists` picker, whether or not
   the page was open when the write landed.
3. A library scan that appends thousands of `change_log` rows does not make the
   device-sync page recompute thousands of deltas. With no device page on
   screen, the cost of an external change stays at setting a flag.
4. A regression test that runs in an ordinary `cargo test` — not behind
   `#[ignore]` — fails if the invalidation or the recompute is removed, and it
   drives the same entry point the picker drives.

## The design

**Invalidate on the signal; recompute at the moment of display, or at once if
the page is already on screen.**

The apply hook marks every connected device stale. Nothing else happens there.
The recompute is pinned to display:

- the device-sync page becoming visible recomputes **that** device;
- the picker opening recomputes **that** device before it snapshots;
- and if the page is visible *at the moment the change arrives*, that device
  recomputes immediately, so an open page behaves like the sidebar rather than
  going quietly stale.

Requirement 3 holds because the work is bounded by how often somebody looks at a
device page, not by how often the library changes. Requirement 1's live half
holds because of the immediate case, and its lazy half because of the show hook.

Two details carry the design and both were decided in the grill:

**The dirty flag is per device**, a `bool` on `DeviceState`. A global flag would
be wrong, not merely coarse: showing device X would clear it and leave device Y
stale for good. Marking sets the flag on every entry in `device_states`;
recomputing clears only the one it recomputed. At most one device is recomputed
per event.

**Visibility comes from the widget, through one source.** `connect_map` on the
device-sync page root is "becomes visible"; `is_mapped()` on the same widget is
"is visible". Any route onto the page maps it, so no navigation call site has to
be found and none can be missed.

**The freshness guarantee lives in the entry point, not in its callers.** A new
`picker_snapshot_fresh(self: &Rc<Self>, device_id)` checks the flag, recomputes
if set, and then delegates to the existing `picker_snapshot`. The picker calls
it; the regression test calls it. That is what lets the test drive the real path
without a display, and it resolves the `&self` / `&Rc<Self>` mismatch cleanly —
`picker_snapshot` keeps its `&self` signature and gains a wrapper rather than a
new receiver.

## Tasks

1. **A per-device stale flag.** Add `library_dirty: bool` to `DeviceState`
   (`device_sync_runtime.rs`), defaulting to `false`. Add to
   `DeviceSyncRuntime`:
   - `mark_all_devices_stale(&self)` — sets the flag on every entry in
     `device_states`;
   - `recompute_if_stale(self: &Rc<Self>, device_id: &str) -> Result<(), String>`
     — if that device's flag is set, clear it and call `recompute_delta`;
     otherwise return `Ok(())`. A device id that is not connected is not an
     error here: it means there is nothing to recompute.

   Reach the `Rc<Self>` through the existing `weak_self`
   (`device_sync_runtime.rs:208`), the way `:464` already does. Do not add a
   second self-reference pattern.

2. **`picker_snapshot_fresh`.** In `device_sync_picker_runtime.rs`, add
   `pub(crate) fn picker_snapshot_fresh(self: &Rc<Self>, device_id: &str) ->
   Result<PickerSnapshot, String>` that calls `recompute_if_stale` and then the
   existing `picker_snapshot`. Change the picker's `present()`
   (`device_sync_picker.rs:25`) to call it. Leave `picker_snapshot` otherwise
   untouched, and leave a short comment there saying its cached read is correct
   because `picker_snapshot_fresh` is the only way in — so the next reader does
   not mistake the asymmetry for the bug it looks like.

3. **The page's two visibility hooks.** On the device-sync page root
   (`device_sync_page.rs`): `connect_map` calls `recompute_if_stale` for the
   device that page is showing. Expose whatever is needed for the apply hook to
   ask two questions — is the page mapped, and which device is it showing — as a
   small accessor rather than by handing the widget around.

4. **Widen the wiring by one argument.**
   `start_external_changes_refresh(db_path, track_list, sidebar)` becomes
   `(db_path, track_list, sidebar, device_sync)`, taking
   `&Rc<DeviceSyncRuntime>` and downgrading it to a `Weak` exactly as the other
   two are downgraded (`window_external_changes_wiring.rs:16-47`). Update the
   call site (`window_runtime_wiring.rs:615`) to pass the runtime the window
   already holds (`window_runtime_setup.rs:29`).

   The device-sync leg of the apply closure must be a **free function** over
   `&Rc<DeviceSyncRuntime>` and `&RefreshPlan`, not a lambda body — that seam is
   what task 5 calls. Its body: on `plan.sidebar`, `mark_all_devices_stale()`,
   then, if the page is mapped, `recompute_if_stale` for the device it shows.

   Widening the signature is deliberate: the one link no test covers is "the
   window actually passes the runtime in", and a mandatory parameter makes the
   compiler cover it.

5. **The regression test**, in `external_changes/tests.rs` beside its siblings,
   **not** `#[ignore]`d:
   - a file-backed temp DB and a `DeviceSyncRuntime` with one connected device,
     following the harness the device-sync tests use
     (`crate::test_db::connection`, e.g. `device_sync_memory_tests.rs:16`);
   - assert `picker_snapshot_fresh` does **not** list the playlist;
   - create it through `reprise_core::library::playlists::create` on a second
     connection — the facade is what appends the `change_log` row
     (`external_changes/tests.rs:41` uses exactly this);
   - call the free function from task 4 with
     `RefreshPlan { sidebar: true, track_list: true, conversion: false }`;
   - assert `picker_snapshot_fresh` now lists it.

   Reverting task 1, 2 or 4 must turn this red. No direct `recompute_delta` call
   anywhere in the test — that is the whole point of routing through
   `picker_snapshot_fresh`.

## Verification

- `cargo test -p reprise-gnome device_sync external_changes` — the new test
  green, the existing picker and external-change tests still green.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- **Control arm, not optional:** revert task 4 alone, confirm the new test goes
  red, restore. A regression test nobody has seen fail is not evidence that it
  can fail.
- Live check, because this is what the showreel needs: run against the real
  database, create a playlist with `scripts/showreel/mcp-playlist.py`, and
  confirm it twice — once by opening the device page **after** the write without
  touching `Check again`, and once with the page **already open** when the write
  lands.

## Risks

- **The immediate case does work while a page is open.** A library scan with the
  device page in the foreground recomputes one device per coalesced batch
  (250 ms debounce). That is the price of not going stale in front of the user.
  If it ever measures badly, the answer is to coalesce the recompute, not to
  drop the immediate case.
- **Another session is editing this directory.** Uncommitted in the main
  checkout: `device_sync_page_layout.rs`, `device_sync_page_tests.rs`,
  `device_sync_page_display_tests.rs`, plus an untracked
  `device_sync_playlist_rows_display_tests.rs`. This plan touches
  `device_sync_runtime.rs`, `device_sync_picker_runtime.rs`,
  `device_sync_picker.rs`, `device_sync_page.rs` and the window wiring —
  `device_sync_page.rs` is the one that could collide. Cut the worktree from
  `origin/dev` (`d5c275602f`) and re-check that file before landing.
- **`connect_map` fires on every map, including a re-show with nothing stale.**
  That is a flag read and an early return, which is the intended cost.

## Parallelität

The cut was attempted and is not taken.

After the grill removed the picker leg, every remaining task is one causal
chain: the flag, the entry point that consumes it, the page hooks that call it,
the widened wiring that sets it, and the one test that only passes when all of
them exist. Splitting it anywhere produces a strand whose test cannot go green
in principle — the exact failure this section exists to prevent, and the one
that cost strand D a whole run in the Flathub wave.

The one file-disjoint candidate, the picker leg against the rest, no longer
exists as work.

**Verdict: one strand.** No `strands:` key, no suffix files, no merge order, and
no post-merge cross-checks — nothing here is compared across a boundary.
