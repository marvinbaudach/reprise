---
slug: the-sync-stops-paying-per-file
worktree: /home/marvin/Projects/reprise-the-sync-stops-paying-per-file
branch: feature/the-sync-stops-paying-per-file
phase: shipped
codex_session:
created: 2026-09-02
---
# The sync stops paying per file

## Why

Device sync reports ~2 MiB/s on a USB3 link that measures **30.6 MB/s** for a
single 200 MB file. Nothing here is bandwidth-bound. Every byte moved is
surrounded by fixed per-object cost, and a run pays that cost three to four
times per track.

Full measurements and file/line evidence live in
`docs/plans/device-sync-throughput-findings.md`. The numbers this plan is built
on, all measured against the connected Pixel 10 Pro XL and this library:

| Quantity | Measured |
|---|---|
| Bulk throughput, native `mtp://` | **30.6 MB/s** (200 MB / 6.54 s) |
| Direct `copy` per object, fresh folder | **0.443 s** |
| `copy → .part` + `query_info` + rename | **0.930 s** |
| Recursive walk of `/Music/Reprise` (1979 files, 320 dirs) | **8.0 s** (8.17 / 7.98 on two runs) |
| Opus 160 transcode of one FLAC | **1.3–2.6 s**, mean ~1.7 s |
| Library composition | **1582 FLAC / 399 MP3 / 2 M4A** of 1983 tracks |
| Local tracks with a `.lrc` beside them | **1602 of 1983 (81 %)** |
| On the device today | 601 opus + 148 mp3, 673 `.reprise-analysis`, **552 `.lrc` totalling 0.8 MiB** |

### The per-track budget today

A fresh copy of a FLAC track with lyrics, in the order the machine performs it:

```
transcode (opusenc, serial, blocks the loop)   ~1.70 s
audio copy   .part + info + rename + payload    ~1.11 s   (5.6 MB @ 30.6 MB/s = 0.18 s)
lyrics copy  .part + info + rename              ~0.93 s   (unconditional, every time, for 1.5 KB)
analysis     .part + info + rename              ~0.93 s   (only when its size changed)
                                                 -------
                                                 ~4.67 s per track → 1.2 MB/s
```

Plus a fixed ~8 s per run for a second full tree walk nobody needs.

**Under 4 % of a sync moves bytes.** That is the defect.

### The projected budget

```
transcode          overlapped at depth 3, off the critical path
audio copy         0.443 + 0.18                     ~0.62 s
lyrics copy        skipped in steady state           ~0.00 s
analysis           skipped in steady state           ~0.00 s
                                                     -------
                                                     ~0.62 s per track → ~9 MB/s
```

The four changes interact, which is why they ship together: after 4, 2 and 1 the
transcode becomes the bottleneck, and change 3 is sized (depth 3, not 1) so that
the device is the limit again rather than the CPU.

## Scope

In: the four changes below, in the order 4 → 2 → 1 → 3.

Out: replacing gvfs with direct libmtp bindings. That would cut the D-Bus layer
underneath all of this, but it is a rewrite of the platform layer and everything
here is available first at a fraction of the cost. Revisit only if these four
land and per-object cost is still the wall.

## Decisions taken in the grill

These are settled. Do not re-open them during implementation.

1. **One walk returns three lists.** `is_known_managed_item_file`
   (`device_sync_inspection.rs:201-205`) filters out both `.part` *and* `.lrc`,
   so neither the partial sweep nor a lyrics size gate can read `managed_files`.
   The walk classifies as it goes and returns all three.
2. **The lyrics sidecar becomes a planned effect**, not a side effect of the
   audio copy — with the consequences that a corrected `.lrc` now syncs on its
   own and lyrics count as work units.
3. **`.part` + rename goes for tracks and sidecars, stays for playlists.**
   `publish()` and `PARTIAL_SUFFIX` therefore survive.
4. **Transcode prefetch depth 3.**
5. **One strand**, commit per change.
6. **The control arm is a bounded real sync**, compared in seconds per track,
   never via `bytes_per_second`.

---

## Change 4 — walk the managed tree once, and classify while walking

Ordered first: it is the smallest blast radius, it is what makes keeping the
`.part` sweep free (change 2 relies on that), and it produces the lyrics
inventory change 1 needs.

### The defect

Two near-identical breadth-first walks of the same tree run per sync:

- `cleanup_partials_in` (`device_sync.rs:406-447`) — `VecDeque` BFS, batched
  `enumerate_children_future`, deletes anything ending in `.part`. Driven by
  `Effect::CleanPartials`, emitted on `(Awaiting::Start, Event::Start)`
  (`machine.rs:315`), i.e. before planning.
- `inspect_target_folder` (`device_sync_inspection.rs:96-144`) — the same BFS,
  the same batching, collecting `ManagedDeviceFile { relative_path, size_bytes }`
  through the `accept` predicate `is_known_managed_item_file`. Driven by the
  planner.

Measured on this tree: **8.0 s each**, 1979 files, 320 directories. One is free.

### The change

1. `crates/reprise-platform-linux/src/device_sync_inspection.rs` — give
   `inspect_target_folder` a result struct rather than a `Vec`:

   ```rust
   pub struct ManagedWalk {
       pub managed_files: Vec<ManagedDeviceFile>,  // unchanged contents
       pub partial_paths: Vec<String>,             // "*.part"
       pub lyrics_files: Vec<ManagedDeviceFile>,   // "*.lrc"
   }
   ```

   The classification happens in the existing per-entry branch. Keep
   `is_known_managed_item_file` exactly as it is: `managed_files` must keep
   excluding both classes, so the planner's view of the tree does not change and
   `plan_orphan_removals` (`mirror.rs:508`) never sees a `.part` or a `.lrc`.
2. Carry `partial_paths` into the plan. `Effect::CleanPartials` keeps its place
   and its event, but its performer deletes the listed paths instead of walking.
3. `cleanup_partials_in` (`device_sync.rs:406`) becomes delete-by-path.
   `DeviceBackend::cleanup_partials` (`device_sync_types.rs:56`) and
   `GioDeviceBackend` (`device_sync_backend.rs:97`) follow.
4. `lyrics_files` is carried into the planner for change 1.

### The ordering trap

Today the sweep runs **before** the inspection; afterwards it necessarily runs
**after** it, because the inspection is what finds the files. That is safe only
because `is_known_managed_item_file` keeps `.part` out of `managed_files` — which
it does today (`!name.ends_with(".part")`, `:203`) and must keep doing.
`is_removable_managed_path` (`mirror.rs:537-546`) does **not** exempt `.part`, so
if a `.part` ever reached `managed_files` the plan would grow a phantom removal.
Assert this in a test rather than trusting the predicate to stay put.

### Verification

- `cleanup_partials_removes_only_orphaned_part_files_under_the_managed_root`
  (`device_sync_tests.rs:432`) is rewritten against the new shape and keeps its
  guarantee: only `.part` files under the managed root are removed.
- A `.part` file on the device appears in neither `managed_files` nor any orphan
  removal.
- **One sync performs exactly one recursive enumeration of the managed root.**
  `FakeBackend` (`device_sync_fake_backend.rs`) counts enumerations; a comment
  claiming "one walk" is not evidence.

---

## Change 2 — drop `.part` and the rename for tracks and sidecars

### The defect

`replace_managed` (`device_sync.rs:472`) writes to `<name>.part`, verifies the
size, then `publish` (`:713`) does `delete_if_present(target)` → `move_future` →
`verify_published`. Measured against a direct copy, interleaved per file into two
fresh folders with the `.part` arm running first so neither arm inherits the
other's folder growth:

```
A direct copy                        0.443 s/file
B .part + query_info + rename        0.930 s/file
delta                               +0.487 s/file   (−52 % when removed)
```

The ceremony costs more than the copy it protects, and it is paid on the audio
file and on both sidecars.

### The change

`crates/reprise-platform-linux/src/device_sync.rs`:

1. In `replace_managed`, copy straight to `target` with
   `gio::FileCopyFlags::OVERWRITE` — no `.part` name, no `publish` call.
2. On copy error **and on cancellation**, `delete_if_present(target)`.
3. After a successful copy keep the size check, now on the target: reuse
   `verify_published(target, expected_size)` (`:739`). On mismatch,
   `delete_if_present(target)` and return the existing
   `DeviceIoError::SizeMismatch` / `PublishNotApplied`. **The error surface does
   not change.**
4. `replace_playlist` (`:533`) is **left alone** — it keeps `.part` and
   `publish`. Playlists are 2–4 files per run at ~0.5 s each; giving that up buys
   ~2 s and risks the phone's player reading a half-written index. `publish()`
   and `PARTIAL_SUFFIX` therefore both stay, and so does the sweep from change 4.
5. Not in the benchmark, worth expecting: `publish`'s `delete_if_present(target)`
   was an extra round trip on every *replacement*. The benchmark copied to fresh
   names and never paid it, so real replacements improve by slightly more
   than 0.487 s.

### What this costs, deliberately

Today an interrupted track copy leaves `X.part`, swept at the start of the next
run. Afterwards it leaves a **truncated file at the final name**.

Recovery is unaffected: `inventory_matches` (`mirror.rs:664-670`) compares
Reprise's own local `DeviceFileRecord`, written only after a verified copy. A
failed copy writes no record, so the next run re-plans it and `OVERWRITE` repairs
the file. The `.part` sweep was never what made re-copying work.

What is lost is the window between an abort and the next sync: Android's media
scanner indexes the truncated file, and the phone's own player shows a broken
track until then. **This was put to the user and accepted as the price of −52 %.**
Do not silently re-introduce `.part` on the track path to "fix" it. Step 2 above
narrows the window to a hard crash or an unplug, because an ordinary Cancel now
deletes the target.

### Verification

Seven existing tests encode the old behaviour. Each states a real guarantee that
survives in a new form — rewrite them, do not delete them (`device_sync_tests.rs`):

| Line | Test | Becomes |
|---|---|---|
| 260 | `replacement_verifies_the_partial_size_before_overwriting_the_final_file` | verifies the *target* size after copying and deletes it on mismatch |
| 330 | `mtp_21_a_published_file_is_proven_by_its_expected_byte_count` | unchanged — `verify_published` survives |
| 347 | `mtp_21_a_rename_that_left_nothing_behind_is_reported_not_believed` | "a copy that left nothing behind is reported, not believed" |
| 358 | `mtp_21_replacing_an_existing_track_publishes_it_without_leaving_a_partial` | replacing writes the final name directly and leaves no litter |
| 384 | `mtp_21_rewriting_a_playlist_replaces_it_without_leaving_a_partial` | **unchanged** — playlists keep `.part` |
| 410 | `pre_cancelled_copy_leaves_no_partial_file` | a cancelled copy leaves no *target* file |
| 432 | `cleanup_partials_removes_only_orphaned_part_files_under_the_managed_root` | already rewritten by change 4; the sweep stays |

New: a copy that fails mid-way leaves no file at the final name.

---

## Change 1 — the lyrics sidecar gets a change gate and its own effect

### The defect

`copy_lyrics_sidecar` (`device_sync_effects.rs:543`) is not planned. It fires
from inside the `Ok(_)` branch of the `Effect::CopyTrack` arm
(`device_sync_effects.rs:190-198`), reads the local `.lrc`
(`lyrics_sidecar.rs:44`), and always calls `replace_track`. It never asks what is
already on the device.

The analysis sidecar, three files away, does the right thing:
`plan_analysis_sidecars` (`mirror.rs:364-415`) builds a `resident` map and skips
the write when sizes match:

```rust
let existing_size_bytes = resident.get(device_path.as_str()).copied();
if existing_size_bytes == Some(size_bytes) {
    continue;
}
```

552 `.lrc` files sit on the device, 0.8 MiB in total — about 1.5 KB each, each
costing ~0.93 s every time its track's audio is copied.

### The change

1. `crates/reprise-core/src/device_sync/mirror.rs` — add `plan_lyrics_sidecars`,
   modelled on `plan_analysis_sidecars` and called from the same place. Its
   `resident` map comes from **`ManagedWalk::lyrics_files`** (change 4), not from
   `managed_files`, which excludes `.lrc` by design. Per desired file, use
   `lyrics_sidecar::paths_for_track` + `lyrics_sidecar::source_file_size`
   (`lyrics_sidecar.rs:16`, `:44`) and push
   `LyricsSidecarWrite { track_id, source_path, device_path, size_bytes,
   existing_size_bytes }` onto `plan.lyrics_writes` only when
   `existing_size_bytes != Some(size_bytes)`.
2. `crates/reprise-core/src/device_sync/machine.rs` — add
   `Effect::WriteLyrics { index }`, `Event::LyricsWritten(Result<u64, String>)`,
   `Awaiting::WriteLyrics(usize)` and an `enter_lyrics_writes` phase modelled on
   `enter_analysis_writes` (`machine.rs:549-564`), placed next to the analysis
   writes in the phase order.
3. `crates/reprise-gnome/src/ui/device_sync/device_sync_effects.rs` — remove the
   `copy_lyrics_sidecar` call from the `Effect::CopyTrack` arm and rewrite
   `copy_lyrics_sidecar` as the `Effect::WriteLyrics` performer returning
   `Event::LyricsWritten`.

### Accounting

Lyrics bytes are added to `plan.transfer_bytes` and `plan.target_bytes` for the
planned writes, the same way the analysis sidecar does. **`reprise_music_bytes`
is left alone** — it is folded from `managed_files`, which keeps excluding
`.lrc`. The storage bar therefore does not change, at the cost of a 0.8 MiB drift
(0.02 %) in the "after sync" projection. That is cheaper than dragging a display
change through this plan.

### Three consequences to state, not discover

- **Lyrics now update without the audio changing.** Today a corrected `.lrc`
  never reaches the phone unless its audio is recopied. Expect no catch-up burst
  on the first run: the 552 resident `.lrc` were copied verbatim, so their sizes
  already match and the gate skips them.
- **Lyrics become counted work units.** They are invisible to the progress bar
  today, which is part of why the tail of a run drifts from its estimate
  (`the-sync-bar-counts-work-not-bytes.md`). `units_total` rises. The bar gets
  more honest, not less.
- **Removal stays as it is.** `is_removable_managed_path` (`mirror.rs:537-546`)
  deliberately exempts `.lrc` so a user's own lyrics are never deleted. Do not
  touch it.

### Verification

- `mirror_tests.rs`: mirror the four analysis-gate tests (`:94`, `:115`, `:137`,
  `:146`) for lyrics — a resident `.lrc` of the expected size plans no write; a
  different size plans exactly its rewrite; a track with no local `.lrc` plans
  nothing; a `.lrc` arriving with its audio is planned once and is never an
  orphan removal.
- A `FakeBackend` test: a second sync with unchanged lyrics issues **zero**
  `replace_track` calls for `.lrc` paths. That is the point of the change and
  needs a test that fails before it.
- A test that a `.lrc` changed alone, with no audio change, plans exactly one
  write — the new behaviour, which nothing today would catch.

---

## Change 3 — transcode ahead of the transfer, depth 3

Ordered last: it is the only change with a concurrency failure mode, and putting
it on a base whose per-file cost is already halved keeps its own measurement
unconfounded.

### The defect

`run_planned_sync` (`device_sync_planned.rs:164-197`) pops one effect, awaits it,
dispatches the event, pops the next. `Effect::Transcode { index }` is awaited
inline (`device_sync_effects.rs:115-157`), stores its result in
`work.transcoded`, and only then does the machine emit `Effect::CopyTrack` for
the same index (`machine.rs:328`, `:538`).

A 1.7 s encode and a 0.6 s device write never overlap, on an 8-core machine, for
a library that is 80 % FLAC.

### The change

Prefetch **outside the machine**, so the machine remains the single authority on
phase and progress — the property `the-sync-bar-counts-work-not-bytes.md` was
written to restore and which must not be handed back.

1. `PlannedWork` gains `transcode_ahead: HashMap<usize, PendingTranscode>`, where
   `PendingTranscode` holds the in-flight handle, its cancellation flag and the
   staged path.
2. In the effect loop, after dispatching an effect for transfer index `N`, look
   ahead in `plan.transfers` for the next up-to-`TRANSCODE_AHEAD` indices whose
   action is `TranscodeOpus160`/`TranscodeMp3` and that are not already pending;
   start each via `backend.transcode_track` (`device_sync_backend.rs:130-153`)
   without awaiting.
3. When `Effect::Transcode { index }` is performed, take the pending entry for
   `index` if present and await that instead of starting a fresh encode. A miss
   falls back to today's inline path, so correctness never depends on the
   prefetch having happened.
4. `const TRANSCODE_AHEAD: usize = 3;` — named, with the sizing argument in a
   comment: encode throughput 1.70 / 3 = 0.57 s/track against the device's
   0.62 s/track, so the device stays the bottleneck. Three of eight cores.

### The cleanup obligations — the whole risk of this change

- On cancel, and on **every** early return from `run_planned_sync` (including the
  `!is_current_run` bail at `device_sync_planned.rs:180`), each pending transcode
  is cancelled via its `Arc<AtomicBool>` **and** its staged file passed to
  `staging::discard`. A prefetched encode whose track is never reached must not
  leak into `~/.cache/reprise/device-sync/`.
- `work.transcoded` is a single slot today (`device_sync_effects.rs:167`). It
  must become per-index, or be filled from the prefetch map at the moment the
  copy runs. A stale slot handing the wrong file to `CopyTrack` is the failure
  mode to test for — it would put one track's audio under another track's name.
- A prefetched transcode that *fails* must produce the same
  `Event::Transcoded(Err(..))` at the same point in the sequence as an inline
  failure, so `one-bad-file-no-longer-stops-the-sync` behaviour is unchanged.

### Verification

- `FakeBackend` test with slow transcodes and instant copies: the encode for
  track N+1 starts before the copy for track N completes. Without this the change
  is unobservable and can silently regress.
- Cancellation with prefetches outstanding leaves the staging directory empty.
- A prefetched failure produces an event sequence identical to an inline failure.
- No track is ever copied from another track's staged file (assert the staged
  path matches the index being copied).

---

## Proving it

The micro-benchmarks justify the changes; they do not prove them. The number
that counts is the wall clock of a real sync.

**The arm** — `"Recently played"` (smart, 50 entries, 325.1 MiB) is currently
deselected on this device, which makes it a bounded, repeatable body of real work:
selecting it copies ~50 tracks with transcode, lyrics and analysis; deselecting it
afterwards removes them again.

1. **Control arm**, built from the branch point, not from whatever binary happens
   to be installed: select the playlist, sync, record wall clock, `units_total`
   and the actual track count; deselect and sync again to clean up.
2. **Fix arm**: identical, from the branch.
3. Compare **seconds per track**, normalised, because a smart playlist's
   membership can shift between the two runs.
4. **Never use `bytes_per_second`** as evidence — it is known to freeze
   (`the-sync-bar-counts-work-not-bytes.md`). Wall clock and unit counts only.
5. Also record a steady-state run (no audio work) in both arms: that is where the
   ~8 s walk saving and the lyrics gate show up on their own.

Expect fresh FLAC copies to fall from ~4.7 s/track towards ~0.6–0.7 s/track, and
a steady-state re-sync to drop by ~8 s plus ~0.93 s for every resident track that
would have had its lyrics rewritten. If the fix arm does not beat the control arm
by a wide margin, that is the finding — do not report the micro-benchmarks as the
result.

## Risks worth naming

- **A parallel session is in the same subsystem.** The wake-lock list shows
  `pipeline-sync-deleting` running for `the-sync-says-what-it-is-deleting`, which
  touches the device-sync removal path. Check `origin/dev` before starting and
  rebase rather than discovering the conflict at landing time.
- **`FakeBackend` shapes what is testable.** Several verifications above need it
  to record enumeration counts and transcode start times, which it does not do
  today (`device_sync_fake_backend.rs` records copy order, managed copies, reads,
  deletes, storage ids). Extending it is part of the work, not a surprise.
- **`ManagedWalk` is a signature change that ripples.** `inspect()`
  (`device_sync_inspection.rs:14`) and `DeviceStorageInspection` are read by the
  device page and the MCP sync-state surface. Keep `managed_files` byte-identical
  in content so nothing downstream shifts.

---

## Parallelität

**One strand.** The cut was attempted and does not survive the file list:

| File | Ch. 4 | Ch. 2 | Ch. 1 | Ch. 3 |
|---|---|---|---|---|
| `reprise-platform-linux/.../device_sync_inspection.rs` | ✓ | | | |
| `reprise-platform-linux/.../device_sync.rs` | ✓ | ✓ | | |
| `reprise-core/.../mirror.rs` | ✓ | | ✓ | |
| `reprise-core/.../machine.rs` | ✓ | | ✓ | |
| `reprise-gnome/.../device_sync_effects.rs` | ✓ | | ✓ | ✓ |
| `reprise-gnome/.../device_sync_types.rs` | ✓ | | ✓ | ✓ |
| `reprise-gnome/.../device_sync_backend.rs` | ✓ | | | ✓ |
| `reprise-gnome/.../device_sync_planned.rs` | | | | ✓ |
| `device_sync_fake_backend.rs` | ✓ | | ✓ | ✓ |
| `device_sync_tests.rs` | ✓ | ✓ | | |

Every candidate pairing shares a file, and not incidentally:

- **1 + 4** share the walk itself. Change 1's gate reads the `lyrics_files` list
  that change 4 creates; splitting them means one strand consumes a type the
  other has not written yet.
- **2 + 4** collide in `device_sync.rs` on `cleanup_partials_in` and
  `PARTIAL_SUFFIX` — change 2 keeps the sweep, change 4 reshapes it.
- **1 + 3** collide in the `Effect::CopyTrack` arm: change 1 removes the lyrics
  call from it while change 3 rewrites the transcode/copy sequencing around it.
- **3 + 4** collide in `device_sync_backend.rs` and `device_sync_types.rs`.
- All four extend `device_sync_fake_backend.rs` for their own verification.

A platform/planner split (A = `reprise-platform-linux`, B = the rest) looks clean
and is not: B cannot compile until A has landed `ManagedWalk`. That is a hard
dependency, not parallelism.

**Task order within the strand**, one commit each so a bisect shows which change
moved the number:

1. **Change 4** — one walk, three lists. Base for 1 and 2.
2. **Change 2** — `.part` off the track path. Largest measured win.
3. **Change 1** — lyrics gate and its own effect.
4. **Change 3** — transcode prefetch, depth 3.

No post-merge cross-checks: with a single strand every verification reads files
the strand owns.
