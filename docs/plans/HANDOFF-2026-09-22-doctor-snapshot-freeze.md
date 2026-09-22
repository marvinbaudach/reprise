# Library Doctor freezes its own fixes — diagnosed 2026-09-22

Origin: the artist `REFORMIST` / `Reformist` appears in two spellings in the
same album (*Voyages*) and the Library Doctor never proposes a fix. The report
was about one artist; the cause is library-wide.

## Short version

The Doctor **created** the split itself, and **cannot see it** afterwards,
because a successful doctor write refreshes only the *file identity* of the
scan snapshot, not the tag columns. Every later scan therefore takes the
DOC-1g reuse path and carries the pre-write reading forward.

## Method

`cp ~/.local/share/reprise/reprise.db` plus its `-wal` into the session
scratchpad; every query ran against the copy with `?mode=ro`. The real DB was
never opened. File tags were read with `mid3v2` / `metaflac` / `mutagen-inspect`
(read-only). Nothing was written anywhere.

## Evidence chain

### 1. The split was manufactured by a partial apply

Seven tracks under `/home/marvin/Music/REFORMIST/`:

| track_id | title | tag file today | `tracks` row today |
|---:|---|---|---|
| 985 | Venomous (FLAC, single) | `ARTIST=REFORMIST` | REFORMIST |
| 2219 | The Crown (feat. …) | `TPE1=REFORMIST`, `TPE2=Reformist` | REFORMIST / album-artist Reformist |
| 2220–2224 | Voyages tracks | `TPE1=Reformist` | Reformist |

Before 2026-08-30 all seven read `REFORMIST` uniformly — visible in
`library_doctor_scan_tracks` for scans 1–4. The local rule
(`local_rules.rs:62`, `add_grouped_field`) had a single spelling per group and
correctly proposed nothing.

Scan 4 / job 22 (`doctor_apply`, 2026-08-30 17:11) then applied MusicBrainz
proposals `artist: REFORMIST → Reformist` for **2220–2224 only**:

- **2219** never received an `artist` proposal in any scan — only an
  `album_artist` one (scan 3, confidence 70), which *was* written. That is why
  its two fields disagree today.
- **985** received no proposal at all in any scan.

So the apply turned a uniform 7:0 into a 5:2 disagreement.

### 2. Every scan since is blind to it

`refresh_snapshot_after_successful_doctor_write`
(`crates/reprise-core/src/library/library_doctor/store.rs:279`) updates only

```
SET (path, file_mtime, file_size, device, inode) = (SELECT … FROM tracks …)
```

in `library_doctor_scan_tracks` — **not** `title/artist/album/album_artist/
year/track_no/genre`. Its own doc comment claims it refreshes "reconciled
`tracks` from the file it just read"; the SQL does not do that.

The next scan compares the `DoctorTrackRef` identity (`scan.rs:124–127`),
finds it unchanged, and takes the reuse path (`scan.rs:174–177`), which clones
the **previous snapshot's tags** instead of reading the file. Scans 5 → 6 → 7
cascade the frozen value. Scan 7 (2026-09-22 10:22, the reviewed one) holds
`REFORMIST` for all seven tracks and therefore has nothing to report.

### 3. Blast radius — 300 tracks, not one artist

Scan 7 snapshot vs. what the write journal says was actually written
(`tag_write_journal.before_value` / `after_value`, `outcome='applied'`,
`kind='doctor_apply'`):

| field | applied | snapshot still holds the pre-write value | snapshot holds the after-value |
|---|---:|---:|---:|
| artist | 81 | **81** | 0 |
| album_artist | 91 | **88** | 3 |
| title | 140 | **140** | 0 |
| album | 117 | **117** | 0 |

**300 distinct tracks** are affected. Effectively everything the Doctor has
ever written is remembered in its pre-fix state. The three `album_artist`
exceptions are consistent with files that were genuinely re-read later because
their identity changed for an unrelated reason.

### 4. Alternative hypothesis considered and refuted

*Could scan 7 have really re-read the files, with `Accessor::artist`
(`tag_edit.rs:126`) resolving to the `TXXX:ARTISTS` / Vorbis `ARTISTS` field,
which does carry `REFORMIST` in these files?* No:

- The freeze is **field-independent**: `title` is 140/140 stale and has no
  `ARTISTS` analogue at all. A reader-side confusion cannot explain that.
- The `tracks` table — populated by the normal scanner path — holds the
  correct post-write values, so the reader itself is fine.

The fault is the snapshot refresh, not the tag reader.

## The fix

**Location:** `store.rs:279`,
`refresh_snapshot_after_successful_doctor_write`. In the same transaction that
already runs there, also write the applied fields' `after_value` from
`tag_write_journal` into the snapshot row's tag columns. Only written fields
need updating; untouched fields stay valid. `stale_flags` must keep working —
the identity refresh exists for it and must not be removed.

**Likely tests pinning the current behaviour** (check before changing):

- `crates/reprise-core/src/library/library_doctor/snapshot_refresh_tests.rs:53`
  `doctor_apply_on_worker_connection_refreshes_snapshot_before_remaining_rows_are_classified`
- `crates/reprise-core/src/library/library_doctor/reuse_scan_tests.rs:161`
  `doc_1g_a_skipped_track_keeps_its_previous_proposals`

**Acceptance criterion (falsifiable):** after the fix, a fresh scan groups all
seven Reformist tracks under `normalize_group_key("reformist")`, counts
5 × `Reformist` against 2 × `REFORMIST`, and proposes `Reformist` for track
2219 and track 985 **locally**, with no remote lookup involved.

## Open — not covered by the fix

The fix is forward-looking. The 300 already-frozen snapshot rows in this real
library are not repaired retroactively: the reuse decision is made from the
identity columns, which are current. They unfreeze only when those files are
genuinely re-read. Options, in rising order of bluntness:

1. Invalidate the tag columns (or the affected snapshot rows) once, as a
   one-off, so the next scan re-reads exactly those 300 tracks.
2. Ship the fix and accept that the old rows stay frozen until each file
   changes on disk for another reason.

Decide this with the plan; it is a data question, not a code question.
Note that `may_reuse_readings` (`scan.rs:117`) already disables reuse whenever
remote was off in the previous scan — that is not a workaround for this case,
because the previous scans all had remote enabled.
