---
slug: doctor-snapshot-freeze
worktree: /home/marvin/Projects/reprise-doctor-snapshot-freeze
branch: feature/doctor-snapshot-freeze
phase: shipped
codex_session:
created: 2026-09-22
---
# The Doctor's snapshot remembers what the Doctor wrote

## The defect

`refresh_snapshot_after_successful_doctor_write`
(`crates/reprise-core/src/library/library_doctor/store.rs:279`) runs inside the
write transaction of every successful `doctor_apply` and updates the row in
`library_doctor_scan_tracks` — but only its **file-identity** columns
(`path, file_mtime, file_size, device, inode`). The seven **tag** columns keep
the value the scan read *before* the write.

The next scan compares the `DoctorTrackRef` identity (`scan.rs:124–127`), finds
it unchanged — precisely because this function refreshed it — and takes the
DOC-1g reuse path (`scan.rs:174–177`), which clones the stored tags instead of
reading the file. The pre-write reading is carried forward and cascades scan
after scan.

The function's own doc comment already claims it refreshes "reconciled `tracks`
from the file it just read". The SQL never kept that promise.

Full diagnosis and evidence chain:
`docs/plans/HANDOFF-2026-09-22-doctor-snapshot-freeze.md`.

### Measured on the real library (read-only copy)

Scan 7 snapshot vs. `tag_write_journal` (`outcome='applied'`,
`kind='doctor_apply'`):

| field | applied | snapshot still holds the pre-write value |
|---|---:|---:|
| artist | 81 | 81 |
| album_artist | 91 | 88 |
| title | 140 | 140 |
| album | 117 | 117 |

A second, independent query confirms it from the other side: snapshot and
`tracks` disagree on **exactly 300 rows**, and those are **exactly** the 300
tracks with an applied `doctor_apply` write. Nothing else in the library has
drifted.

The symptom that started this: on 2026-08-30 the Doctor applied
`REFORMIST → Reformist` to five of seven tracks under
`/home/marvin/Music/REFORMIST/` (track 2219 never had an `artist` proposal,
track 985 had none at all). It *created* a two-spelling split and has been
unable to see it ever since.

## Decisions (settled in the grill, 2026-09-22)

**D1 — the initial design took the refreshed value from the `tracks` row for
all seven tag columns.**
`reconcile_after_write` (`tag_mutation_guarded.rs:250`) runs the real scanner
over the file *before* `terminal_success`, so `tracks` holds a genuine
read-back from disk. If reconciliation fails, `post_write_failure` is set,
`state` becomes `"failed"`, and the `if wrote && state == "complete"` guard
(`write.rs:517`) means the refresh never runs on an unreconciled row. No extra
I/O, no network, and it is what the doc comment already promises.

Rejected as the general source: `tag_write_journal.after_value` records what we
*asked* to write, not what the file says afterwards. The narrow empty-title
exception uses it only to distinguish a genuinely applied empty value from the
scanner's filename fallback. Also rejected: forcing a re-read via a marker
column (drags the remote resolution of the whole album along — reading-reuse
and remote-reuse are one decision, `scan.rs:112–121`).

**D1a — one guard test decides whether D1 must be narrowed.** The scanner and
`read_editable_tags` are two different readers: `scanner_entry.rs:446` sets
`tracks.title` to the file's display name for *any* empty title, where the tag
reader yields `""`. In the real library this affects **0** rows today, so D1 and
a field-narrowed variant are empirically identical here — but the divergence is
real in code. T2 therefore carries a test with an untitled fixture. If it is
green under D1, D1 stands. If it is red, narrow the copy to the fields that
appear as `applied` in this file's journal — a `WHERE` on the column list, not a
redesign.

**D1a outcome — the guard fired and the narrow branch was taken.** The test was
red under the wide copy because the scanner's filename fallback is not the
file's empty title. Both the runtime refresh and migration therefore update
only fields recorded as `applied` by a `doctor_apply`; for an applied empty
title, the journal's empty applied value overrides the scanner fallback.

**D2 — the 300 frozen rows are repaired once, in a migration, restricted to the
last complete scan.** The reuse path only ever reads `last_complete_scan`
(`scan.rs:103`), so older snapshots have no effect. The repair is the same
operation as the fix. Deterministic, no file reads, no network, and it makes the
acceptance criterion reachable without waiting for an unrelated file change.

Note the semantic shift it accepts: the tag columns of that scan then hold "what
the file says now" rather than "what we read then". For the reuse path that is
the intended meaning. For the review page it is inert — review rows carry their
own `current_value` from `library_doctor_proposals`, and a row whose field was
written has already left the scan through `written_pairs`.

**D3 — proof is both a fixture test in the gate and one copy-DB run.** The
fixture test is the permanent regression; the copy-DB run is the evidence for
this specific report. The CLI has no doctor command and MCP's `music_scan_tags`
would touch the live DB, so the copy-DB run is an `#[ignore]`d test in the
worktree, not committed. It needs neither files nor network: with
`remote_enabled=false`, `may_reuse_readings` is true and every identity matches,
so the scan reads only the repaired snapshot.

**D4 — the guarantee becomes a new rule, DOC-1h `[active] [core]`.** It is a
different guarantee from DOC-1g's "skips unchanged files", and the rulebook's
traceability rule wants a test to carry exactly one primary rule ID. DOC-1g gets
a cross-reference; its thirteen existing test names stay where they belong.
`[planned]` is not an option — the rulebook requires a rule to go `[active]` in
the commit that implements it.

**D5 — `doctor_revert` stays out of scope.** A revert runs under
`kind='doctor_revert'`, which the refresh's SQL guard excludes, so the identity
stays stale and the next scan re-reads the file. That is the safe direction and
needs no change here.

## Tasks (test-first, one commit each)

**T1 — the failing test.** In
`crates/reprise-core/src/library/library_doctor/snapshot_refresh_tests.rs`:
scan a fixture track, apply a single-field `doctor_apply` write, then assert the
snapshot row's column for that field equals what `read_editable_tags(path)`
returns for it. Run it, watch it fail.

`doc_1h_a_written_field_is_remembered_as_the_file_now_reads_it`

**T2 — the fix, plus the guard test from D1a.** Extend
`refresh_snapshot_after_successful_doctor_write` to copy from `tracks` only the
tag fields recorded as `applied` for this `doctor_apply`, alongside the identity
columns, in the same statement and the same transaction. An applied empty title
comes from the journal so the scanner's filename fallback cannot replace it.
Keep the identity refresh exactly as it is — `stale_flags` depends on it and
`doctor_apply_on_worker_connection_refreshes_snapshot_before_remaining_rows_are_classified`
(`snapshot_refresh_tests.rs:53`) pins it. Fix the doc comment to describe what
the code now does.

Guard test, same file: a fixture **without a title tag**, a `doctor_apply` write
to `artist` only, assert the snapshot's `title` stays `""` rather than picking
up the scanner's filename fallback. Red means D1a applies — narrow the copy to
the journal's `applied` fields and keep both tests.

`doc_1h_an_untitled_file_keeps_an_empty_title_in_the_snapshot`

**T3 — the regression that would have caught the bug.** In
`crates/reprise-core/src/library/library_doctor/reuse_scan_tests.rs`: several
tracks sharing one normalised group key and one spelling; apply a write that
changes the majority of them; scan again with the reader counting reads; assert
(a) the second scan reads no file — DOC-1g still holds — and (b) it nevertheless
proposes the majority spelling for the untouched minority. This is the Reformist
case in miniature.

`doc_1h_a_split_the_doctor_created_is_found_by_the_next_scan`

While writing it, confirm rather than assume that
`doc_1g_a_skipped_track_keeps_its_previous_proposals` (`reuse_scan_tests.rs:161`)
does not deliberately encode the identity-only refresh. Read as written it is
about proposals surviving a *skipped* track, which is a different subject.

**T4 — the one-off repair.** `migrate_v86` in
`crates/reprise-core/src/db_library_doctor.rs`, bumping
`SUPPORTED_SCHEMA_VERSION` (`db.rs:24`) from 85 to 86: apply the same
field-narrowed copy to the rows of `library_doctor_state.last_complete_scan_id`
whose track has an applied `doctor_apply` write and whose `read_ok=1`.

**The `EXISTS` must not be scoped to a scan.** This is the one place the
migration can quietly do nothing. The writes that froze the current snapshot
belong to *earlier* scans' jobs — the Reformist rows were frozen by job 22,
which belongs to scan 4, while the snapshot being repaired is scan 7. The
natural reading, and the shape `written_pairs` (`store.rs:395`) uses, is
`j.scan_id = s.scan_id`; here that would match only the current scan's own jobs
and leave most of the 300 rows frozen. The predicate is
`j.kind='doctor_apply' AND v.outcome='applied'` across **all** jobs, with no
`j.scan_id` condition. The measurement that produced the number 300 joined the
journal exactly this way.

If D1a triggers and the fix narrows to the journal's `applied` fields, this
migration narrows the same way — a wide repair beside a narrow fix would make
the two disagree on the first untitled file.

Migration test in the same file, matching the local convention
(`migration_v66_to_v67_preserves_existing_scans_and_is_idempotent`): a snapshot
row whose track was written by an **earlier scan's** job ends up holding the
`tracks` value — that case is the point of the test, not an extra; a row with no
write is untouched; running it twice changes nothing the second time.

`migration_v85_to_v86_repairs_written_snapshot_rows_and_is_idempotent`

**T5 — the rule.** Add DOC-1h `[active] [core]` to section Y of
`docs/ux-rules.md`, directly after DOC-1g (`docs/ux-rules.md:4667`):

> **A file the Doctor itself has written is not an unchanged file.** After a
> successful `doctor_apply` the stored reading of that track is brought up to
> date with the write, so a later scan that reuses it never works from a
> pre-write value — including a spelling split the Doctor's own partial apply
> created.

`*Tests:*` lists T1, T2's guard test and T3. Append a one-sentence
cross-reference to DOC-1g naming DOC-1h as the exception to "skips unchanged
files". Do not renumber or rewrite anything else in section Y.

**T6 — gates.** From the repo root: `cargo fmt --check`;
`cargo clippy --all-targets --workspace -- -D warnings`;
`cargo test --workspace`; `cargo audit` (only RUSTSEC-2024-0436 accepted);
`scripts/check-ux-traceability.sh` (DOC-1h must resolve to a real `#[test]`);
and the core purity proof — `cargo tree -p reprise-core | grep -E
'gtk4|libadwaita|gstreamer|zbus'` must be empty. This change is core-only.

**T7 — the copy-DB evidence run (not committed).** In the worktree, an
`#[ignore]`d test that opens a copy of the real DB
(`cp ~/.local/share/reprise/reprise.db $SCRATCH/verify.db`; the live DB is never
opened), applies the migration, runs a whole-library scan with
`remote_enabled=false`, and asserts both halves of the acceptance criterion.

It stays in the worktree and is **not part of any commit** — the same standing
this repo gives its other measurement harnesses. Do not delete it: `/check`
reviews the worktree diff and `/refactor` runs in the same worktree, so removing
the harness mid-sequence throws the evidence away before anyone has read it.
What travels onward is its output, in the report.

## Acceptance criterion

On the copy of the real DB, after migration and a `remote_enabled=false`
whole-library scan:

1. All seven `/home/marvin/Music/REFORMIST/` tracks group under
   `normalize_group_key("reformist")`, the count is 5 × `Reformist` against
   2 × `REFORMIST`, and the **local** rule proposes `Reformist` for track 2219
   and track 985 — no remote lookup involved.
2. The number of scan-snapshot rows whose tag columns disagree with `tracks`
   goes from **300 to 0**, measured with the same query that produced the 300 —
   not a re-derivation:

   ```sql
   SELECT COUNT(*) FROM library_doctor_scan_tracks s
     JOIN tracks t ON t.id = s.track_id
   WHERE s.scan_id = (SELECT last_complete_scan_id FROM library_doctor_state)
     AND s.read_ok = 1
     AND (coalesce(s.title,'')        <> t.title
       OR coalesce(s.artist,'')       <> t.artist
       OR coalesce(s.album,'')        <> t.album
       OR coalesce(s.album_artist,'') <> t.album_artist);
   ```

   This is the four-text-field proxy: `year`, `track_no` and `genre` are
   deliberately not in this check, because they are the columns where an
   integer-vs-text or empty-vs-null difference would produce noise rather than
   signal. The measured outcome was four-field mismatches **300 → 0**, with
   five residual `year` rows whose last year write came from the Tag Editor,
   not the Doctor. That open case is filed as DOC-1i rather than silently
   widened into this repair.

The authoritative local pass is `scan.rs:246`
(`local_rules::proposals_for(&read_tracks)` over the whole set); the per-track
call at `scan.rs:200` is only the progress forecast its own comment says it is.

## Out of scope

- The partial-apply behaviour that created the split — track 2219 got an
  `album_artist` proposal but never an `artist` one. That is a question about
  remote coverage, not about the snapshot.
- `terminal_failure`'s path, where a file was written but the job still failed:
  it calls no refresh at all, so the identity stays stale and the next scan
  re-reads. Safe direction, no change.
- `doctor_revert` — see D5.

## Parallelität

**No cut.** The change is five files and they are coupled through one function
and one gate:

- T2 writes `store.rs`; T4's migration re-uses the very SQL shape T2 introduces;
  T1, T2's guard test and T3 assert T2's behaviour.
- T5 (`docs/ux-rules.md`) looks separable, and is not. `check-ux-traceability.sh`
  is part of the merge gate and requires every `[active]` rule to resolve to a
  real rule-named `#[test]`. A strand that adds DOC-1h `[active]` while another
  strand owns the test files could not go green in its own worktree **in
  principle** — the exact trap the pipeline's own guidance names. Making it
  `[planned]` to dodge that contradicts the rulebook's same-commit requirement.

So the honest cut is one strand. Merge order and post-merge cross-checks: not
applicable.
