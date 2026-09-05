---
slug: every-album-gets-its-own-cover
worktree: /home/marvin/Projects/reprise-every-album-gets-its-own-cover
branch: feature/every-album-gets-its-own-cover
phase: refactored
codex_session:
created: 2026-09-05
---
# Every album gets its own cover

## The problem, as measured

`~/Music/Chelsea Grin/Chelsea Grin (2008)/` holds 88 files belonging to seven
different albums, and **every one of them carries the same embedded picture**
(md5 `2c7220fa…`, the *Eternal Nightmare* front cover). Verified by extracting
the embedded art from six files spanning five album tags — byte-identical in all
six. Reprise renders exactly what the tags contain; the display is correct and
the library is wrong.

The automatic download never steps in. `cover_download.rs:1-5` says it fetches
"when the local cover pipeline has no usable image", and a *wrong* embedded
picture is a usable one. Concretely: the startup batch calls
`cover_download_worker::result_for_path`
(`crates/reprise-gnome/src/ui/cover/cover_download_worker.rs:126-142`) with
`skip_if_covered: true`, `resolve_source` returns `CoverSource::Embedded(…)`,
`cover_status` answers `Covered`, and the worker returns `AlreadyCovered`
without ever reaching `fetch_and_cache`. That is why the cache holds 235
downloaded covers and none for Chelsea Grin.

A library-wide sweep (one track per album, embedded picture extracted and
hashed) puts numbers on it: **402 albums, all with embedded art, and 17 pictures
that appear under more than one album key** — 11 inside a single directory, 6
across directory boundaries:

```
a day to remember — Common Courtesy  ↔  Homesick
dead by april     — Break My Fall    ↔  Dead By April
ocean sleeper     — two singles
a foreign affair  — two singles
breaking benjamin/Ember ↔ bring me the horizon/Count Your Blessings
oceans ate alaska ↔ pvris  (both "Punk Goes Pop, Vol. 6")
```

The last one is a real compilation — one release, two album-artist values — and
it is the only false positive in the set.

## The decision

**Improve the download itself. No UI, no new user action.**

Two alternatives were considered and dropped during the grill:

- **A Library Doctor artwork category.** Its `DoctorValue` is
  `Empty | Text | Year` and `write.rs` applies a finding *only* by writing tags
  into the audio file through `commit_guarded_tag_changes`. There is no
  DB-or-cache-only path. Adding artwork means `Image(Vec<u8>)` through every
  encode/decode path, a second review-row factory, and a fix that rewrites the
  user's files — the one thing this task rules out. ~200–400 LoC of core rework
  to arrive at the rejected option.
- **A "get cover for this album again" context-menu action.** Once the rule
  below repairs *both* sides of a collision there is no blind spot left for a
  manual action to mop up, so it would buy only a retry button — `force`
  plumbing, a menu entry, a translated string, four toasts and a debounce
  question against the permanent negative marker, for a case nobody has hit yet.
  Left out under YAGNI; it stays available as a follow-up if a downloaded cover
  ever turns out worse than the embedded one.

## What the codebase already provides

Four existing mechanisms carry almost all the weight.

**1. A downloaded cover already outranks embedded artwork.**
`resolve_source_with_source` (`crates/reprise-core/src/cover.rs:82-104`) puts the
downloaded cache first, and its doc comment states the intent verbatim: *"A
shared downloaded source takes precedence so a detected album mismatch can
converge without modifying any audio file."* This case is that case. The
resolution order does not change.

**2. The user's files are protected by an existing guard.**
`cover_writeback::album_has_artwork` (`cover_writeback.rs:60-75`) refuses to
write `cover.jpg` into a directory that holds either a folder image or any track
with an embedded picture. Every affected directory has both. The writeback is
therefore a no-op here **without any new code**, the whole change stays inside
`~/.cache/reprise/covers/downloaded/`, and it is reversible by deleting a cache
entry.

**3. The matcher is already strict enough to be the safety net.**
`parse_best_release` (`cover_download.rs:196-258`) requires `score >= 90`
**and** normalized equality of both release title and artist credit. The
realistic outcomes are an exact match or `NoMatch → negative marker → nothing
changes`. That is what makes the compilation false positive above harmless.

**4. The detection rule exists already — in mirror image.** `cover_status`
(`cover_download_worker.rs:145-171`) keeps `observed_embedded: HashMap<album_key,
fingerprint>` and answers `NeedsSharedCover` when *one album* holds *different*
embedded pictures (BROWSE-10, routine for compilations). This case is the exact
mirror: *one picture* across *different albums*.

## The change

### 1. The mirror rule in `cover_status`

Add a second map beside `observed_embedded`, keyed the other way round:

```
fingerprint -> (album_key, album_artist, album)
```

**Library-wide, not per directory.** A per-directory map would miss the five
real cross-directory cases listed above; the single false positive it would
avoid is neutralised by the matcher anyway.

When a track's embedded fingerprint is already recorded under a *different*
album key, `cover_status` answers `NeedsSharedCover` **and carries the
previously seen album identity with it**, so the caller can act on both sides:

```rust
enum CoverStatus {
    Covered,
    NeedsSharedCover { also: Option<(String, String)> },  // (album_artist, album)
}
```

The existing BROWSE-10 answer (different pictures inside one album) keeps
`also: None`. `Covered` is unchanged.

**Both sides get fetched.** `result_for_path` calls `result_for_tag` for this
track's album and, when `also` is present, for the remembered album too.
Rationale: a picture belongs to at most one of the colliding albums, and nothing
in the tags says which — so the rule does not guess. It also means an album
whose embedded art was already correct can end up showing the Cover Art Archive
edition instead. That is accepted, and it is reversible.

Without the `also` payload the first album key seen for a given fingerprint
would keep the wrong picture forever: `query_live_track_paths` sorts
`ORDER BY path` (`crates/reprise-core/src/queries/maintenance.rs:179`), so the
order is deterministic, and once the *other* albums resolve through stage 1 they
never re-enter `cover_status`. For Chelsea Grin the permanently wrong album
would be the self-titled EP (first track `01 Crewcabanger.mp3`).

`attempted` (`result_for_tag`) already dedupes by album key, so the extra call
costs no extra network request when the album has been handled.

### 2. Making the change reach an already-settled library

Both gates in front of the batch were written under the **old** rule and would
keep this change away from an existing library:

- `outcome_settles_track` (`cover_download_batch.rs:355`) counts `AlreadyCovered`
  as settled, so every affected track carries `download_settled` in its
  resolution index and `open_paths` filters it out of the next pass.
- `exact_signature_decision` (`startup_tasks.rs:291`) skips the whole batch while
  the library revision is unchanged.

So the change ships with a one-time invalidation of both:

- **`RESOLUTION_FORMAT` 2 → 3** (`cover.rs:376`). This is exactly what the
  constant is for — *"bumped whenever an index entry's meaning changes"* — and
  the meaning does change: a `download_settled` entry written under the old rule
  is no longer a valid answer. Accepted cost: the first launch after the update
  re-reads tags for the whole library once instead of three `stat` calls per
  track.
- **A migration deleting the `CoverDownload` task record** (settings key
  `startup_tasks.covers`, `SignatureTask::key()` at `startup_tasks.rs:39-45`), so
  `exact_signature_decision` returns `NeverCompleted` and the batch runs once.

## Tasks

1. `CoverStatus::NeedsSharedCover { also }` plus the reverse map in
   `cover_status`. Pure function, no network.
2. `result_for_path` acts on both sides of a collision.
3. `RESOLUTION_FORMAT` 2 → 3.
4. Migration deleting the `CoverDownload` startup-task record.
5. Tests (see the seam note below).

## Verification

**Unit tests go on `cover_status`, never through `result_for_tag`.**
`cover_status` is a pure function over `(&CoverTag, &CoverSource, &mut maps)`;
`result_for_tag` calls the real `fetch_and_cache`, which performs live
MusicBrainz and Cover Art Archive requests. Only `fetch_and_cache_with` takes
injectable fetchers. A test written at the worker level passes locally and hangs
in CI — do not write one.

Cases:
- same picture under two album keys → `NeedsSharedCover` with `also` naming the
  first album;
- same picture under one album key → `Covered` (BROWSE-10 must not regress);
- different pictures under one album key → `NeedsSharedCover { also: None }`
  (existing behaviour);
- a track without album/album-artist → `Covered` (existing early return).

Plus `cargo test -p reprise-core -p reprise-gnome`.

**The real measurement is the user's library, and its baseline is already
recorded.** Extracting the embedded picture from one track per album across the
29 albums in the five worst directories gives **6 distinct covers** today:

| Directory | Albums | Distinct covers |
|---|---|---|
| `Emmure/Speaker Of The Dead (2011)/` | 9 | 1 |
| `Chelsea Grin/Chelsea Grin (2008)/` | 7 | 1 |
| `Suicide Silence/The Black Crown (2011)/` | 5 | 2 |
| `Asking Alexandria/Stand Up And Scream (2009)/` | 4 | 1 |
| `Oceano/Revelation (2017)/` | 4 | 1 |

After the change, record what each of the 29 albums *resolves* to
(`thumbnail_for_track`, hashed). The counting rule is the number of distinct
hashes among the 29: **6 before**, up to 29 after, fewer wherever MusicBrainz
legitimately has no match. Report the actual number — an unmatched album is not
a failure of this change.

**The load-bearing check:** no file under `~/Music/` may be modified. Stamp a
reference file before the run and assert `find ~/Music -newer <stamp>` comes back
empty.

## Out of scope

Rewriting the embedded pictures in the user's audio files. The downloaded-cover
stage fixes the display without touching the library, and `album_has_artwork`
already refuses the writeback here.

## Parallelität

**No cut — one strand.** Tasks 1 and 2 both edit
`crates/reprise-gnome/src/ui/cover/cover_download_worker.rs`; tasks 3 and 4 are a
constant and a migration whose only purpose is to make tasks 1–2 reach an
existing library, so they cannot be verified apart from them.

A "core vs. UI" cut was considered and rejected: there is no UI half left after
the grill dropped the context-menu action, and the remaining work is four edits
across three files with a shared verification. Splitting it would put
`cover_download_worker.rs` in two groups — exactly the overlap the disjointness
check exists to catch.

No post-merge cross-checks: with one strand, every verification step reads files
that strand owns.

---

## Measured outcome (2026-09-05)

Run against a `.backup` copy of the real library (243 MB) plus a copy of the
real cover cache (212 downloaded covers, 23 negative markers), isolated via
`XDG_DATA_HOME`/`XDG_CACHE_HOME`, app headless under a virtual X server.

**A private session bus is mandatory.** Without `dbus-run-session`,
GApplication's single-instance registration finds the user's real running
Reprise, hands the launch over to it and exits — the first attempt produced
"+0 covers", which reads exactly like "the fix does not work".

**Control arm:** the same measurement in the isolated copy *before* the run was
byte-identical to the real environment (29 tracks, 6 distinct covers), so the
copy does not distort the result. The measuring tool resolves through
`cover::resolve_source` — the same path the app uses — and independently
reproduced the baseline that an unrelated `ffmpeg` sweep had produced.

**Result: 6 → 21 distinct covers** across the 29 albums. 20 albums now resolve to
a downloaded cover, 8 stay embedded, 1 resolves to a folder image.

| Directory | before | after |
|---|---|---|
| Emmure | 1 | 9 of 9 |
| Chelsea Grin | 1 | 5 of 7 |
| Oceano | 1 | 3 of 4 |
| Asking Alexandria | 1 | 3 of 4 |
| Suicide Silence | 2 | 0 of 5 |

The Chelsea Grin self-titled EP — the blind spot the `also` payload was designed
to close — did get its own cover. That mechanism works.

**No stale negative marker blocked anything**: all six albums that ended with a
marker were probed during this run. The one-time invalidation
(`RESOLUTION_FORMAT` 3 + the v83 migration) therefore does reach a settled
library. Six misses are the strict matcher working as designed (`Evolve
[Explicit]`, `Self Inflicted (Deluxe Edition)`, `No Time to Bleed (Bonus Track
Version)` — titles MusicBrainz does not carry in that form). That `No Time To
Bleed` and `The Black Crown` also missed without any suffix is unexplained and
worth a look.

### Open residual gap — the mirror of finding 1, on the primary side

Three albums were never probed at all. One is explained: `The Black Crown (2011)`
has no embedded picture, resolves through the folder image, and so belongs to no
collision. Two are not: `Punk Goes Pop 3` and `Ending Is The Beginning`.

A second pass proved nothing about them — it skipped the batch entirely
("startup task skipped: library signature unchanged"), because the first pass had
written the completion record.

By code inspection the gap is nevertheless real: `cover_status` returns `Covered`
on the `CoverSource::FolderImage` branch before the fingerprint logic runs. Once
the other albums of a collision group have their downloaded covers, they resolve
through stage 1 and never re-enter the map — so an album whose own fetch ended in
`TransientFailure` (which writes no marker) has lost its trigger permanently.
`fetch_collision_pair` protects the mirror side by aborting before publishing the
primary; the reverse case — primary failing after the mirror succeeded — has no
such protection. Whether the two albums above are in that state is unproven.

### Second measurement — the finished four-commit version (2026-09-05, later)

Fresh copy of the same library and cache (baseline re-confirmed at 6 distinct
covers before the run), app run again under the same isolation.

**Result: 6 → 20 distinct covers**, 19 albums resolving to a downloaded cover.
The first measurement, against the half-finished version, gave 21 / 20. Both are
single runs of a network-dependent process, so **the difference between 20 and 21
is not established as an effect of the change** — MusicBrainz and Cover Art
Archive availability varies between runs, and n=1 per arm cannot separate the two.

Of the nine albums still not resolving to a downloaded cover:

- five carry a **fresh** negative marker, i.e. they were probed this run and
  genuinely not matched (`Evolve [Explicit]`, `Self Inflicted (Deluxe Edition)`,
  `No Time To Bleed`, `No Time to Bleed (Bonus Track Version)`, `The Black
  Crown`);
- one is explained by design (`The Black Crown (2011)` has no embedded picture,
  resolves through the folder image, belongs to no collision);
- three have neither cover nor marker (`Punk Goes Pop 3`, `Ascendants`,
  `Incisions`) — the signature of a `TransientFailure`, which deliberately
  records nothing.

A methodology note worth keeping: classifying markers as new-vs-stale needs the
cutoff set to the run's *start*. A cutoff at the run's end labelled its own fresh
markers "stale" and produced a completely wrong reading on the first attempt.

### The deferral has no automatic second chance

`fetch_collision_pair` turns a transient failure into "publish nothing, retry on a
later pass". Collision detection now survives (commit 4), so a later pass *would*
re-detect it — but there may be no later pass: `exact_signature_decision`
(`startup_tasks.rs:291`) skips the whole cover batch on the next launch while the
library signature is unchanged, which is exactly the situation after a completed
pass. Observed directly: a second launch logged `startup task skipped: library
signature unchanged  task="cover batch"` and did no work at all.

So a transiently-failed album waits until the library changes, or until the user
triggers a pass by hand — `start_user_triggered` exists and is wired to the
cover-download progress card and to granting the artwork permission
(`main_cover_download_progress.rs:227`, `window_artwork_permission_wiring.rs:27`).
Not lost, but not automatic either. Whether that is good enough is a product
decision, not a defect of this branch.

### Isolation held

Verified after the runs: the real cache still holds exactly the 212 downloaded
covers and 23 negative markers it held before, and no file under `~/Music` was
written. Everything happened inside the isolated `XDG` tree.
