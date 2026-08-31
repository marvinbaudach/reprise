---
slug: the-sync-keeps-the-name-the-phone-already-has
worktree: /home/marvin/Projects/reprise-the-sync-keeps-the-name-the-phone-already-has
branch: feature/the-sync-keeps-the-name-the-phone-already-has
phase: shipped
codex_session:
created: 2026-08-31
---
# The sync keeps the name the phone already has

## Why

A track whose desired device path differs from the resident one **only in
letter case** is re-planned as a transfer on every single run, and every run
fails the same way. The phone's filesystem is case-insensitive; MTP keeps both
spellings as separate objects, so creating the "new" directory fails with
`libmtp error: Could not send object info`.

The analysis sidecar is derived from that same desired path
(`plan_analysis_sidecars` uses `desired.device_path`), so it targets the same
uncreatable directory and fails too. Android never computes analysis itself, so
the seek track of those songs stays a flat line forever. That is the symptom
this started from: "manche Songs haben kein Spektrum".

The case changes come from the **desktop** side — retagging, the Doctor,
MusicBrainz — so this recurs; it is not a one-off:

| Track | tag today | on the device |
|---|---|---|
| 1667 | Bring Me **the** Horizon | Bring Me **The** Horizon |
| 1276 | We Butter **the** Bread | We Butter **The** Bread |
| 2500 | Graveside Confessions | GRAVESIDE CONFESSIONS |

Measured evidence is in `docs/plans/android-flat-seek-track-findings.md`:
7 of 17 failed transfers are pure case conflicts (tracks 512, 808, 1276, 1666,
1667, 2151, 2500), and `device_files` carries 7 album directories under two
spellings at once. The related MTP phantom behaviour is in
`docs/plans/device-sync-mtp-phantom-objects-findings.md`.

### The exact break

`crates/reprise-core/src/device_sync/mirror.rs:668`, in `inventory_matches`:

```rust
existing.device_path == desired.device_path
```

An exact string compare. A case change in an artist or album tag therefore
falls through `plan_file_changes` into `plan.replace`, which writes into the
case-variant path that cannot be created.

### What this fix does not do

It stops **new** divergence and stops the endless retry. It does **not** heal
the 7 tracks already split across two spellings on this phone — those need the
MTP recovery from the phantom report (`scan_volume` + remount) first, then one
sync that runs to completion, then a library scan on the phone. Say this in the
PR body; nobody should expect the spectra to return on merge.

## Decisions

Settled in the plan grill on 2026-08-31.

1. **The resident spelling wins. Never rename on the device.** Renaming a case
   variant over MTP is the operation that fails today, and a failed attempt is
   what creates the phantoms that abort later syncs. Consequence, accepted: the
   phone keeps the old spelling even after a deliberate tag correction. Nobody
   browses that folder tree; the phone's library reads tags.
2. **Rule 1 — a track's own `device_files` row decides.** Per-track, exact, and
   it records a spelling the desktop demonstrably wrote successfully. This also
   keeps a track that sits in the *minority* directory where it is, so the fix
   never turns into a move.
3. **Rule 2 — majority, and on a tie do nothing.** Only for tracks with no
   inventory row. The directory carrying the most files wins; that is right in
   6 of the 7 real cases. `Chelsea Grin/Desolation of Eden` is 4 : 4 — on a tie
   the track is left unplanned and a `tracing::warn!` names it, rather than
   tossing a coin and failing on every run.
4. **Case-variant siblings are never removal candidates.** Rule 1 already keeps
   them in `known_paths`, so this is a belt: a few lines and a test. The cost of
   being wrong is asymmetric — a delete attempt on a phantom raises `could not
   delete object`, which is what aborts every later sync at `CleanPartials`.
5. **Fold with `to_lowercase()`, not `eq_ignore_ascii_case`.** The affected set
   includes `12 Angriff der Dönerteller`; ASCII-only folding misses non-ASCII
   case differences. This matches the existing collision keys in
   `transfer.rs:158` and `mirror.rs:706`.
6. **No user-facing UI and therefore no gettext work.** There is no sync run
   report in the UI today — `sync_runs` is read only by the diagnostic helpers
   `recent_runs`/`deviations`. A new `MirrorWarning` variant is deliberately
   *not* used: it maps to `SyncPageWarning` in `page.rs:231` and would surface
   in the dock, dragging in `po/reprise.pot`, `de` and `es`. A device-sync
   report is its own feature, not a by-product of this fix.
7. **The state machine is not touched.** The runtime already holds the
   `AnalysisSidecarWrite` at the effect site (`device_sync_effects.rs:245`), so
   the deviation is recorded there. Routing it through `machine.rs` would widen
   the change for nothing.

## Tasks

### 1 — Adopt the resident spelling

New module `crates/reprise-core/src/device_sync/device_case.rs`, declared in
`crates/reprise-core/src/device_sync.rs`.

```rust
/// The spelling the device already uses for a path, when it differs only in case.
pub(super) enum ResidentSpelling {
    /// Use this path instead of the desired one.
    Adopt(String),
    /// Nothing on the device folds equal; keep the desired path.
    Keep,
    /// Two spellings tie; the caller must not invent one.
    Ambiguous,
}

pub(super) fn adopt_resident_spelling(
    desired_path: &str,
    own_inventory_path: Option<&str>,
    directory_spellings: &HashMap<String, DirectorySpelling>,
) -> ResidentSpelling
```

A named enum rather than `Option<String>`: the tie and "no entry" are different
answers with different handling, and one `None` for both is how they get
conflated.

Rules, in order:

1. `own_inventory_path` is `Some(p)` and `p` folds equal to `desired_path`
   → `Adopt(p)`.
2. Otherwise fold the desired path's parent directory. If `directory_spellings`
   holds an unambiguous different spelling for that key → `Adopt(that
   directory + the desired file name)`.
3. Ambiguous key (a tie) → `Ambiguous`.
4. No entry → `Keep`.

Plus a builder over the inventory and `managed_files` that produces, per folded
directory key, either the winning spelling or an ambiguity marker. Deterministic:
count files per spelling, highest count wins, equal counts are a tie.

### 2 — Apply it once, early

In `plan_mirror` (`mirror.rs`), rewrite every `DesiredManagedFile.device_path`
right after `desired_by_id` is built and **before** `plan_file_changes` and
`plan_analysis_sidecars` run.

**`Ambiguous` must never remove the track from `desired_by_id`.** The third loop
in `plan_file_changes` treats every inventory row whose track is absent from
`desired` as a removal candidate, so dropping the entry would delete the file
from the phone — the opposite of decision 3. Handle it as:

- the track has an inventory row → keep it in `desired_by_id` with **its own
  inventory path**, so `inventory_matches` is true and the run does nothing for
  it;
- the track has no inventory row → it is not on the device and nothing can be
  removed, so leave it out.

Both branches emit `tracing::warn!` with the track id and the two spellings.
(On this device every tie case currently has an inventory row, so rule 1
already covers it; the branch exists for the case that does not yet occur.)

Everything downstream then works unchanged on the corrected path:
`inventory_matches`, `arriving_audio_paths`, `known_paths`, and the sidecar
path derived by `analysis_sidecar::device_path_for_track`.

Do not change `sanitize.rs`. The generated name stays as it is; only the match
against what is already on the device changes.

### 3 — Keep case-variant siblings out of orphan removal

`plan_orphan_removals` (`mirror.rs:508`) treats every managed file not in
`known_paths` as removable. Exclude a managed file whose path folds equal to a
known path. Pin it with a test.

### 4 — A deviation kind for the analysis sidecar

Migration `migrate_v80` — confirm the next free number against `origin/dev` at
implementation time; `db.rs:761` currently ends at v79. SQLite cannot alter a
CHECK constraint in place, so rebuild `sync_events` following the shape of
`db_device_sync::migrate_v68`: create the new table, copy, drop, rename,
recreate `idx_sync_events_run`.

New allowed kind: `analysis_failed`. Add the matching variant to `DeviationKind`
in `device_sync/sync_log.rs` and to its `as_str`.

### 5 — Record the failure

In `device_sync_effects.rs`, when `copy_analysis_sidecar` returns `Err`, record
a deviation carrying the track id, the sidecar device path and the error,
through the path the other deviations already take
(`device_sync_run_log.rs:111` → `sync_log::note_deviation`). The run's
`terminal_error` behaviour stays exactly as it is.

### 6 — Say something when a sidecar is skipped at plan time

`device_sync_compact.rs:286` drops a track with `Ok(None) => continue`. That
runs inside the delta computation (`device_sync_compact.rs:187`), before a run
row exists, so it cannot be a `sync_events` entry. Make it a `tracing::warn!`
naming the track id. A log line, not a feature.

## Verification

- `mirror_tests.rs`: resident at spelling X, desired at spelling Y →
  `plan.copy` and `plan.replace` are both empty, and the planned
  `AnalysisSidecarWrite.device_path` is derived from X.
- `mirror_tests.rs`: the same with a non-ASCII difference
  (`.../12 Angriff der Dönerteller.opus`).
- `mirror_tests.rs`: a track with no inventory row whose album directory is
  resident under another spelling is planned into the resident directory.
- `mirror_tests.rs`: a 4 : 4 tie leaves the track out of `copy`, `replace` and
  `analysis_writes` — **and out of `remove`**. This is the assertion that
  catches the data-loss reading of task 2.
- `mirror_tests.rs`: a retained unavailable track whose inventory path is a
  minority spelling stays in `retained_unavailable` and out of `remove`.
- `mirror_tests.rs`: a track sitting in the minority spelling with its own
  inventory row is **not** moved.
- `mirror_tests.rs`: a managed file folding equal to a known path is **not** in
  `plan.remove`.
- `sync_log_tests.rs`: an `analysis_failed` deviation is written and comes back
  from `deviations()`.
- A migration test: an existing `sync_events` row survives the rebuild.
- `cargo test -p reprise-core device_sync`, `cargo test -p reprise-gnome device_sync`.
- `cargo fmt --check`, workspace build, strict workspace clippy — each exit
  status captured directly, never through a pipe.
- No gettext work is expected. If a user-visible string appears after all,
  `scripts/tests/gettext-catalogs.sh` must exit 0.

## Parallelität

**One strand — deliberately not cut.**

The file sets would be disjoint (planner and tests on one side; migration,
`sync_log`, and the effect call site on the other), so the cut was permissible.
It was rejected on size and on evidence order: together this is roughly 120
lines, and two worktrees mean two full Rust builds, two reviews and two
landings. More importantly, task 4/5 is the instrument by which tasks 1–3 are
observed on the device — split apart, the observation lands after the thing it
is supposed to evidence.

**Post-merge check on the live phone** (needs the MTP recovery first, so it
cannot gate the merge):

- After `scan_volume` + remount and one sync that runs to completion, a
  following run plans **no** transfer for tracks 512, 808, 1276, 1666, 1667,
  2151 and 2500 — today it plans one every time.
- `sync_runs.detail` for that run no longer ends in
  `could not copy analysis sidecar`.
- After a library scan on the phone, those tracks show spectral bars instead of
  the plain seek track.
