---
slug: doc-1i-frozen-snapshot-field
worktree: /home/marvin/Projects/reprise-doc-1i-frozen-snapshot-field
branch: feature/doc-1i-frozen-snapshot-field
phase: planned
codex_session:
created: 2026-10-01
---
# DOC-1i — a field written by another actor stays frozen behind a later Doctor write

Closes the last `[planned]` rule of the doctor-snapshot-freeze round. The shape
of the answer was settled in the grill; what follows is the decided plan, not a
survey of options.

## The defect

1. The Tag Editor writes `year` to a file. The Doctor's stored reading
   (`library_doctor_scan_tracks`) is not touched.
2. A later `doctor_apply` writes a *different* field (`artist`) to the same
   file. Per DOC-1h it refreshes the file identity and the fields **it** wrote.
3. The identity is now current, so a later scan treats the file as unchanged and
   reuses the stored reading — including `year`, still at its pre-Tag-Editor
   value.

Measured on the real library in the #1012 round: tracks 288, 289, 291, 292, 293
have snapshot `year` NULL against `tracks.year` 2008, with journal evidence of
`tag_editor:year:applied` followed by `doctor_apply:artist:applied`. Not
re-measured for this plan — the real DB is off limits to unasked tooling, and
nothing about the five rows changes the decision.

## The evidence the decision rests on

| # | Fact | Where |
|---|---|---|
| E1 | Reuse is identity equality, per track: `previous.snapshot.reference == **track` over `DoctorTrackRef` (track_id, path, mtime, size, device, inode). | `library_doctor/scan.rs:121` |
| E2 | The identity refresh exists to protect the *review session*: a stale track's proposals are dropped from it outright (`fingerprint_allows_review`). The commit that introduced the refresh is #594, "The doctor stops staling its own rows and stops showing the ones it did". | `library_doctor/review.rs:268`, `9cf35608a7` |
| E3 | The per-field journal lookup inside the refresh is **job-scoped**: `v.file_id=?3` is a `tag_write_job_files.id`, a row belonging to this Doctor job. Another actor's writes are structurally invisible to it. | `library_doctor/store.rs:289–348` |
| E4 | Some tag writers leave no journal row at all: the scanner's move/reconcile path writes tags *and* identity into `tracks` directly. | `library/scanner_move.rs:214` |
| E5 | **Every** tag write — `tag_editor` as much as `doctor_apply` — refreshes `tracks.file_mtime/file_size/device/inode` synchronously, via `reconcile_after_write()` → `scan_folder_in()` → the scanner UPSERT. | `library/tag_mutation.rs:463`, `library/scanner_entry.rs:376` |
| E6 | The write path already guards **per field**: it reads the file's current values and compares them against the plan's frozen `expected`; a mismatch becomes `outcome = "conflict"` and the field is not written. Beyond that it validates only the path. | `library_doctor/write.rs:150–200` |
| E7 | The snapshot's identity columns come from `tracks`, not from a `stat()` of the file. | `library_doctor/scope.rs:31` |
| E8 | The snapshot's tag columns are not only a skip cache: they seed the proposals (`local_rules.rs:34/49/70/127`) and are what the review page shows as the **current value** (`review.rs:293` → `review_model.rs:216` → `review_row.rs:267`). A frozen field produces a wrong proposal *and* a wrong baseline to judge it against. | as listed |
| E9 | The scan itself stores `""` for an untitled file's snapshot title — the scanner's filename fallback lives in `tracks.title`, not in the snapshot. A re-read therefore reproduces an empty title without help from the journal. | `snapshot_refresh_tests.rs:149` |
| E10 | No reader outside the Doctor touches these columns — MCP, CLI and the runtime protocol have no hits. | scout sweep |

**How the measured case gets past the existing guards.** E5 and E6 together
explain it. The Tag Editor write makes the track stale immediately — `tracks`
identity moves, the snapshot's does not — so a review session built after that
point would exclude it (E2). The measured case survives because the apply ran
from a plan **frozen before** the Tag Editor write, and the only thing between a
frozen plan and the file is the path check plus the per-field conflict check.
`year` was not a field the plan wrote, so nothing looked at it.

E6 is also the frame for the whole rule. Reprise already promises *"I do not
overwrite a field someone else changed since the scan."* DOC-1i completes that
promise: from *the field I write* to *the file I touch*.

## The decision

**A Doctor write refreshes the stored reading only when, at the moment that
write begins, the file still matches the identity that stored reading carries.
Otherwise it leaves the snapshot row completely untouched.**

The reference is deliberately *the stored reading's own identity*, not "what the
scan originally recorded". After an earlier apply the row legitimately holds a
refreshed identity, and the promise is about the file still matching the row the
next scan would reuse.

Four points were settled in the grill and are not open:

1. **Any actor, not just the Tag Editor.** The comparison reads identities, not
   journal rows, so Picard, beets and the scanner's own move path (E4) are caught
   as surely as the Tag Editor. That is the rule as titled, and it is why no a/b
   split is needed.
2. **The reference is `tracks`, not a fresh `stat()`.** The snapshot's identity
   is a copy of `tracks` (E7), and `stale_flags` compares the same two sources
   with the same equality — `store.rs:455` insists that "changed under us" must
   not mean two different things in two places. A `stat()`-based check would be
   stricter but would introduce a second, disagreeing notion of staleness.
3. **On mismatch: refresh nothing.** Not the identity, not the Doctor's own
   fields. With the identity left stale the next scan re-reads the file and
   reads those fields from disk anyway, so a field-only refresh buys nothing and
   leaves the row a mixture of scan-time and post-write values — the half-state
   DOC-1i exists to remove. E9 removes the one counter-argument: the empty-title
   journal special case is not needed on the re-read path.
4. **Only what the snapshot remembers.** Whether the write should happen at all
   on a file that changed under the scan is a different promise and gets its own
   `[planned]` rule (task T5), not a clause here.

**Why this keeps #594 fixed.** A file nothing else touched is blessed exactly as
today, so the Doctor still does not stale its own rows. A file that *was*
touched stays stale, and its remaining rows stay out of the review session —
which is correct, because the file genuinely changed under the scan. #594's
principle was that the Doctor must not stale its **own** rows; it says nothing
about rows another actor staled.

## Tasks — test-first, in order, one commit

**T1 · The failing test.** Add
`doc_1i_a_field_another_actor_wrote_is_not_frozen_by_a_later_doctor_write` to
`crates/reprise-core/src/library/library_doctor/snapshot_refresh_tests.rs`,
using the existing `fixture` / `scan_track` helpers:

1. scan the file;
2. build a `DoctorReviewSession` from that scan and `freeze_plan()` an `artist`
   row — frozen *before* step 3, which is what the measured case did;
3. write `year` through the **Tag Editor's own path**
   (`crates/reprise-core/src/library/tag_edit_write_pipeline.rs`), never raw SQL:
   the test has to exercise the reconcile of E5 or it proves nothing;
4. `apply_review_plan` the frozen plan;
5. run a second scan.

Assert the second scan's stored `year` equals the file's actual `year`. If a
handle to `reused_readings` is reachable from the scan outcome, assert the track
is not in it as well; otherwise the stored `year` of the new scan row is the
assertion. **Run it and see it fail before writing T2.** A test that passes here
is measuring the wrong thing.

**T2 · Capture the verdict.** For each file, immediately **before** its Lofty
write, compare two identities and carry the result to `terminal_success`:

- left: this job's current row in `library_doctor_scan_tracks`
  (`scan_id` from `tag_write_jobs`, `track_id` from the job file);
- right: the file's row in `tracks`.

Compare them as `DoctorTrackRef` values, using the same equality `stale_flags`
uses — `store.rs:455` insists that "changed under us" must not mean two
different things in two places. Do not write a second comparison.

**Never compare against the plan-carried `DoctorTrackRef`** (`inputs[0].track`
in `prepare_files`, which the existing path check uses). That ref was frozen when
the plan was made, so after an earlier apply to the same file it is legitimately
out of date — comparing against it would refuse the refresh on the Doctor's own
second write and re-break #594.

Place the comparison in the executor, next to the per-file write, not in
`prepare_files`: `prepare_files` runs for every file before any of them is
written, so a verdict taken there is stale by the time the second file is
written, and it would not survive a job resumed from persisted state. Read
`run_job` (`write.rs:734`) and the point where `ExecutableFile` is built before
choosing where the bool lives; if execution can start from DB state alone, an
in-memory bool is not enough and the verdict must be taken inside the per-file
step regardless.

A job with no `scan_id` (not a `doctor_apply`) never reaches the refresh at all,
so it needs no verdict; keep that path unchanged.

**T3 · Gate the refresh.** `refresh_snapshot_after_successful_doctor_write`
(`store.rs:283`) takes the verdict and returns `Ok(())` without touching the row
when it is false. The SQL itself stays exactly as it is.

Rewrite the function's doc comment. The sentence that today calls the freeze
deliberate — "fields last written by another actor, including the Tag Editor,
are deliberately left as this scan originally read them" — is precisely what
this task reverses; leaving it would leave the file arguing with itself.

**T4 · Guard #594.** Add `doc_1i_a_second_doctor_write_still_blesses_its_own_file`.
A single apply followed by a rescan is **not** enough — that passes even when T2
compares the wrong sides. The test needs a second write to the same file:

1. scan the file;
2. freeze a plan for field B **now**, before anything is applied;
3. apply field A from its own plan — this refreshes the snapshot identity;
4. apply the plan frozen in step 2.

Assert that after step 4 `stale_flags` still reports the track fresh, and that
field B is refreshed in the snapshot. This fails if the comparison is taken
against the plan-carried ref, and it fails if a later change over-corrects into
refusing the refresh unconditionally.

`doctor_apply_on_worker_connection_refreshes_snapshot_before_remaining_rows_are_classified`
(`snapshot_refresh_tests.rs:182`) covers the same ground from the worker side and
must stay green.

**T5 · The rulebook** (`docs/ux-rules.md`, three edits):

*DOC-1i* — flip to `[active]`, drop the `<!-- REVIEW: rule proposal -->` marker,
replace the body:

> **DOC-1i** [active] [core] — **A field written by another actor must not
> remain frozen when a later Doctor write refreshes the file identity.** A
> Doctor write refreshes the stored reading only when, at the moment that write
> begins, the file still matches the identity that stored reading carries. If
> anything changed
> the file in between — the Tag Editor, an external tagger, the scanner's own
> move path — the write leaves the stored reading and its identity untouched, so
> the next scan re-reads the file instead of skipping it. A file nothing else
> touched is refreshed exactly as DOC-1h describes. *Tests:*
> `doc_1i_a_field_another_actor_wrote_is_not_frozen_by_a_later_doctor_write`,
> `doc_1i_an_unchanged_file_still_keeps_its_doctor_write`.

*DOC-1h* — keep the ID and the body; append an amendment line in the italic form
the file already uses. **Date it the day the commit lands, not the day this plan
was written:**

> *Amended &lt;landing date&gt;: the refresh happens only when the file still
> matched the identity its stored reading carried when the write began; see
> DOC-1i. The
> guarantee is unchanged — a later scan never works from the pre-write value,
> because a file that changed under the scan is re-read instead of skipped.*

*DOC-1j* — new, directly after DOC-1i, carrying the
`<!-- REVIEW: rule proposal -->` marker. `DOC-1a` through `DOC-1i` are taken;
`DOC-1j` is the next free ID and IDs are append-only (`ux-rules.md:18`):

> **DOC-1j** [planned] [core] — **A Doctor write does not act on a reading the
> file has outgrown.** The per-field conflict check guards the field being
> written, so a plan frozen before another actor changed a *different* field
> still applies, with a justification derived from a reading that no longer
> holds. DOC-1i makes the snapshot honest about this; whether the write should
> happen at all is open.

**T6 · Gates.** The full battery plus the core purity proof — this is `[core]`.
T1, T4 and T5 land in the **same commit**: a rule switches to `[active]` in the
commit that proves it (`ux-rules.md:13`), and a half-implemented rule must be
split instead (`ux-rules.md:15`), which is exactly what a two-commit shape would
produce.

## Out of scope — deliberately

- **Refusing the write.** Recorded as DOC-1j above. It is a promise about what
  Reprise writes into the user's files, not about what it remembers, and it
  carries its own UX question, its own strings and its own measurement.
- **The Tag Editor.** It already reconciles identity correctly (E5); it simply
  does not know the Doctor's snapshot exists, and after this change it does not
  need to.
- **No migration.** `library_doctor_scan_tracks` is unchanged.

## Parallelität

**One strand.** The reason is the rulebook, not the file count: `ux-rules.md:13`
requires the rule to switch to `[active]` in the same commit that proves it, and
`ux-rules.md:15` forbids leaving it half-implemented across two. A cut along
"tests here, behaviour there, rule somewhere else" produces exactly the
forbidden intermediate state — after the first merge there would be either an
`[active]` rule with no implementation or an implementation with no rule.

The file groups are not disjoint either: T2 and T3 are two ends of one call
chain (`prepare_files` → `terminal_success` →
`refresh_snapshot_after_successful_doctor_write`), and T1's assertion is what
T3's early return has to satisfy.

Branch `feature/doc-1i-frozen-snapshot-field`, cut from `origin/dev`.

Owned files:

- `crates/reprise-core/src/library/library_doctor/write.rs`
- `crates/reprise-core/src/library/library_doctor/store.rs`
- `crates/reprise-core/src/library/library_doctor/snapshot_refresh_tests.rs`
- `docs/ux-rules.md` — the DOC-1h amendment line, the DOC-1i body, the new DOC-1j

No merge order and no post-merge cross-checks: there is no seam.

`docs/ux-rules.md` appears under the Flathub strand A in `AGENTS.md`, but that
table is historical — #1012 and #1019 both wrote the file since. No live
ownership claims it.

## Gates

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit                      # only RUSTSEC-2024-0436 accepted
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # must be empty
```
