# Handoff — Android Sync (MTP): finish the work

**Status:** the feature is **merged to `main`** (`9d49082`) and **works end-to-end on real
hardware** — 63 tracks copied to a Pixel 10 Pro XL and reported synced. Gates on `main` are
green: workspace clippy clean, 1052 tests, `cargo fmt --all --check` clean.

What is left is (P1) hardening that can cost user data, (P2) correctness bugs, and (P3) the
polish the maintainer asked for. Read "Things you must not relearn" first — several of those
cost hours to find and are invisible from the code alone.

Start from a **fresh branch off `main`**. The worktree `.worktrees/synch-android-settings`
(branch `feat/synch-android-settings`) is fully merged; reuse or remove it, but do not build
new work on it. `docs/plans/android-sync.md` holds the original design decisions and the V1/V2
scope split — read it before changing scope.

---

## Things you must not relearn (hard-won)

**The maintainer's phone is attached to this machine.** You can measure instead of guess:

```bash
cargo run -p reprise-platform-linux --example probe_devices   # what GVfs exposes + what we project
cargo run -p reprise-platform-linux --example probe_copy      # one real MTP copy, prints the real error
gio mount -li | grep -A10 -i mtp                              # ground truth
gio list "mtp://<device>/Internal shared storage/Music/Reprise/"
```

Both probes were written for exactly this and found two bugs in minutes. If you change transfer
code, run `probe_copy` — and **clean up after it** (`gio remove …/Music/Reprise/Probe…`).

- **MTP has no filesystem at the device root.** The root lists *storage volumes*
  ("Internal shared storage", plus an SD card on some phones) and is read-only. Creating
  `Music/` there fails with *"Cannot make directory in this location"* — this is what made
  **every** transfer fail. Managed root = `<storage>/Music/Reprise`; `DeviceStorage::storage_root`
  resolves it (cached, gated on the `mtp://` scheme so local test roots stay verbatim).
- **GVfs lists a phone twice**: a `GProxyVolume` ("Pixel 10 Pro XL", phone icon) whose mount hangs
  off the volume, plus a top-level **shadowed** `GDaemonMount` named just `mtp`. Enumerating raw
  mounts is order-dependent — it once showed "mtp" with an iPod icon, and once showed nothing.
  Devices are projected **volume-first**; do not go back to `monitor.mounts()`.
- **Reprise manages `Music/Reprise` only.** The maintainer has ~30 GB of Rhythmbox music directly
  in `Music/` — it must never be written, moved, or deleted. `inspect()` deliberately lists *all*
  audio under `Music/` (display only, see its doc comment and the test that pins it); removals are
  driven by the `device_files` DB inventory and scoped to `Music/Reprise`. **Do not "fix" this
  asymmetry** — a previous attempt was reverted after a test showed it was intentional.
- **Cards must update in place.** `render()` used to rebuild the sidebar section on every state
  update; during a sync `notify()` fires per progress callback, so the card was destroyed between
  a click's press and release and was permanently unclickable. Cards now live in a registry keyed
  by device id (`sidebar_device_card.rs`). Keep it that way.
- **Retain subscriptions on `destroy`, not `unrealize`** (`Subscription::retain_for_widget`).
  GTK4 unrealizes widgets routinely (split-view collapse during window construction), which froze
  cards on their first render.
- **The runtime reports no transcoding step** (`SyncStep::{Removing, Copying, WritingPlaylists}`)
  and `current_track` carries **only the title**. Two spec items depend on changing that — see P3.

### Running things

- The app reads **`REPRISE_LOG`**, not `RUST_LOG`: `REPRISE_LOG=info,reprise_platform_linux=debug`.
- **Single instance**: if the maintainer's app is running, a headless launch just presents their
  window. Prefer display tests over launching the app.
- **Never open a window on the maintainer's desktop.** Headless only (`xvfb-run`).
- **One `gtk4::init` per process**: run display tests individually and exactly —
  `xvfb-run -a cargo test -p reprise-gnome <full::test::path> -- --ignored --exact --test-threads=1`.
  A loose filter matching two display tests will panic in the second.
- `reprise-gnome` is a **bin** crate: `cargo clippy -p reprise-gnome` (no `--lib`).
- **No palette literals in CSS.** The app ships several named dark themes; use `@accent_color`,
  `alpha(@window_fg_color, …)`. A test forbids `@define-color` in feature CSS, and
  `sidebar_device_card`'s test forbids the literal `#1CA98F`. GTK CSS ≠ web CSS (no `overflow`,
  no `gap`); validate with the GTK parse tests (`css_parses_in_gtk_without_dropping_declarations`)
  — an invalid property makes GTK silently drop the whole declaration.
- This branch's convention: after merging `main`, **restore the strict gates** (fix warnings the
  merge brought in), see `170b22d`.

---

## P1 — Hardening that can cost user data (do this first)

### 1. Unknown selection JSON silently becomes "sync nothing" → device wipe
`crates/reprise-core/src/device_sync/settings.rs` (`decode_selection`, ~:274)

`decode_selection` only errors on invalid JSON *syntax*. Any syntactically valid but unrecognised
payload (`{}`, `42`, a future selection shape written by a newer Reprise) falls through to an
**empty** `Sources(vec![])`. With the default `remove_deleted = true`, the next sync computes
`to_remove` = every non-pinned `device_files` row → **the whole managed library is deleted from
the phone**.

Fix: return `Err` (or a distinct "unknown selection" that suppresses removal) when the JSON is
well-formed but matches no known shape. An unrecognised payload must be distinguishable from a
genuinely empty `[]`. Test both.

### 2. A worker panic hangs the pipeline forever
`crates/reprise-platform-linux/src/device_transfer.rs` (~:183)

`state.workers` is decremented as a plain statement after the loop. If `transcode_to_opus` panics
(e.g. a GStreamer `set_property` type mismatch on some plugin version), unwinding skips it;
`next()` only returns `None` at `workers == 0`, so the consumer blocks on `available.wait()`
**forever**. The consuming backend thread owns the pipeline and is itself stuck inside `next()`,
so `Drop` never runs and **cancel cannot break it**.

Fix: decrement + `available.notify_all()` from a guard struct's `Drop` so it runs on unwind too.

### 3. Cancel leaks encoded temp files, and does not wake parked workers
`crates/reprise-platform-linux/src/device_transfer.rs` (`Drop` ~:128, worker park ~:164)

- `Drop` sets `cancelled`, notifies `space`, joins — but never removes the `ReadyFile`s still in
  the ring buffer. They are raw `/tmp` `*.opus` files (not `.part`, so `cleanup_partials` never
  touches them): up to ~200 MB orphaned per cancelled run. Fix: drain `ready.state.files` after
  the join and `remove_file` each.
- Workers park on a bare `space.wait()`; the runtime sets the shared `cancelled` flag without
  notifying any condvar, so a cancel while the buffer is full leaves both encoders asleep.
  `next()` never checks `cancelled` at all. Fix: `wait_timeout` re-checking `cancelled`, or an
  `EncoderPipeline::cancel()` that sets the flag and notifies both condvars.

**Tests are the real gap here**: fixtures are ~0.1 s WAVs, so the 200 MB backpressure path never
runs, and only a *pre*-cancelled transcode is tested. Findings 2 and 3 would pass CI today. Add
a mid-run cancel test and a worker-panic test.

---

## P2 — Correctness

4. **Two sync engines can run on one device (Critical).**
   `device_sync_planned.rs::sync_now` (~:56) never sets `device.running`; `start_or_resume`
   (`device_sync_runtime.rs` ~:581) only bails when `active_device` is a *different* device. A
   drag-and-drop enqueue during a planned sync therefore starts a second pipeline on the same
   device and overwrites `device.cancellable`, so a later cancel stops only one. Both paths are
   reachable from the same dialog. Fix: make `sync_now` mark the device busy in a way
   `start_or_resume` respects, and reject `enqueue` while a planned sync is active.

5. **A bitrate change never re-copies.** `device_sync/delta.rs` (~:42) treats a file as unchanged
   when `device_path` + source `mtime` match. Switching Opus 128 → 64 kbps changes neither, so the
   phone keeps the old files forever. `device_files.size` is stored but never compared. Fix: fold
   the expected transfer size (or the bitrate) into the comparison.

6. **`available_bytes` is stale after a planned sync.** `finish_sync`
   (`device_sync_planned.rs` ~:471) calls `recompute_delta` (DB only) but never `refresh_contents`,
   unlike the legacy path. The storage bar and the *next* sync's space pre-check then use stale
   numbers. Fix: refresh contents at the end, or decrement while copying.

7. **Collision suffixes are order-dependent.** `device_sync/transfer.rs` (~:23, `path_stem_key`
   ~:87) assigns ` (2)` by input order within one plan, so a changed selection can swap which
   colliding track owns the bare name → both re-copy and the old paths orphan. Collisions are also
   computed per-plan only, never against existing `device_files`. Fix: sort collision members by a
   stable key (track id) and seed from the existing inventory.

8. **Truncation can reintroduce a trailing dot.** `device_sync/sanitize.rs` (~:80) trims trailing
   dots *before* truncating to 120 bytes, then only `trim_end()`s whitespace. FAT strips the
   trailing dot, so the recorded path never matches the on-device name → re-copied every sync.
   Fix: `trim_end_matches(|c| c == '.' || c.is_whitespace())` after truncation.

9. **Planned progress callbacks have no generation guard.** `device_sync_planned.rs` `set_phase`
   (~:544) / `update_copy_bytes` (~:556) find the device by id and are guarded only by
   `phase == Syncing`, while the legacy path guards every mutation with `device.generation`. A late
   callback from a superseded run can corrupt the current run's `bytes_done`. Fix: carry a
   generation in `PlannedWork`.

10. **Settings are editable mid-sync and stomp the phase.** `update_settings`
    (`device_sync_runtime.rs` ~:293) unconditionally sets `sync_phase = ComputingDelta` and clears
    `sync_error` even while syncing; the switches are not desensitised during a sync. Fix: reject
    or defer while active, or desensitise the switches.

Also noted, lower severity: `load_or_create_settings` SELECT-then-INSERT is not atomic
(`ON CONFLICT DO NOTHING`); `for_job` uses non-saturating `sum()`; `device_files` has no index on
`track_id`, so the `ON DELETE CASCADE` from `tracks` is a full scan per delete; `transfer_track`'s
recv-error path skips `.part` cleanup; one unreadable playlist aborts the whole `inspect()`;
`cleanup_partials` would delete a concurrent run's in-progress `.part` (only safe because sync is
single-flight — make that invariant explicit).

---

## P3 — Polish (explicitly requested)

### 11. Storage bar like the mock (16a/17a)
Today the bar under the device name shows only Reprise-managed bytes, so it sits near zero even
with 30 GB on the phone. Target: `Music 61.4 GB · after sync +1.2 GB · Other 20.4 GB · Free 45.0 GB`.

- **Free** already works (`DeviceStorage::available_bytes`, ~164.7 GiB on the test device).
- **Music** = sum of `inspect()`'s files (it now scans the real `Music/` — this is why the fix in
  `166f8c5` matters for the bar).
- **Total/Other** need `filesystem::size` from GVfs — **verify with `probe_copy`/`gio info` that the
  MTP backend actually reports it before designing around it**; the SDD ledger already records that
  the current backend exposes no reliable total, so this may need a documented fallback (the ledger's
  assumption: total = managed bytes + free).
- **after sync** = the delta's `bytes`.

### 12. Sync animations
There are currently **zero** animations in the sync UI: the delta card jumps to the progress card,
the bar jumps per callback, the spinner pops in. Reuse the app's existing patterns rather than
inventing: `scan_progress.rs` (`Revealer`, crossfade 150 ms + pulse), `player_bar.rs`
(`AdwTimedAnimation`, 125 ms crossfade), `eq_bars.rs` (CSS `@keyframes`), `cover_accent.rs`
(`cross_fade_accent`). Highest value first: animate the progress `fraction` instead of jumping
(you stare at it for minutes), then crossfade delta ↔ progress ↔ "in sync". Respect
`gtk-enable-animations`.

### 13–14. Two card-spec items blocked on missing data
The maintainer's card spec (implemented in `7e15879`) is complete except:
- **"⟳ transcoding · Track"** — needs the encoder pipeline to report a transcoding step
  (`SyncStep` has none; encoding happens invisibly inside `EncoderPipeline`).
- **"↑ Immortal — Lorna Shore"** — needs `current_track` to carry the artist, not just the title.

Both are small backend changes; do them *with* the UI, not by guessing in the UI.

### 15. Scan + Sync stacked in a shared bottom slot
The maintainer asked for this, but `docs/plans/android-sync.md` lists "Sidebar-Bottom-Slot-Architektur
(einheitlicher Fortschritts-Slot)" explicitly under **V2, not V1**. It is an architecture change,
not a card tweak. **Ask before building it.**

---

## P4 — Refactors worth doing while you are in there

- `with_device_mut(&self, id, f)` helper — the
  `device_states.borrow_mut().iter_mut().find(|d| d.descriptor.id == id)` shape appears ~15× across
  runtime/planned/legacy and makes borrow scopes inconsistent.
- `PlannedSyncPhase::is_active()` (the `matches!(… Syncing{..} | Finishing)` test is duplicated 6×)
  and `SyncDelta::has_delta()` (3×).
- `phase_copy` (`device_view.rs` ~:382) and `delta_copy` (`preference_sync_planned.rs` ~:252) are
  near-identical phase→(title, subtitle, fraction) mappers → one function in `device_sync_strings`.
- `DeviceState` mixes legacy-queue fields (`queue, running, generation, cancellable, …`) and planned
  fields (`delta, transfer_plan, sync_phase, planned_cancel, …`); the two engines share it and
  `active_device`. This is the root of finding 4. Long term: separate them.
- Duplicated safe-relative-path predicate (`device_sync.rs` ~:312 vs `m3u.rs` ~:42) — **security
  relevant** (path-traversal guard), must not drift. Same for the display-flattening helper
  (`device_sync.rs` ~:289 vs `m3u.rs` ~:19).
- Extract the recursive GVfs directory walk shared by `inspect()` and `cleanup_partials()`.

---

## Definition of done

- Gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets` (zero warnings),
  `cargo test --workspace` (≥1052 passing).
- Display tests run individually under xvfb (see above).
- For anything touching transfers: prove it against the attached phone with `probe_copy`, and clean
  up the probe files afterwards.
- The maintainer wants a compact status per finished chunk. Keep changes in small, reviewable
  commits with the *why* in the message — this codebase's comments explain intent, not mechanics;
  match that.
