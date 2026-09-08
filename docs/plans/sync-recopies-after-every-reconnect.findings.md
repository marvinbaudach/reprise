---
slug: sync-recopies-after-every-reconnect
phase: findings
created: 2026-09-03
---
# Every new connection re-copies a few hundred MiB that are already on the phone

Reported: "es wird jedesmal gesynct", "immer wenn ne neue Verbindung aufgebaut
wird warte ich hier 10 min", "die Daten haben sich nie geändert".

## What the run history says

`sync_runs`, live DB, Pixel 10 Pro XL (`59100DLCQ006SB`):

| run | started | outcome | planned | copied | MiB |
|---|---|---|---|---|---|
| 119 | 10:46 | completed | 0 | 0 | 0 |
| 120 | 10:47 | completed | 0 | 0 | 0 |
| 121 | 10:59 | completed | 0 | 0 | 0 |
| 122 | 12:45 | cancelled | 154 | 154 | 465 |
| 123 | 12:59 | completed | 112 | 112 | 352 |

Three runs in a row plan **0** on one connection. After the reconnect the same
unchanged library suddenly plans 154, then 112. That shape is the whole case:
whatever makes matching work is per-connection state, not per-library state.

## Which planner branch fires

`crates/reprise-core/src/device_sync/mirror_file_changes.rs:33-59`. Only four
branches can produce a copy. Three are ruled out by measurement:

- **`None` (no ledger row)** — the selection resolves to 769 unique tracks
  (playlist 1 + `rating >= 4` + 50 most recently played); 751 of them have a
  `device_files` row. **18 missing, not 112.**
- **`replace` (ledger row disagrees)** — the page reported `0 updated`, and
  `replacements` is fed straight from `plan.replace` (`page.rs:206-224`).
- **unsafe managed path** — would raise a warning; paths are ordinary.

What is left is the residency guard:

```rust
Some(existing)
    if inventory_matches(existing, file)
        && managed_files_scanned
        && !managed_paths.contains(&existing.device_path.to_lowercase()) =>
{
    plan.copy.push(file.clone());
}
```

Introduced by `ee80faf579` — *"The inventory is a memory, not a proof."* The
ledger agrees on every field, but the device walk does not list the path, so the
track is re-copied as **new**.

## The files were on the phone the whole time

`device_settings.size_on_device` is written from the walk itself —
`verified_track_bytes(&device.managed_files)`
(`device_sync_compact.rs:323`, called at
`device_sync_runtime_refresh.rs:128-135`), i.e. the summed size of every audio
file the walk found.

| verified at | size_on_device |
|---|---|
| 2026-09-03 10:59:37 | 4 839 941 168 B |
| 2026-09-03 13:03:37 | 4 839 941 046 B |

Run 123 wrote **352 MiB of "new" files** between those two walks and the walked
audio total moved by **−122 bytes**. Freshly added files would have grown it by
352 MiB. They did not — so those 112 files were already counted at 10:59 and are
on the device. **The walk that planned run 123 under-reported them.**

## Why a reconnect is the trigger

`managed_files` is per-connection in-memory state, never persisted
(`device_sync_runtime.rs:42-103`):

- connect → `refresh_contents_on_connect` walks **immediately**
  (`device_sync_device_list.rs:201`, `device_sync_runtime_refresh.rs:18-19`)
- success → `ever_inspected = true`, `scan_error = None`, so
  `managed_files_scanned = true` (`device_sync_compact.rs:152`)
- disconnect → `managed_files.clear()`, `ever_inspected = false`
  (`device_sync_device_list.rs:62-83`)

Nothing re-walks in between. **The user's 10-minute wait re-walks nothing** —
the plan still rests on the listing taken in the first seconds after connect,
while the phone is still settling. Only "Check again" or a post-sync verify
takes a new listing.

## The defect

`inspect_target_folder` (`device_sync_inspection.rs:96-144`) cannot tell "this
directory is empty" from "this directory enumerated nothing because the device
was not ready": an empty `next_files` batch ends the loop, the walk returns
`Ok`, and a partial listing is promoted to proof-of-absence for the whole
session. A walk that silently returns a subset must not set
`managed_files_scanned = true`.

## Measured on the phone (device-lock held, 2026-09-03 13:2x)

Ground truth via `adb`, i.e. a channel that never touches gvfs/libmtp:

| | |
|---|---|
| `find /sdcard/Music/Reprise -type f` | **2234** files |
| of those `.opus` / `.mp3` | **628 / 157 = 785 audio** |
| summed audio bytes | **4 839 941 010 B** |

The ledger holds exactly **785** rows for this serial, exactly 628 opus + 157
mp3, and the two recorded walk totals are 4 839 941 168 B (10:59) and
4 839 941 046 B (13:03) — all three agree to within ~160 bytes of 4.84 GB.
**Nothing was missing and nothing changed.** The 112 files run 123 re-copied
were on the phone the whole time.

A Gio `mtp://` walk of the same tree, run five times:

```
dirs=323 files=2234 empty_dirs=37 secs=8.7 … 10.4
```

byte-identical to the `adb` listing (`diff` = 0 lines) and identical across all
five runs. The walk code is correct; a warm, idle device is walked completely
and reproducibly. (2234 − 658 `.lrc` = **1576** — the "1576 tracks" the page
shows, so `managed_files` really does carry all of them.)

And the decisive comparison: every one of the 785 ledger `device_path` values is
present in that walk, case-folded and NFC-normalised — **0 missing**. Against a
complete walk the residency guard cannot fire at all. The re-copies can only
come from a walk that returned fewer files than the device holds.

## What the run history says about *which* walk

Gaps between runs do not predict a big plan — runs 99–102 sat behind gaps of
377, 52, 227 and 239 minutes and all planned 0. The plans flip inside a single
connection instead:

| run | time | gap | planned |
|---|---|---|---|
| 107 | 19:52:51 | 0 min | 0 |
| 108 | 19:54:42 | 1 min | **70** |
| 110 | 20:11:18 | 1 min | 0 |
| 111 | 20:18:24 | 6 min | **140** |
| 113 | 20:46:20 | 0 min | 81 |
| 114 | 20:55:51 | 1 min | **140** |

Two minutes apart, same connection, same library: 0 then 70. So it is not the
connect walk specifically — **any** walk can come back short, and the counts
(70, 140 = 2×70, 81, 112, 154) look like whole album folders dropping out at
once rather than scattered files.

## Root cause

`inspect_target_folder` (`device_sync_inspection.rs:96-144`) cannot tell "this
directory is empty" from "this directory returned nothing this time": an empty
`next_files` batch simply ends the loop. A hard error would propagate and set
`scan_error`, which *disables* the guard — so the only way to get a wrong plan
is a walk that silently comes back short while still returning `Ok`. That walk
sets `ever_inspected = true`, `managed_files_scanned = true`, and
`mirror_file_changes.rs:36-40` then treats "not in the walk" as proof the file
is gone.

The inventory is a memory, not a proof — but the walk is not a proof either, and
nothing checks it before it is used as one.

## Tried and failed to reproduce the short walk

Ten walks with the *same* API the app uses — `enumerate_children_async` +
`next_files_async(64)`, breaking on the empty batch exactly as
`inspect_target_folder` does, same `ENUMERATE_ATTRIBUTES`
(`device_sync.rs:31-32`):

```
run1..run10  dirs=323 files=2234 empty_dirs=37 short_batches=286  9.0-10.3 s
```

10/10 identical, 0 enumeration errors, and each set-identical to the `adb`
listing. Three walks in parallel only slow it down (29.5 s each) — still
2234/2234, no truncation. (The first probe used `next_file`, a different gvfs
code path; the batched runs above are the ones that count.)

Ruled out along the way:

- **Ledger pruning by the walk** — `delete_device_file` has exactly one caller,
  `device_sync_effects.rs:403`, on a planned removal. Nothing deletes rows
  because a walk came back short.
- **Stale or racing walk results** — `refresh_contents_with_delta`
  (`device_sync_runtime_refresh.rs:34-100`) bumps `scan_generation` per request
  and drops any result whose generation no longer matches. A superseded walk
  cannot land.
- **Truncation in transport** — `GioDeviceBackend::inspect`
  (`device_sync_backend.rs:39-51`) calls `DeviceStorage::inspect` in-process.
  There is no IPC to truncate the 1576-entry list.
- **A hard I/O error** — it propagates, sets `scan_error`, and *disables* the
  guard (`managed_files_scanned = false`), which yields `planned 0`, not 112.

So the short listing is real but only appears under a condition an idle, warm
device does not offer — the failures cluster right after a sync has written into
the tree, and reaching that state costs a live reconnect plus a ~350 MiB
transfer (`device_settings.sync_automatically = 1`, so a reconnect starts one by
itself).

## Fix

The guard asks the walk for proof and accepts silence as proof. Two changes:

1. **Prove absence per file, not per walk.** When the ledger matches but the
   path is missing from the walk, confirm that one path with a direct
   `query_info` before copying. ~6 ms per doubtful file, against 352 MiB of
   re-transfer for 112 of them. This keeps the behaviour #338 wanted — a file
   the user deleted on the phone really is re-copied — while a walk that came
   back short no longer costs a gigabyte. Needs the check in the runtime layer:
   `plan_mirror` is pure and has no device access, so the doubtful entries have
   to be marked in the plan and verified by the effect layer before transfer.
2. **Do not let a short walk silently feed the rest of the page.**
   `managed_files` also writes `device_settings.size_on_device`
   (`verified_track_bytes`) and the "1576 tracks" figure. A walk that finds far
   fewer managed files than the ledger holds rows should be reported, not
   quietly believed.

## Still open

Reproducing the short walk in isolation. Everything above is consistent with it
and nothing else survives, but it has not been caught in the act.
