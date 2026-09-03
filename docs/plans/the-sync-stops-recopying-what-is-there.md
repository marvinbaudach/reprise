---
slug: the-sync-stops-recopying-what-is-there
worktree: /home/marvin/Projects/reprise-the-sync-stops-recopying-what-is-there
branch: feature/the-sync-stops-recopying-what-is-there
phase: reviewed
codex_session:
created: 2026-09-03
---
# The sync stops re-copying what is already there

## The bug in one paragraph

`plan_mirror` asks two sources whether a track is on the phone: the ledger
(`device_files`) and the device walk (`managed_files`). Since #338 the walk
wins — "the inventory is a memory, not a proof". But the walk has no way to
report that it came back short: an empty `next_files` batch simply ends the
loop in `inspect_target_folder`
(`crates/reprise-platform-linux/src/device_sync_inspection.rs:96-144`), the walk
returns `Ok`, `ever_inspected` becomes `true`, and `mirror_file_changes.rs:36-40`
then reads silence as proof the file is gone. Measured 2026-09-03: run 123
re-copied **112 files / 352 MiB** that `adb` shows byte-for-byte present on the
phone. Full evidence in
`docs/plans/sync-recopies-after-every-reconnect.findings.md`.

The walk is not a proof either. Nothing checks it before it is used as one.

## What this change does

**Repair the walk at its trust boundary, before anything consumes it.** After a
successful `inspect`, every ledger row whose `device_path` is absent from the
walk is a *doubtful* path — as is that track's derived analysis-sidecar path.
Ask the device about those paths directly, one `query_info` each, and fold the
ones that answer back into `managed_files`.

Repairing the list rather than second-guessing the copy decision matters because
`managed_files` feeds more than the plan: `verified_track_bytes` writes
`device_settings.size_on_device`, and the page's track count comes from the same
list. A fix that only patched `plan_file_changes` would leave those two reading
a short listing, and — see task 2 — would not stop the sidecar re-writes at all.

Cost when the walk is fine: zero, the doubtful set is empty. Cost when it is
short: ~6 ms per doubtful path (`device-sync-throughput-findings.md`), bounded by
the ledger size, against 352 MiB of re-transfer and ~0.5 s of ceremony per
sidecar.

What this deliberately does **not** change: a file the user really deleted on the
phone is still re-copied — the probe answers "not there" and the track stays in
`plan.copy`. That is the behaviour #338 wanted, and the reason "just trust the
ledger again" is the wrong fix.

## Tasks

### 1. A backend method that asks about single files

`crates/reprise-gnome/src/ui/device_sync/device_sync_types.rs` — add to
`DeviceBackend`, following the shape `read_managed_file` already uses (same
`storage_id` / `target_path` / relative-path convention, **not** a `SyncTarget`):

```rust
fn probe_managed_files(
    &self,
    _root_uri: String,
    _target_path: String,
    _storage_id: Option<StorageId>,
    _relative_paths: Vec<String>,
) -> BackendFuture<Vec<ManagedDeviceFile>> {
    Box::pin(async { Ok(Vec::new()) })
}
```

A **default body returning empty** is deliberate: "recovered nothing" is exactly
today's behaviour, so the other five `impl DeviceBackend` blocks
(`device_sync_smoke.rs`, `device_sync_compact_tests.rs`,
`device_sync_target_browser_tests.rs`, `sidebar_playlist_notification_tests.rs`,
and the fake backend) keep compiling untouched. Only
`device_sync_fake_backend.rs` overrides it.

The result carries one `ManagedDeviceFile` per path that is present; absent
paths are simply missing from the result, which is a normal answer and not an
error.

**"Present" means: the file exists *and* its size is greater than zero.** A
zero-byte entry is what an aborted MTP transfer leaves behind, and treating it
as resident would make that torso permanently unrepairable. The size written
into `ManagedDeviceFile` is the one the **device** reports, never the ledger's
`device_size` — otherwise `size_on_device` would state a number nobody measured.

The probe deliberately does *not* compare against `device_files.device_size`.
`inventory_matches` (`mirror.rs:613-619`) compares source path, source size,
source mtime, device path and profile fingerprint — the device-side size is not
part of the contract, and adding it here would re-copy files that a complete
walk waves through today.

### 2. The doubtful set includes analysis sidecars

`plan_analysis_sidecars` (`mirror.rs:396-440`) reads the same walk:

```rust
let existing_size_bytes = resident.get(device_path.as_str()).copied();
if existing_size_bytes == Some(size_bytes) { continue; }   // else rewrite
```

So recovering only the audio is not enough: the sidecar path is still missing
from the short listing, `existing_size_bytes` stays `None`, and every sidecar is
rewritten anyway. At ~0.5 s per object on this device that is minutes of
ceremony for zero useful bytes — the slow tail visible in the report.

Therefore the doubtful set is, per ledger row whose `device_path` is missing
from the walk:

- the audio path itself, and
- `analysis_sidecar::device_path_for_track(&device_path)` when it yields one.

Sidecars belonging to tracks the walk *did* find are not probed: they live in
the same directory the walk demonstrably read.

One caveat to carry into the code, not to fix here: the sidecar map is keyed
**case-sensitively** (`resident.get(device_path.as_str())`) while the audio guard
lowercases (`mirror_file_changes.rs:39`). Recovered entries enter the list with
the ledger's spelling, so the sidecar lookup only lands when ledger and device
agree on case. That is already true today and the repair does not make it worse.

### 3. The platform implementation

`crates/reprise-platform-linux/src/device_sync_read.rs` — next to `read_managed`,
whose structure it mirrors exactly:

```rust
pub async fn probe_managed(
    &self,
    storage_id: Option<StorageId>,
    target_path: &str,
    relative_paths: &[String],
) -> Result<Vec<ManagedDeviceFile>, DeviceIoError>
```

`resolve_target_storage(storage_id)` **once**, then per path
`safe_relative_components(relative_path)?` → `Self::managed_child(&storage,
target_path, &components)` → the existing `target_size(&file)`
(`device_sync.rs:603`). Do not write a second path splitter.

Per-path error handling:

- `NotFound`, or a size of `None`/`0` → not recovered. A valid answer.
- any other per-path error → `tracing::debug!` and not recovered. The
  conservative direction: the track then stays planned for copy exactly as
  today.
- failure to resolve the storage → `Err`, because no path could be answered.

Wire it through `GioDeviceBackend::probe_managed_files`
(`device_sync_backend.rs`), same shape as the existing `inspect`.

### 4. Repair the walk in the refresh path

`crates/reprise-gnome/src/ui/device_sync/device_sync_runtime_refresh.rs`, inside
`refresh_contents_with_delta`, between `backend.inspect(...).await` and the point
where `device.managed_files = managed_files` is assigned:

1. `load_device_files(&runtime.conn, &id)` — this device's ledger.
2. `doubtful` = ledger rows whose `device_path.to_lowercase()` is not among the
   walk's lowercased relative paths, plus their sidecar paths per task 2. Use
   **the same key the guard uses** (`mirror_file_changes.rs:39`); if the two ever
   diverge the repair silently stops matching.
3. If `doubtful` is non-empty, call `probe_managed_files` and extend
   `managed_files` with what comes back. The function already holds the
   `SyncTarget` it passed to `inspect`; the probe takes its parts —
   `target.path.clone()` for `target_path` and `target.storage_id` for
   `storage_id` (`SyncTarget` is `{ storage_id, path, enabled }`,
   `targets.rs:18-22`). Do not add a second overload that takes a whole
   `SyncTarget`.
4. `tracing::warn!(doubtful, recovered, "device scan came back short")` whenever
   anything is recovered, and a second `warn` when `doubtful` is implausibly
   large (say above the ledger's own row count), so a pathological case stays
   visible.

**No cap on the probe.** The doubtful set is bounded by the ledger, not by the
device: ~785 rows here, ~5 s worst case. A cap would refuse to work in exactly
the situation it exists for — the catastrophically short walk is the expensive
one. The refresh is async and the page shows its scanning state throughout, so
nothing blocks.

**Two implementation constraints, both easy to get wrong:**

- The `scan_generation` guard (`device_sync_runtime_refresh.rs:91-93`) must be
  re-checked **after the probe await**, not only after the inspect await. A
  second suspension point is a second chance for a newer walk to have started,
  and applying a stale *repaired* listing is worse than applying a stale plain
  one.
- The probe runs while nothing else holds the device from this session. It is
  read-only (`query_info`), so it does not need to serialise against a running
  sync — but it must honour the same `gio::Cancellable` path the rest of the
  refresh uses, so a disconnect mid-probe ends it instead of hanging.

### 5. No proof means no proof

When `probe_managed_files` fails as a whole (storage cannot be resolved, device
vanished mid-refresh), the walk is kept as it is — it is still fine for sizes and
counts — but **the residency guard is disarmed for this refresh**.

Concretely: a new `residency_proven: bool` on `DeviceState`
(`device_sync_runtime.rs:42-103`), reset with the rest of the per-connection
state on disconnect, that joins the existing derivation in
`device_sync_compact.rs:152`:

```rust
managed_files_scanned = device.ever_inspected
    && device.scan_error.is_none()
    && device.residency_proven
```

`scan_error` stays reserved for real inspection failures, so the page does not
report an error for something that merely could not be proven.

**The invariant, because the obvious default is wrong:** `residency_proven` is
`true` after *any* refresh in which the probe was not needed (doubtful set empty
— the overwhelmingly common case) **or** succeeded; it is `false` only when a
probe was needed and failed as a whole. Set it on both the empty-doubtful path
and the successful-probe path, and reset it with the other per-connection fields
on disconnect (`device_sync_device_list.rs:62-83`). A field that starts `false`
and is only ever set by a successful probe would leave the guard disarmed on
every normal refresh — a silent revert of #338 that no existing test would
catch.

The rule the whole fix rests on is "absence is only proof once it has been
checked". If the check cannot run, absence must not act like proof. The price is
that a file genuinely deleted on the phone waits for the next successful refresh;
the price of the alternative is a gigabyte.

A single path answering "not found" is **not** a failure — it is the valid
answer "really gone" and leaves the guard armed.

### 6. Say it on the page when the scan lied

`DeviceState` also carries the last repair's numbers (`short_scan: Option<(usize,
usize)>` — doubtful, recovered), set only when something was actually recovered
and cleared on the next clean refresh. The device page renders one line next to
`verified …`, e.g. *"Scan was incomplete — N files re-checked"*, via
`device_sync_strings.rs` like every other string on that page.

This is the only durable trace the event leaves. `RunLog::note` writes to
`sync_events`, but the repair happens during a **refresh**, where no run log is
open, and `sync_events.kind` has a six-value CHECK constraint that a seventh kind
would need a migration to widen. The page line costs a field and a string and
answers the one question the findings document leaves open. It also corrects a
claim the page already makes: "verified" today can be a short listing presented
as a complete one.

### 7. Tests

`crates/reprise-gnome/src/ui/device_sync/` — a new test module beside the
existing ones, driving the fake backend the way
`device_sync_auto_start_tests.rs` already drives `refresh_contents`.
`device_sync_fake_backend.rs` gains a scripted `probe_managed_files` and a call
counter.

1. **Short walk, file present** — the walk omits one inventoried path, the probe
   confirms it. Assert `plan.copy` is empty, and that the verified track count
   and `verified_track_bytes` include the recovered file at the size the *probe*
   reported.
2. **Short walk, file genuinely gone** — the walk omits it, the probe says
   absent. Assert the track is in `plan.copy`. This is the MTP-52 behaviour and
   must not regress.
3. **Complete walk** — probe call counter is `0`, **and**
   `managed_files_scanned` is `true` for the resulting projection. The second
   assertion is the one that catches a `residency_proven` that never gets set on
   the no-probe path; without it this test passes while the guard is disarmed
   everywhere.
4. **Probe fails** — the backend errors. Assert `managed_files_scanned` is false
   for the resulting projection and that nothing is planned for copy. The
   fixture must give every track a matching ledger row, so that the empty
   `plan.copy` is caused by the disarmed guard and not by tracks that were never
   desired in the first place.
5. **Sidecar recovery** — the walk omits both an audio path and its analysis
   sidecar, the probe confirms both. Assert `plan.analysis_writes` does not
   contain that track.

`crates/reprise-core/src/device_sync/mirror_inventory_truth_tests.rs` stays as it
is: the core planner's contract does not change, and those three tests are what
pins it.

### 8. Gate

`cargo test -p reprise-gnome -p reprise-core -p reprise-platform-linux`, then the
repo's normal dev gate. No new dependencies.

## Verification

The honest limit first: **the short walk has never been caught in the act.** Ten
walks with the app's exact API (`enumerate_children_async` +
`next_files_async(64)`, same attributes, break on empty batch) returned 2234/2234
files, ten out of ten, set-identical to `adb`; three concurrent walks only got
slower, not shorter. A device arm can therefore only fail to reproduce the bug —
it cannot prove the fix.

Accepted evidence for landing:

1. **The five tests above**, which is where the repair's behaviour actually
   lives.
2. **A device control arm after landing, not a proof.** On the next reconnect,
   record `sync_runs.planned` for the first run against
   `adb shell find /sdcard/Music/Reprise -type f | wc -l` and the
   `device_files` row count. A `planned` well above 0 with the files present on
   the phone means the fix did not take. A `planned` of 0 means nothing on its
   own — the walk may simply have been complete that time. Record which it was
   either way.

Explicitly **not** a landing condition: waiting for the task-6 line to appear
once. The fix is correct whether or not the short walk ever returns, and a change
that waits on a rare event ships nothing. That line is the instrument that will
answer the question on its own schedule; the findings document keeps the question
open until it does.

## Deliberately out of scope

- **Making the walk itself reliable.** Retrying an empty directory enumeration,
  or distrusting a walk that is short by more than a threshold, are both
  plausible and neither is grounded in a reproduction. This change makes the
  wrong answer cheap instead of guessing at its cause.
- **`sync_automatically`.** That a reconnect starts a sync by itself is what
  makes the bug expensive, not what makes it wrong. Untouched.
- **Lyrics sidecars.** They are excluded from the walk entirely
  (`is_known_managed_item_file`) and gated on their own resident size from
  `ManagedWalk::lyrics_files`, so they are not affected by this guard.

## Parallelität

**No cut. One strand.**

Files:

```
crates/reprise-gnome/src/ui/device_sync/device_sync_types.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_backend.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_runtime.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_runtime_refresh.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_compact.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_strings.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_fake_backend.rs
crates/reprise-gnome/src/ui/device_sync/device_sync_*_tests.rs
crates/reprise-platform-linux/src/device_sync_read.rs
```

The obvious cut would be "probe mechanics" against "visibility" (task 6). It
does not hold: the visibility strand needs the two new `DeviceState` fields in
`device_sync_runtime.rs` and the derivation in `device_sync_compact.rs`, both of
which the mechanics strand also edits — and it cannot compile without the
mechanics strand, because the number it displays is produced there. Two strands
on the same two files is a merge conflict announced in advance.

The chain trait → implementation → caller → test is exactly one dependency line,
and the whole change is well under a single Codex run.

No merge order, no post-merge cross-checks.
