# "1 synchronization items failed" — findings (2026-09-02)

## What the UI showed

`1 synchronization items failed` on the Pixel 10 Pro XL sync page
(`device_sync_planned.rs:303` builds that string from `failed_tracks.len()`).

## What actually failed

Live DB, `sync_events` for run 103:

```
run_id: 103
kind: failed
track_id: 168
device_path: Asking Alexandria/Stand Up and Scream/13 When Everyday's the Weekend.mp3
detail: copy failed: creating the destination directory failed:
        device I/O failed: libmtp error:  Could not send object info.
```

Journal, same moment:

```
19:40:22 WARN …runtime::planned::effects: device transfer failed track_id=168
  error=creating the destination directory failed: device I/O failed:
        libmtp error:  Could not send object info.
```

## Where it comes from

`crates/reprise-platform-linux/src/device_sync.rs` — `ensure_managed_directories`:

```rust
match current.make_directory_future(...).await {
    Ok(()) => {}
    Err(error) if error.matches(gio::IOErrorEnum::Exists) => {}
    Err(error) => return Err(error.into()),
}
```

Only `G_IO_ERROR_EXISTS` is tolerated. gvfs/libmtp on Android does **not**
reliably map "this folder is already there / the device is busy" to `EXISTS`;
it surfaces libmtp's `Could not send object info`, which lands in the generic
arm and kills the whole track (`WriteStep::CreateDirectories`).

## Confirmed cause 1 — a case-only collision with a folder the phone already has

Every directory the phone reports having (`kind='skipped'`, "phone listen report
path could not be resolved" — these are device-side paths) is a case-only
variant of a directory Reprise failed to create:

| Reprise tried to create | phone already has |
|---|---|
| `Emmure/Speaker of the Dead/` | `Emmure/Speaker Of The Dead/` |
| `Emmure/Slave to the Game/` | `Emmure/Slave To The Game/` |
| `Lorna Shore/I Feel the Everblack Festering Within Me/` | `Lorna Shore/I Feel The Everblack Festering Within Me/` |
| `Bring Me the Horizon/Count Your Blessings _ Repented/` | `Bring Me The Horizon/Count Your Blessings _ Repented/` |

4 of 4. Android's emulated storage is case-insensitive, so `make_directory`
on the lowercase spelling hits the resident folder and libmtp answers
`Could not send object info` instead of `G_IO_ERROR_EXISTS`. That is not a
hiccup — it is why runs 87 and 88 failed the *identical* twelve tracks.

Worse, the device scan reports **both** spellings as separate MTP folders
(run 81's `deleted` events list `Emmure/Speaker Of The Dead/02 …` next to
`Emmure/Speaker of the Dead/05 …`). `device_case::build_directory_spellings`
resolves a folded key by majority vote and falls to `Ambiguous` on a tie, so
adoption cannot reliably settle which spelling to use — while on the
filesystem underneath they are one and the same folder.

## Confirmed cause 2 — genuine MTP flakiness at other steps

The same libmtp message also appears at `WriteStep::Publish` (run 82, track
523) and `WriteStep::CopyPartial` (run 83, same track), so the device does
fail sporadically on its own. Those heal on the next run.

## Failures do heal on a later run

| run | when | outcome | copied | failed |
|-----|------|---------|--------|--------|
| 87  | 08-31 20:49 | failed | 215 | 12 |
| 88  | 08-31 21:25 | failed | 151 | 12 (same 12 tracks) |
| 90  | 09-01 09:53 | failed | 99  | 5 (next track in 5 of the same folders) |
| 91  | 09-01 10:05 | completed | 8 | 0 (exactly the 5 + rest) |
| 97  | 09-02 04:26 | failed | 68 | 1 (track 168) |
| 103 | 09-02 19:39 | failed | 69 | 1 (track 168 again) |
| 104 | 09-02 19:50 | cancelled | 1 | 0 (**track 168 copied fine**) |

The same destination folder that failed at 19:40 was created without complaint
at 19:50. Every failure has healed on a later run — run 91 copied exactly what
run 90 lost. So the sync is not permanently stuck; it loses one file per
affected folder per run and re-tries it the next time.

Runs 87/88 closed with `could not copy analysis sidecar: creating the
destination directory failed: …` — the sidecar writes go through the same
`ensure_managed_directories` and hit the same wall.

## Proposed fix

In `ensure_managed_directories`, a non-`Exists` failure must not be final:

1. `enumerate_children` on the parent and look for a **fold-equal** entry. If
   one is there, adopt that spelling for the rest of the path and carry on.
   This is the fix for cause 1; a plain `query_info` on the desired name would
   miss it, because the desired name is exactly the one that is not there.
2. Only when nothing fold-equal exists, retry the creation once with a short
   backoff. This is the fix for cause 2.

Reuse `device_case::fold_path` rather than re-implementing the fold — it
already covers the curly-quote axis (relevant for track 168's `’`), and two
layers folding differently would be its own bug. It is `pub(super)` in
`reprise-core`; the fix site is in `reprise-platform-linux`, which already
depends on `reprise-core`, so it needs widening to `pub`.

Follow-up: `finish_sync` marks the whole run `failed` for a single lost track;
see the existing `one-bad-file-no-longer-stops-the-sync` plans.

## Confirmed code gap — a case-drifted *ancestor* is never adopted

`device_case::build_directory_spellings` (`device_case.rs:95-133`) records only
the **full parent directory** of each file Reprise knows about. Nothing records
intermediate ancestors. So when the album folder does not exist on the device
yet, its folded key is absent, `adopt_resident_spelling` returns `Keep`, and a
case-drifted *artist* folder above it is never adopted either. Readable from
source; no device needed.

## Blocked

`device-lock status` → `HELD by showreel-sync … "showreel handover shot: sync
now, progress bar on camera"`. No device access from this session, so no
listing of `/Music/Reprise/Asking Alexandria/` and no repro loop.

## Second report: sync restarts by itself

`sync_runs` 106 (19:52:03) and 107 (19:52:51) — both `completed`, 0 copied,
0 deleted — with **no** `device sync started from page` line in the journal.
Starters that log nothing on success:

- `device_sync_agent.rs:90` — MCP `music_device_sync` `Start`. **This one logs
  nothing**, while `device_sync_page_actions.rs:43` logs `device sync started
  from page`. That asymmetry is exactly why runs 106/107 cannot be attributed;
  a two-line `tracing::info!` there would make the re-check trivial.
- the resume path (`device_sync_runtime_refresh.rs:240`)
- auto-start (`device_sync_runtime_refresh.rs:266`), which needs
  `just_connected` **and** `balance.has_work()` — neither holds for an idle
  device that only got scrolled

The parallel `showreel-sync` session holds the device lock precisely to press
"Sync now" for a camera shot. Runs 106/107 are most likely that session, not a
bug — this needs re-checking once the lock is free.

## Implemented (2026-09-02)

`ensure_managed_directories` now creates each component through
`ensure_directory` and returns the spelling the device actually has, and
`replace_managed`/`replace_playlist` build the file path from those components
instead of the desired ones — gvfs matches MTP folder names exactly, so
adopting a directory without rebuilding the path underneath it would only move
the failure one step later.

On a non-`Exists` failure `ensure_directory`:

1. enumerates the parent and adopts a fold-equal directory (exact name first;
   two fold-equal spellings side by side are left alone, the same refusal
   `device_case` makes) — cause 1, with a `tracing::warn!` on every adoption;
2. otherwise retries once after 250 ms and, if that fails too, propagates the
   **first** error — cause 2.

A listing that cannot be read (a remote enumerator disappearing between
batches is a real MTP failure mode) looks exactly like "nothing folds equal",
so that path warns too — otherwise a track would fail with the original error
and no trace of why adoption never happened.

`device_case::fold_path` widened to `pub` and re-exported from
`reprise_core::device_sync`, so both layers fold identically.

Tests (`device_sync_tests.rs`) use a `0o500` parent as the stand-in for the
generic MTP failure — enumerating an `r-x` directory still works, so the
adoption runs end to end on a case-sensitive filesystem. Control arm: the two
adoption tests fail without the fix, the ambiguity test is a guard and passes
either way.

## Follow-up: the ledger still records the *planned* path

`Effect::RecordFile` writes `entry.device_path` (the plan) into
`device_files`, and the backend returns only `CopyOutcome` — so a file that
landed in an adopted spelling is recorded under the desired one. Consequences,
all of which predate this change and are now merely reached more often:

- `delete_managed` takes the ledger path, so removing such a track hits
  `NotFound`, returns `Ok(false)` and leaves an orphan on the device.
- `build_directory_spellings` chains ledger paths *and* the device scan, so the
  two spellings can tie and turn into `DirectorySpelling::Ambiguous`.

Fixing it means threading the actual relative path back through
`replace_managed` → the `replace_track` backend trait → the machine → the
ledger (both backends, ~12 test call sites, and the machine's plan state). Left
out of this pass deliberately: it changes which `device_path` the planner emits
and therefore the delta, and the device is locked, so it cannot be verified.

The `build_directory_spellings` ancestor gap above is part of the same
follow-up — the runtime enumerate now covers drifted ancestors generically,
because every component is checked against the device when its creation fails.

## Measured on the device, 2026-09-02 23:1x

Phone attached, `device-lock` held, target `/Music/Reprise`. 2158 files listed
via `adb shell find`, 785 ledger rows read from `device_files` for serial
`59100DLCQ006SB`.

### The premise holds: gvfs matches MTP names exactly

The whole adoption design rests on it, so it was measured rather than assumed:

```
$ gio info "mtp://…/Music/Reprise/Emmure/Speaker Of The Dead"
display name: Speaker Of The Dead
type: directory

$ gio info "mtp://…/Music/Reprise/Emmure/Speaker of the Dead"
gio: …/Emmure/Speaker%20of%20the%20Dead: File not found

$ gio info "mtp://…/Music/Reprise/Emmure/Speaker of the Dead/02 Area 64-66.opus"
gio: …/02%20Area%2064-66.opus: File not found
```

The second path is a **live ledger row**. gvfs does not fold case; the resident
folder is invisible under the spelling the ledger recorded.

### The orphan bug is live, not theoretical — 77 rows

77 of the 785 ledger rows, spread over **11 distinct directories**, name a path
that `gio` cannot resolve:

| ledger records | device has |
|---|---|
| `Emmure/Speaker of the Dead` | `Emmure/Speaker Of The Dead` |
| `Emmure/Slave to the Game` | `Emmure/Slave To The Game` |
| `Emmure/Goodbye to the Gallows` | `Emmure/Goodbye To The Gallows` |
| `Asking Alexandria/Stand Up and Scream` | `Asking Alexandria/Stand Up And Scream` |
| `Chelsea Grin/Desolation of Eden` | `Chelsea Grin/Desolation Of Eden` |
| `Lorna Shore/I Feel the Everblack Festering Within Me` | `Lorna Shore/I Feel The Everblack Festering Within Me` |
| `Carnifex/Graveside Confessions` | `Carnifex/GRAVESIDE CONFESSIONS` |
| `Immortal Disfigurement/King` | `Immortal Disfigurement/KING` |
| `Fight the Fade/Isolationist` | `Fight The Fade/Isolationist` |
| `Fight the Fade/APOPHYSITIS (deluxe edition)` | `Fight The Fade/APOPHYSITIS (Deluxe Edition)` |
| `Bring Me the Horizon/Count Your Blessings _ Repented` | `Bring Me The Horizon/Count Your Blessings _ Repented` |

`delete_managed` takes the ledger path, so deselecting any of those 77 tracks
returns `Ok(false)` and leaves the file on the phone. Every folded key resolves
to exactly one device directory (0 of 785 rows point at a folder that is absent
in every spelling), so the adoption is unambiguous in all 11 cases.

### Run 81's two spellings were an MTP artifact, not two folders

**0 of 2158 device files** live in a directory that exists under two spellings
side by side. The `deleted` events of run 81 listed
`Emmure/Speaker Of The Dead/02 …` beside `Emmure/Speaker of the Dead/05 …`,
which was read as two resident folders; on the filesystem there is one. This
matters because `DirectorySpelling::Ambiguous` and the majority vote in
`build_directory_spellings` were designed against the belief that they were
real.

### The drift causes no re-planning — and that is the problem

Run 115 planned **0**. `compute_delta` (`delta.rs:52-58`) calls a track
unchanged when five fields match, `device_path` among them — and the candidate
carries the same drifted spelling the ledger does, because ledger and device
scan tie in `build_directory_spellings` and the vote falls to `Ambiguous`, so
nothing is adopted at planning time. Both sides are consistently wrong, so the
delta is empty.

The consequence for the fix in
`the-sync-records-the-folder-it-used.md`: **it is forward-only.** It corrects
the path for files that get copied from now on, and these 77 are never copied
again precisely because nothing plans them. No code path rewrites
`device_files.device_path` from a device scan — `upsert_device_file`
(`settings.rs:462`) is reached only from `Effect::RecordFile`, i.e. after a copy.

### Follow-up worth its own plan

Break the tie in `build_directory_spellings` in favour of the device scan
instead of counting ledger and scan equally. The device is ground truth; the
majority vote treats a stale ledger as an equal witness. Weighting the scan
higher makes the 77 rows adopt the resident spelling at planning time, which is
the only route that heals them without re-copying 77 files over MTP at ~2.79 s
per unit.

Not attempted here, and deliberately not folded into the current branch.

## Device confirmation (2026-09-03, Pixel 10 Pro XL over MTP)

`examples/probe_case_adoption.rs` builds the collision on the device and asks
for the fold-equal spelling. Each run uses a fresh folder pair, because a
leftover from an earlier run makes `make_directory` answer `EXISTS` and the
probe would prove nothing. Everything it writes lives under
`/Music/Reprise Probe` and is removed again; the music library is never
touched.

Control arm — `origin/dev` code, pair `Alpha Beta C1` / `alpha beta C1`:

```
seed OK: Copied
PROBE ERR: creating the destination directory failed: device I/O failed:
           libmtp error:  Could not send object info.
```

That is the field error verbatim, reproduced on demand.

Fix arm — same commit as this branch, pair `Alpha Beta F3` / `alpha beta F3`:

```
WARN device sync: creating this directory failed, but a fold-equal one is
     already on the device desired=alpha beta F3 resident=alpha beta F3
     first_error=libmtp error:  Could not send object info.
PROBE OK: Copied
landed at alpha beta F3/probe.bin: yes (4096 bytes)
```

So the first `make_directory` fails exactly as before, the listing rescue finds
a usable directory, and the copy lands. What the run also shows: the device
reports the folder under the **attempted** spelling as well, so the rescue hits
its exact-name branch rather than a differently-spelled resident one. That
branch used to stay silent — it now logs like every other adoption, because a
directory the device errors on while demonstrably having it is precisely what
this rescue is for.

The retry-after-backoff path never fired in these runs; it stays in for cause 2,
which the earlier `Publish`/`CopyPartial` failures document.
## Follow-up implemented (2026-09-02)

The copy result now carries the relative path that the platform actually
wrote. That path travels through the sync machine into `device_files`, and the
replacement cleanup compares the previous ledger path with the recorded path
rather than with the plan. An adopted spelling therefore cannot make a run
delete the file it just wrote.

The ancestor concern needs no additional planning state. The production
planning regression reaches `adopt_resident_spelling`, which checks the track's
own inventory path before consulting `directory_spellings`. When that recorded
path folds equal to the freshly computed desired path, the planner adopts it
immediately. `plan_file_changes` then compares the adopted path with the same
inventory row and plans no transfer. The runtime directory walk continues to
handle a missing album below a drifted artist directory.

The agent command bridge now logs successful Start and Cancel commands with the
resolved device id. Future sync runs initiated through the MCP surface can
therefore be distinguished from page starts in the journal.

Analysis and lyrics sidecars and the track metadata list do not own separate
`device_files` rows. Their copy outcomes remain intentionally unrecorded; the
audio file's ledger row is the single inventory record for the track.

The on-device arm remains unverified because no phone was attached for this
follow-up.
## Measured on the device, 2026-09-02 23:15

Phone attached, `device-lock` held, target `/Music/Reprise`. 2158 files listed
via `adb shell find`, 785 ledger rows read from `device_files` for serial
`59100DLCQ006SB`.

### The premise holds: gvfs matches MTP names exactly

The whole adoption design rests on it, so it was measured rather than assumed:

```
$ gio info "mtp://…/Music/Reprise/Emmure/Speaker Of The Dead"
display name: Speaker Of The Dead
type: directory

$ gio info "mtp://…/Music/Reprise/Emmure/Speaker of the Dead"
gio: …/Emmure/Speaker%20of%20the%20Dead: File not found

$ gio info "mtp://…/Music/Reprise/Emmure/Speaker of the Dead/02 Area 64-66.opus"
gio: …/02%20Area%2064-66.opus: File not found
```

The second path is a **live ledger row**. gvfs does not fold case; the resident
folder is invisible under the spelling the ledger recorded.

### The orphan bug is live, not theoretical — 77 rows

77 of the 785 ledger rows, spread over **11 distinct directories**, name a path
that `gio` cannot resolve:

| ledger records | device has |
|---|---|
| `Emmure/Speaker of the Dead` | `Emmure/Speaker Of The Dead` |
| `Emmure/Slave to the Game` | `Emmure/Slave To The Game` |
| `Emmure/Goodbye to the Gallows` | `Emmure/Goodbye To The Gallows` |
| `Asking Alexandria/Stand Up and Scream` | `Asking Alexandria/Stand Up And Scream` |
| `Chelsea Grin/Desolation of Eden` | `Chelsea Grin/Desolation Of Eden` |
| `Lorna Shore/I Feel the Everblack Festering Within Me` | `Lorna Shore/I Feel The Everblack Festering Within Me` |
| `Carnifex/Graveside Confessions` | `Carnifex/GRAVESIDE CONFESSIONS` |
| `Immortal Disfigurement/King` | `Immortal Disfigurement/KING` |
| `Fight the Fade/Isolationist` | `Fight The Fade/Isolationist` |
| `Fight the Fade/APOPHYSITIS (deluxe edition)` | `Fight The Fade/APOPHYSITIS (Deluxe Edition)` |
| `Bring Me the Horizon/Count Your Blessings _ Repented` | `Bring Me The Horizon/Count Your Blessings _ Repented` |

`delete_managed` takes the ledger path, so deselecting any of those 77 tracks
returns `Ok(false)` and leaves the file on the phone. Every folded key resolves
to exactly one device directory (0 of 785 rows point at a folder that is absent
in every spelling), so the adoption is unambiguous in all 11 cases.

### Run 81's two spellings were an MTP artifact, not two folders

**0 of 2158 device files** live in a directory that exists under two spellings
side by side. The `deleted` events of run 81 listed
`Emmure/Speaker Of The Dead/02 …` beside `Emmure/Speaker of the Dead/05 …`,
which was read as two resident folders; on the filesystem there is one. This
matters because `DirectorySpelling::Ambiguous` and the majority vote in
`build_directory_spellings` were designed against the belief that they were
real.

### The drift causes no re-planning — and that is the problem

Run 115 planned **0**. `compute_delta` (`delta.rs:52-58`) calls a track
unchanged when five fields match, `device_path` among them. Production planning
gets that matching path earlier: `adopt_resident_spelling`
(`device_case.rs:30-32`) checks `own_inventory_path` first, and the stale ledger
path folds equal to the newly computed desired path because both came from the
same canonical `device_track_path` naming. The planner therefore adopts the
ledger spelling immediately and never consults `directory_spellings`.
`plan_file_changes` (`mirror.rs:288-313`) then compares that adopted path with
the identical inventory value and plans nothing. Both sides are consistently
wrong, so the delta is empty.

The consequence for the fix in
`the-sync-records-the-folder-it-used.md`: **it is forward-only.** It corrects
the path for files that get copied from now on, and these 77 are never copied
again precisely because nothing plans them. No code path rewrites
`device_files.device_path` from a device scan — `upsert_device_file`
(`settings.rs:462`) is reached only from `Effect::RecordFile`, i.e. after a copy.

### Follow-up worth its own plan

The healing route has to address the `own_inventory_path` short-circuit: when a
track's recorded path folds equal to the desired path but does not exist on the
device, the resident scan must be able to correct that stale spelling. Weighting
the device scan above the ledger in `build_directory_spellings` would not heal
the 77 rows, because the planner returns from the own-inventory check before the
vote is reached. The design belongs in a separate plan.

Not attempted here, and deliberately not folded into the current branch.

## Accepted review follow-ups (2026-09-03)

- The track metadata list now serializes paths from the recorded
  `DeviceFileRecord` rows, with the planned path as the fallback when a track
  has no row. This removes the contradiction between the ledger and the file
  content the device reads (finding 3).
- The caller's `gio::Cancellable` now reaches directory creation and enumeration,
  and cancellation is checked before the retry delay and before the second
  creation attempt (finding 4).
- Lossy UTF-8 conversion while folding device entry names was reviewed and
  deliberately left unchanged. It is a pre-existing pattern shared with
  `storage_root`, not part of this focused correction (finding 10).
