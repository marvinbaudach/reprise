---
slug: the-unplanned-track-keeps-its-file
worktree: /home/marvin/Projects/reprise-the-unplanned-track-keeps-its-file
branch: feature/the-unplanned-track-keeps-its-file
phase: shipped
codex_session:
created: 2026-09-01
---
# The unplanned track keeps its file

## Why

Follow-up to **#773** (`The sync keeps the name the phone already has`). That
PR taught the device-sync planner to adopt the spelling a phone already uses
for a path that differs only in letter case, which stopped an endlessly
retried, endlessly failing transfer.

The review of #773 ran while the branch was already landing, so its findings
arrived too late to be applied in place. Six were accepted; this branch applies
them against `dev`. One of them is a data-loss path that #773 introduced, so
this is not cosmetic follow-up work.

Two findings from that review were considered and **rejected** — do not act on
them here:

- Directory spellings resolve against the whole parent path rather than per
  path segment, so a *new* album under an artist whose case has already drifted
  still targets a third spelling. Real, but a separate change with its own
  design question, not a repair of #773.
- The `desired[&track_id]` indexing and the two `.get_mut(..).unwrap()` calls
  in `rewrite_desired_paths`, plus the per-comparison `String` allocations in
  the folding helpers. Accepted as they are.

## Tasks

### 1 — An unplanned track must never become an unprotected one

`crates/reprise-core/src/device_sync/device_case.rs:85`, in
`rewrite_desired_paths`: when `adopt_resident_spelling` returns `Ambiguous` and
the track has **no** `inventory_by_id` entry, the track is removed from
`desired`. #773's premise for that branch was "no inventory row → nothing is on
the device, so nothing can be removed." That premise does not hold.

`inventory` (`device_files`) is desktop bookkeeping; `managed_files` is a scan
of what is physically on the device. The two can disagree — a transfer that
wrote bytes over MTP and died before its `DeviceFileRecord` was committed
leaves exactly that state, and this repository has documented MTP phantom
objects in `docs/plans/device-sync-mtp-phantom-objects-findings.md`.

The consequence, traced through the code:

1. `build_plan` (`mirror.rs`) rebuilds `desired_files` — and hence
   `plan.desired_files` — from the post-rewrite `desired_by_id`, so a dropped
   track is gone from it.
2. `known_paths` is chained from `inventory`, `playlist_inventory`,
   `plan.desired_files`, `plan.playlist_writes` and
   `owned_analysis_sidecar_paths`. A dropped track contributes to none of them:
   it has no inventory row by construction, and it is no longer in
   `plan.desired_files`.
3. `known_paths` is the only thing keeping a physically resident
   `ManagedDeviceFile` out of `ManagedRemoval::Orphan` in
   `plan_orphan_removals`. The `known_folded_paths` exemption does not save it
   either — the track's file name is unique, so its full path folds equal to no
   known path.
4. Because the track is no longer a key in `desired_by_id`, the first loop of
   `plan_file_changes` (which iterates `desired.keys()`) never runs for it, so
   **no `copy` is queued to replace what was just deleted**.

Net effect: a case-only directory tie can delete the user's audio file, and its
`.reprise-analysis` sidecar, off the phone with no re-transfer. Before #773 the
track stayed in `desired` and its path was therefore protected, so this is a
regression that PR introduced.

**Take this shape:** have `rewrite_desired_paths` return the pre-rewrite desired
paths of the tracks it dropped, and chain them into `known_paths`. The track
stays out of `desired_by_id`, so nothing is planned for it; its resident file is
protected anyway.

Do **not** solve it by keeping the track in `desired_by_id`. That branch's
defining condition is that the track has no inventory row, so `inventory_matches`
is false, and `plan_file_changes`'s first loop would plan a `copy` into the very
tied directory this logic exists to avoid — the endless-failing transfer, back
again. Making that work would need a new suppression flag on
`DesiredManagedFile`, which is outside this scope.

Protect **both** paths: the audio path and the analysis sidecar path derived
from it via `analysis_sidecar::device_path_for_track`.
`owned_analysis_sidecar_paths` reads `plan.desired_files` too, so the sidecar is
unprotected by exactly the same mechanism. A fix that saves only the audio file
still deletes the `.reprise-analysis` file — and a flat seek track is the
symptom that started #773 in the first place.

### 2 — `migrate_v81` must guard on the schema, not just the version

`crates/reprise-core/src/db_sync_log.rs:52` returns early when
`PRAGMA user_version >= 81`. Compare `db_device_sync.rs`'s `migrate_v68`, whose
doc comment explains this exact hazard and checks the actual schema shape
instead: a database that already ran an *earlier* build of an unshipped
migration under the *same* version number is otherwise never repaired.

That case is live here — the migration was planned as v80 and shipped as v81
because `dev` moved underneath it. A dev or CI database can therefore carry
`user_version = 81` together with a `sync_events.kind` CHECK that lacks
`analysis_failed`. On such a database the guard short-circuits forever and every
`analysis_failed` insert raises a CHECK violation, which
`device_sync_run_log.rs`'s `note()` swallows into a `tracing::warn!`. That is
precisely the silent-failure mode #773 exists to remove, one layer up.

Follow `migrate_v68`'s precedent: decide whether to rebuild from the real schema
— e.g. whether `SELECT sql FROM sqlite_master WHERE name = 'sync_events'`
already contains `analysis_failed` — rather than from the version number alone.
Keep the rebuild transactional and idempotent as it is today.

### 3 — The migration test must be able to fail for the reason the migration exists

**Resolved before this follow-up branch:** the base already contains the later
PR #773 review fixes H2 and M2. The test constructs the genuine five-kind v45
table directly, keeps the existing row-preservation assertion, and inserts an
`analysis_failed` row after migration. The stale `Db::open_in_memory()` premise
below no longer describes the repository and requires no implementation here.

`crates/reprise-core/src/db_sync_log_migration_tests.rs` opens through
`Db::open_in_memory()`, which runs the full current chain, so `sync_events`
already carries the new CHECK before the test forces `user_version` back to 80
and re-runs the migration. The rebuild goes from the new shape to the new shape,
and the test only asserts that a pre-existing `'failed'` row survived. A typo in
the CHECK list, or a missing `analysis_failed` entry, would leave it green.

After the migration call, insert a row with `kind = 'analysis_failed'` and
assert it succeeds. Keep the existing row-preservation assertion.

### 4 — Pin the run outcome

**Rejected by implementation evidence on this follow-up branch:** the focused
runtime test records `RunOutcome::Failed`, not `Completed`.
`device_sync_effects.rs` forwards `Event::AnalysisWritten(Err)` and
`machine.rs` assigns that error to `terminal_error`. The statement below that
`AnalysisFailed` cannot reach the terminal error considers only the run-log
counter and misses the independent state-machine path. Pinning `Completed`
would therefore be a behavior fix beyond this assertion-only task, and the
prohibited `machine.rs` is one of the relevant seams.

`crates/reprise-gnome/src/ui/device_sync/device_sync_analysis_metadata_tests.rs`,
`failed_analysis_copy_records_track_path_and_error` drives a real sidecar
failure through the runtime and asserts the deviation is recorded, but never
asserts the run's outcome. The behaviour is currently correct —
`device_sync_effects.rs` passes the `Result` through unchanged and
`device_sync_run_log.rs` counts only `DeviationKind::Failed`, so `AnalysisFailed`
cannot reach `terminal_error`. Nothing guards that against a future edit.

Assert in that test that the run's outcome is the completed one, so "a failed
analysis sidecar does not abort the run" is pinned rather than merely true.

### 5 — Document the orphan-removal exemption

`plan_orphan_removals`' `known_folded_paths` exempts any managed file that folds
equal to any known path. That is deliberate — decision 4 of #773's plan: the
cost of a wrong delete is asymmetric, since a delete attempt on an MTP phantom
raises `could not delete object` and aborts every later sync at `CleanPartials`.
But the consequence — a genuinely orphaned case-variant duplicate is never
reclaimed and its space on the device never freed — is invisible to the next
reader.

Add a short comment stating the tradeoff and why it was chosen. No behaviour
change.

### 6 — Two tests do not exercise what they claim

- `equal_directory_counts_plan_neither_arrival_analysis_nor_removal`: its
  `plan.remove.is_empty()` assertion only covers the tracks already protected by
  exact inventory-path matches, not by any of #773's new logic. Task 1's new
  test is the real coverage; make sure the two do not merely duplicate each
  other, and that this one keeps asserting what its name promises.
- `unavailable_track_keeps_its_minority_inventory_spelling`: the track is
  `Unavailable`, so it never enters `desired_by_id` and never passes through
  `rewrite_desired_paths` — the test is vacuous with respect to that code, and
  its path is protected by the pre-existing exact `known_paths` match. Either
  make it exercise the rewrite path, or rename it so it stops claiming coverage
  the code does not have.

## Verification

- **The new regression for task 1**, written red first and observed failing
  against the current code: a track with no inventory row, whose album directory
  is tied between two case spellings, and for which a `ManagedDeviceFile`
  physically exists both at its desired path and at the derived sidecar path →
  `plan.remove`, `plan.copy`, `plan.replace` and `plan.analysis_writes` are all
  empty.
- The existing tie test passes unchanged.
- `cargo test -p reprise-core device_sync`, `cargo test -p reprise-gnome device_sync`,
  the migration tests.
- `cargo fmt --check`, workspace build, strict workspace clippy — each exit
  status captured directly, never through a pipe.
- No user-facing strings, therefore no gettext work expected. If one appears
  anyway, `scripts/tests/gettext-catalogs.sh` must exit 0.

## Parallelität

**One strand.** Roughly 60 lines across two crates, and tasks 1 and 6 touch the
same test module — there is no disjoint file group worth two builds and two
landings.

## Post-merge

Unchanged from #773 and still outstanding: the seven tracks already split across
two spellings on the phone need the MTP recovery (`scan_volume` + remount), then
one sync that runs to completion, then a library scan on the phone. Nothing in
this branch heals them.
