---
slug: android-reads-during-scan
worktree: /home/marvin/Projects/reprise-android-reads-during-scan
branch: feature/android-reads-during-scan
phase: coded
codex_session:
created: 2026-08-14
---
# Android: the library stays readable while a scan walks the tree

Read against `5721ade95e` (`origin/dev`, fetched 2026-08-14 10:22). Every claim
below carries its file and line, checked against that tree.

This plan comes from the architecture review of the Android app on 2026-08-14,
point 1 — the root finding underneath the executor sprawl. It was grilled the
same day; §8 records what the grill settled and why, including the three things
it deliberately left undone.

## Rules for the implementer — read first

**No device, no emulator, no `adb`, no `cua-driver`.** Every verification here
runs on this host. The environment, checked:

```bash
export ANDROID_HOME=/home/marvin/.local/share/android-sdk   # SDK: platforms, build-tools
export ANDROID_NDK_HOME=/opt/android-ndk                    # NDK (also at $ANDROID_HOME's sibling /opt/android-sdk/ndk/29.0.14206865)
```

`ANDROID_HOME` is **not** set in the shell, and `scripts/android-build.sh:17`
guesses `$HOME/Android/Sdk`, which does not exist — set it per invocation. Do
**not** point it at `/opt/android-sdk`: that directory contains only `ndk/`, no
`platforms/` and no `build-tools/`, and Gradle then fails with an error that
looks like a broken project rather than a missing SDK. Write
`sdk.dir=/home/marvin/.local/share/android-sdk` into `android/local.properties`
in the worktree — the file is gitignored and absent in a fresh worktree.

**The file lists are entry points, not a fence.** Each task names where the
change starts. Touch whatever the contract genuinely requires and name the extra
files in the commit message. Stop only if the *contract* is wrong, never because
a path is missing from a list.

**Red before green, per task** (`AGENTS.md`, "How to resume", point 2). A test
that has never been seen to fail proves nothing here — the subject of this plan
is a concurrency property, and concurrency tests pass by accident all the time.

**Every mutation proof must hit production code.** A one-token change is
acceptable and expected — which connection a method takes is an identifier, and
there is no honest single-character form of that. A mutation applied to a *test*
file is not a proof of anything and does not count.

**A hanging test is not a red test.** Everything here that waits, waits with a
deadline and then *asserts*. Nothing may deadlock a `cargo test` run.

---

## 1. The finding, checked against the code

The review's point 1 is substantially right. The exact state of `origin/dev`:

- `MusicLibrary` holds everything behind one mutex —
  `crates/reprise-android-ffi/src/library_types.rs:23-28`, with
  `state: Mutex<LibraryState>` at `:25` and `LibraryState { db, tree }` at
  `:18-21`.
- `MusicLibrary::lock()` is the single accessor —
  `crates/reprise-android-ffi/src/library_types.rs:30-39`; it maps a poisoned
  mutex to `LibraryError::Database` rather than propagating a panic over the FFI
  boundary.
- `scan` takes that lock at `crates/reprise-android-ffi/src/lib.rs:118`, before
  `scan_folder_with_source_and_progress` at `:120`, and holds it until the
  method returns at `:142`. **Correction to the finding:** the method spans
  `:114-142`, not `:114-141`.
- SQLite is in WAL — `crates/reprise-core/src/db.rs:60`, inside
  `open_with_options`, which every `Db` open goes through
  (`crates/reprise-core/src/db.rs:37-64`). `busy_timeout` is 5000 ms by default
  (`crates/reprise-core/src/db.rs:35`, applied at `:62`).
- The Kotlin side names the defect in five places, not one:
  - `android/app/src/main/java/de/reprise/spike/MobileSurfaceViewModel.kt:67-68`
    ("the read that takes the lock a scan holds for its whole walk"), inside the
    doc block `:64-69`. **Correction to the finding:** the sentence is at
    `:67-68`, the block at `:64-69`.
  - `android/app/src/main/java/de/reprise/spike/MainActivity.kt:89-90`
  - `android/app/src/main/java/de/reprise/spike/BrowseScreen.kt:416-419`
  - `android/app/src/main/java/de/reprise/spike/TrackLoader.kt:32-36`
  - `android/app/src/main/java/de/reprise/spike/RatingWriter.kt:24-28`

### 1.1 Every `MusicLibrary::lock()` call site, classified

28 call sites take the lock (plus the accessor itself at
`library_types.rs:35`). Grep:
`rg -n 'self\.lock\(\)' crates/reprise-android-ffi/src` — note that
`playback_session.rs`, `playback_session/*.rs` and `visualizer.rs` have their own
unrelated mutexes and are **not** in this list.

**Pure reads — 16 sites, 16 methods. These are what this plan moves.**

| Method | Site | Reads |
|---|---|---|
| `list_tracks` | `lib.rs:145` | `queries::query_library_text_search` |
| `search_albums` | `lib.rs:158` | `queries::query_albums` |
| `search_artists` (and `list_artists`, `lib.rs:166-168`) | `lib.rs:175` | `queries::query_artists` |
| `list_album_tracks` | `lib.rs:191` | `queries::query_album_tracks` |
| `search_tracks` | `lib.rs:204` | `queries::query_library_metadata_text_search` |
| `track_by_id` | `browse.rs:170` | `queries::query_present_track_by_id` |
| `album_track_ids` | `browse.rs:186` | `queries::query_album_canonical_track_ids` |
| `list_artist_albums` | `filtered_browse.rs:17` | `queries::query_artist_albums` |
| `list_artist_untagged_tracks` | `filtered_browse.rs:32` | `queries::query_artist_untagged_tracks` |
| `list_artist_tracks` | `filtered_browse.rs:46` | `queries::query_library_tracks` |
| `appearance_settings` | `appearance.rs:159` | `settings::get_setting`, `settings::get_color_scheme` |
| `visualizer_setting` | `appearance.rs:181` | `settings::get_setting` |
| `library_destination_setting` | `appearance.rs:202` | `settings::get_setting` |
| `playback_settings` | `playback_settings.rs:87` | `AndroidPlaybackSettings::load` |
| `track_spectrogram` | `track_analysis.rs:43` | `query_present_track_by_id`, `get_track_spectrogram` |
| `track_render_bars` | `track_analysis.rs:66` | `query_present_track_by_id`, `get_waveform_peaks`, `get_track_spectrogram` |

**Writes — 9 sites. These stay exactly where they are.**

| Method | Site | Writes |
|---|---|---|
| `set_tree_uri` | `lib.rs:101` | `settings::set_library_root` + replaces `state.tree` |
| `scan` | `lib.rs:118` | the whole walk |
| `set_theme` | `appearance.rs:172` | `settings::set_setting` |
| `set_visualizer` | `appearance.rs:191` | `settings::set_setting` |
| `set_library_destination` | `appearance.rs:218` | `settings::set_setting` |
| `set_equalizer_enabled` | `playback_settings.rs:92` | `settings::set_equalizer_enabled` |
| `replace_equalizer_curve` | `playback_settings.rs:103` | `settings::set_equalizer_curve` |
| `set_gapless_enabled` | `playback_settings.rs:108` | `settings::set_gapless_enabled` |
| `set_track_rating` | `library_listen_report.rs:10` | reads `device_path_for_track` **then** `set_rating_at_if_present` — one path, one handle |

**Tree only — 1 site.** `track_artwork` at `lib.rs:243` takes the lock purely to
clone `Arc<BridgedSource>` out of `state.tree` (`lib.rs:242-246`) and drops it
before the expensive decode. It never touches `state.db`. It is still shut out
during a scan — not because it *holds* anything, but because it cannot
*acquire* anything. The finding lists "artwork paths" among the blocked reads;
that is correct, but for a different reason than the rest, and it needs a
different fix. **This classification is true of `origin/dev` only — see §7.**

**Mixed — 2 sites, one method.** `import_track_analysis` (`mobile_sync.rs:7-37`)
takes the lock at `:9` for the tree and the sidecar path, drops it at `:17`,
reads the sidecar through the source with nothing held, then retakes it at `:28`
to write. It is a write path.

**No lock at all — 1 method.** `prepare_listen_report`
(`library_listen_report.rs:46-57`) goes to the journal file next to the
database, not to SQLite.

### 1.2 What the finding understates

> "The serialisation is purely self-inflicted."

It is self-inflicted, but relaxing the Rust mutex is not sufficient and cannot be
made sufficient. `rusqlite::Connection` is `Send` but **not** `Sync` —
`unsafe impl Send for Connection {}` at
`~/.cargo/registry/src/index.crates.io-*/rusqlite-0.40.1/src/lib.rs:364`, and the
struct at `:355-360` holds `db: RefCell<InnerConnection>`, which is what takes
`Sync` away. `Db` wraps exactly that `Connection`
(`crates/reprise-core/src/db_handle.rs:21-23`) and adds no `unsafe impl`. Its own
documentation already says so: "Construct one per process that talks to the
library, plus one per worker thread — a `Connection` is not `Sync`"
(`crates/reprise-core/src/db_handle.rs:18-20`).

So "let readers share the handle" is not on the table at any lock flavour. A
second reader needs a second connection. That is the whole design decision.

---

## 2. The shape

### D1 — a second `Db` over the same file, for reads only. Chosen.

`MusicLibrary` opens a second handle in its constructor and routes the 16 pure
reads through it. The writer mutex keeps its job — serialising the scan against
every other write in this object — and loses the job it should never have had.

**Why the alternatives lose.**

**`RwLock<LibraryState>` — does not compile.** `std::sync::RwLock<T>: Sync`
requires `T: Send + Sync`; `Mutex<T>: Sync` requires only `T: Send`. That is
precisely why the current code is a `Mutex`. `LibraryState` contains `Db`, which
is `!Sync` (§1.2), so `RwLock<LibraryState>` is `!Sync`, and
`#[derive(uniffi::Object)]` emits `assert_impl_all!(MusicLibrary: Sync, Send)` —
`~/.cargo/registry/src/index.crates.io-*/uniffi_macros-0.32.0/src/object.rs:161-168`.
The build fails at the derive. And even if it compiled it would buy nothing: one
`Connection` cannot serve two `SELECT`s at once.

**Shortening the scan's lock hold (per-batch, or a scan-owned writer connection)
— rejected, and this was the close call.** The strongest version is: `scan` opens
its own `Db::open_ready(&self.database_path)` and never takes the object mutex
during the walk. That is a one-method change and touches zero read call sites —
genuinely attractive. It loses on one concrete regression: the foreground writes
(`set_track_rating` at `library_listen_report.rs:9-40`, `set_theme`, the
equalizer setters) would then meet the scan's transaction at the SQLite layer
instead of at the Rust mutex. Today they block on the mutex for the whole walk
and then **succeed**. After that change they would block inside SQLite for
`busy_timeout` (5000 ms, `crates/reprise-core/src/db.rs:35`) and then **fail**
with `SQLITE_BUSY`. This crate already documents that exact failure and what it
costs: `crates/reprise-android-ffi/src/play_recorder.rs:17-26` —
"`MusicLibrary::scan` wraps its entire folder walk in **one** transaction, and an
SAF walk over a large tree easily runs longer than the five-second
`busy_timeout` … which is exactly the moment a user is most likely to press
play." Play counting paid for that with a journal, a retry ladder
(`play_recorder.rs:79-90`) and a worker thread. A heart tap has no journal.

D1 changes write behaviour by exactly nothing. That is deliberate, and it is
also the plan's honest limit — see §2.1.

**Splitting `LibraryState` — not an alternative, a *required part* of D1.** See
D2. `track_artwork` reads no database at all and is still shut out.

### 2.1 What this plan does not fix, stated plainly

**Writes still wait for the whole walk.** A heart tap during a scan is answered
late — potentially minutes late on a large tree. It is not *lost* and it does not
freeze the surface: `RatingWriter` owns a single background thread and answers
every tap exactly once through `report`
(`android/app/src/main/java/de/reprise/spike/RatingWriter.kt:21-39`). But the
heart does not move until the scan finishes.

That is the *same* behaviour as today, not a regression, and improving it means
either accepting `SQLITE_BUSY` failures (rejected above) or splitting the scan's
transaction in `reprise-core` (`scanner.rs:298` … `:689`), which has desktop
consequences and needs its own plan. **Recorded as follow-up F1 (§9).**

### D2 — the configured tree comes out of the writer mutex too.

`LibraryState` has two fields (`library_types.rs:18-21`) and they have nothing to
do with each other. `tree` is read by `scan` (`lib.rs:119`), `track_artwork`
(`lib.rs:244`) and `import_track_analysis` (`mobile_sync.rs:10`), and written only
by `set_tree_uri` (`lib.rs:107`) — four sites total
(`rg -n '\.tree\b' crates/reprise-android-ffi/src`). Every reader of it wants one
pointer clone and one `PathBuf`.

Without D2, D1 fixes the track list and leaves every cover in it blank for the
duration of the scan. `track_artwork` is the read the browse surface issues most
often. So:

```
MusicLibrary {
    writer:        Mutex<Db>,                     // was state.db
    reader:        Mutex<Db>,                     // new
    tree:          Mutex<Option<ConfiguredTree>>, // was state.tree
    cache_root:    PathBuf,
    database_path: PathBuf,
}
```

`LibraryState` disappears. `Mutex` rather than `RwLock` for `tree`: it is held
for an `Arc::clone` and a `PathBuf::clone` and nothing else, so a reader-writer
lock buys nothing measurable. (`ConfiguredTree` *would* satisfy `Sync` —
`dyn SafSource: Send + Sync` at `source.rs:61`, `SourceNames` is three
`Mutex<HashMap<…>>` at `source_names.rs:11-15` — so `RwLock` is available if a
later measurement ever wants it. It does not today.)

### D3 — lock order: `writer` before `tree`. Nothing else holds two at once.

`scan` must take `writer` **first**, then `tree`, clone, drop `tree`, and keep
`writer` for the walk. Not the other way round: if `scan` released `tree` before
taking `writer`, a `set_tree_uri` could slip between them and the scan would walk
the old root while the database records the new one — a race that does not exist
today. Taking `writer` first restores today's mutual exclusion exactly, because
`set_tree_uri` also needs `writer`.

`track_artwork` takes only `tree`. `import_track_analysis` takes `writer` (tree +
sidecar path), releases, reads the file, retakes `writer`. `reader` is never held
together with either. No cycle exists.

### D4 — the classification rule, in one sentence a reviewer can check.

> **A method that writes anything uses the writer for everything it does,
> including its own reads. Only a method that writes nothing moves to the
> reader.**

This is not fastidiousness. `set_track_rating` reads `device_path_for_track` and
then writes `set_rating_at_if_present` on the same row
(`library_listen_report.rs:11-26`); routing the read to the other connection
would put the check and the write in two different snapshots. The rule removes
the misclassification risk that is D1's main cost, and it is checkable by reading
a method top to bottom.

### D5 — `Db::open_ready`, not `open_ready_read_only`.

`Db::open_ready` (`crates/reprise-core/src/db_handle.rs:62-65`) opens through
`db::open`, so the reader gets WAL, `foreign_keys=ON` and `busy_timeout=5000`
(`crates/reprise-core/src/db.rs:60-62`), and it runs no migrations — it only
asserts the schema is already at `SUPPORTED_SCHEMA_VERSION` (`db_handle.rs:78-93`).
It is the exact call this crate already makes from a background thread three
times: `play_recorder.rs:228`, `listen_export_recorder.rs:73`,
`playback_session.rs:723`.

`Db::open_ready_read_only` (`db_handle.rs:73-76`) is tempting — a connection that
is *physically* unable to write would enforce D4 at the OS level. Rejected here
for two checkable reasons: (a) it calls `Connection::open_with_flags` directly and
therefore never reaches `db::open_with_options`, so it carries **no**
`busy_timeout` and no `foreign_keys` pragma; adopting it means changing
`reprise-core` for every other caller
(`crates/reprise-gnome/src/ui/cover/cover_download_worker.rs:117`,
`crates/reprise-gnome/src/ui/preferences/preference_rhythmbox.rs:406`); and (b)
opening a WAL database with `SQLITE_OPEN_READ_ONLY` depends on the `-shm` file
being writable, which is an Android-filesystem question no host run can settle.
**Follow-up F2 (§9)**, with a device proof attached.

### D6 — one reader connection, not a pool. Poisoning gets finer.

One connection means reads still serialise **against each other**, exactly as
they do today. That is not a regression, and read parallelism is not this plan's
goal: the blockade that hurts is the scan — minutes against milliseconds. Two
reads queueing for the length of one indexed `SELECT` has never been measured to
be a problem. If it ever is, the field becomes a small pool and **no call site
changes**. **Follow-up F3 (§9)** records the measurement to take after the
repository/coroutine rework, since that is the change that would first make
concurrent reads plausible.

Poisoning: today one panic under `state` kills reads and writes alike. After the
split, a panic under `writer` leaves reads working — a library you can browse but
not change beats a dead surface, and every method still reports the same
`LibraryError::Database` it reports today. This changes the meaning of one
existing test; Task 3 step 5 re-points it deliberately rather than letting it
pass for the wrong reason.

---

## 3. The hazards, each with the line that makes it real

**H1 — `SQLITE_BUSY` on the reader while the scanner commits.** In WAL a reader
does not block on a writer and does not receive `SQLITE_BUSY` from one; that is
the property `crates/reprise-core/src/db.rs:28-35` is written to exploit. The
residual exposures are WAL recovery after a crash and a *blocking* checkpoint;
SQLite's automatic checkpoint is PASSIVE and does not block readers, and the
scan's single transaction (`crates/reprise-core/src/library/scanner.rs:298`,
committed at `:689`) prevents any checkpoint at all while it runs. The reader's
`busy_timeout` of 5000 ms (D5) bounds whatever is left. **Nothing new is
needed**, and Task 1's test asserts `Ok(_)`, which is exactly the assertion a
`SQLITE_BUSY` would break.

**H2 — a long-lived reader pinning the WAL.** It cannot happen: no read method
opens a transaction, so every read is a per-statement implicit transaction that
ends before the FFI call returns. The reader holds a WAL read-mark for
microseconds at a time.

**H3 — what a reader sees mid-scan.** The scan is **one** transaction for the
whole walk — `let tx = conn.unchecked_transaction()?` at `scanner.rs:298`,
`tx.commit()?` at `:689`, with the vanish-reconcile folded into the same
transaction on purpose (`scanner.rs:170-191`). So under WAL a reader sees the
library **exactly as it was before the scan started**, and then, in one step, the
finished library. There is no half-scanned view to be uncomfortable about. This
is the good outcome and it must be asserted, not assumed — Task 1's second test.

**H4 — total and rows can straddle the scan's commit. Accepted, not fixed.**
`query_library_tracks` runs its count and its page as two statements —
`crates/reprise-core/src/queries/surface_browse.rs:103` and `:104` — so a read
that begins before the scan commits and ends after it can return a `total` from
the old snapshot and `rows` from the new one. Today that interleaving is
impossible for foreground reads, because the mutex blocks them entirely. **This
plan introduces it deliberately**, and that is the price of a browse surface that
shows anything at all during a scan.

Scope of the damage: the rows themselves are correct; `TrackWindow.total` and
`has_more` (`browse.rs:71-76`) can disagree with them for exactly one refresh.
Note the second-order effect, because it is not obvious: `LibraryCatalogShape`
compares `titles.total`
(`android/app/src/main/java/de/reprise/spike/MobileSurfaceViewModel.kt:88-93`) —
the very field that can be stale — so a stale count reads as "shape unchanged"
and the paged rows are *kept* rather than dropped. Keeping correct rows is the
harmless direction; the visible defect is a wrong `has_more` for one refresh.
`LibrarySession.scanTree` re-reads the whole browse state after the scan returns
(`android/app/src/main/java/de/reprise/spike/LibrarySession.kt:224-231`), so it
self-heals at scan completion at the latest.

Fixing it means an explicit read transaction, and `Db` deliberately does not
expose one — `conn()` is `pub(crate)` and the type's doc comment says why
(`crates/reprise-core/src/db_handle.rs:1-10`). **Follow-up F4 (§9)**; do not
build it here, and do not touch `reprise-core`.

**H5 — migrations and a second connection.** `Db::open_ready` runs no migrations
and refuses a schema that is not exactly `SUPPORTED_SCHEMA_VERSION`
(`db_handle.rs:78-93`); each migration step commits its own transaction and bumps
`user_version` inside it (`crates/reprise-core/src/db.rs:546-559` and the steps
after it), so no connection ever observes a torn schema. The reader is opened in
`MusicLibrary::open` immediately after that object's own `Db::open_migrated`
(`lib.rs:86`), on the same thread, so `user_version` is already at the supported
value and cannot go backwards. **Ordering is load-bearing** — writer first,
reader second — and reversing the two lines makes every test in the crate fail on
a fresh database with `SchemaNotReady`.

**H6 — the second native handle already in the process.** `MainActivity` opens
`MusicLibrary.open` (`MainActivity.kt:62`) and `ReprisePlaybackService` opens
`AndroidPlaybackSession` independently (`ReprisePlaybackService.kt:141`). That
session already opens `Db::open_migrated` (`playback_session.rs:530`), holds it as
`database: Mutex<Db>` (`playback_session.rs:553`), spawns a `PlayRecorder` whose
thread opens its own `Db::open_ready` (`play_recorder.rs:228`) and a
`ListenExportRecorder` that does the same (`listen_export_recorder.rs:73`), and
opens a transient one on every `reload_playback_settings`
(`playback_session.rs:723`). **This process already runs five SQLite connections
over one file.** This plan makes it six.

What that means for the consolidation this plan must not do: it makes it
*easier*, not harder. The consolidation's job is to remove duplicate handles;
after this change the surviving object already expresses the WAL role split — one
writer mutex, reads on their own connection — so the consolidation deletes
handles instead of having to invent the split first. The costs are one extra file
descriptor and one extra WAL reader slot per `MusicLibrary`, noise next to the
five already there. The one thing this plan must not do is add a **second
migrating** handle; it does not (D5).

**H7 — UniFFI object rules.** `#[derive(uniffi::Object)]` requires `Send + Sync`
(`uniffi_macros-0.32.0/src/object.rs:161-168`). All three new fields are
`Mutex<T>` with `T: Send`, so `MusicLibrary` stays `Sync` — the same reason
`RwLock` is unavailable (D1). Nothing about the exported surface changes: no
signature, no record, no error variant, so the generated Kotlin is
byte-identical. **Task 5 proves that**, because if the bindings changed the
Kotlin side would need a matching change and §6 claims it does not.

**H8 — teardown.** The Kotlin side closes the handle in `MainActivity.onDestroy`
(`MainActivity.kt:430-432`), and the bindings free the Rust object only after the
last in-flight call returns (`MainActivity.kt:417-422`). The reader is a field of
the same object, so it is dropped with it. Nothing new to close, no Kotlin
change.

---

## 4. Tasks

Standing gates after every task:

```bash
cargo fmt --check
cargo clippy --locked --all-targets --workspace -- -D warnings
scripts/check-architecture.sh
```

Note what is deliberately **not** in that list: `cargo test --locked --workspace`.
This change lives in one crate. The workspace run drags in the display suite,
which is red on `dev` for reasons that have nothing to do with this branch, and
handing the implementer a red it cannot fix is how a run gets stuck. The full
workspace is CI's job behind the merge; the Android gates run there since #471.

### Task 0 — record the baseline

Run `cargo test -p reprise-android-ffi` on the untouched branch base and write
down the exact `test result:` line. Every later task quotes its delta against
that number, not against a number in this document.

**Verify:** the quoted line, with the commit it was measured at.

---

### Task 1 — the red proof, before any production change

**Entry point:** new file
`crates/reprise-android-ffi/src/read_during_scan_tests.rs`, registered in
`crates/reprise-android-ffi/src/lib.rs` beside the other `#[cfg(test)] mod` lines
(`lib.rs:12-13`, `:20-21`, `:23-24`, `:37-38`). A new file rather than
`lib_tests.rs`, which is already 610 lines against the 800-line rule
(`scripts/check-architecture.sh:16-23`).

**The mechanism, and why it is deterministic.** The test drives the overlap
through the `SafSource` callback, which the scanner calls *inside* the
transaction and therefore *inside* the writer's lock hold. Not a new trick in
this crate: `mobile_sync.rs:120-142` already asserts a lock property from inside
`open_read_fd`, with the message "the SAF sidecar read must not hold the app-wide
library lock" (`:127`).

```
BlockingSource::list_children(root)
  ├─ first call only (guarded by an AtomicBool; `estimated_audio_files`
  │  reads the database and never walks the source —
  │  crates/reprise-core/src/library/scanner_progress.rs:36-57 — so this
  │  fires exactly once, in the walk)
  ├─ signal `scan_is_inside`  → the reader thread starts
  ├─ wait for the reader's answer, `recv_timeout(READER_DEADLINE)`
  │     • answer arrives → record it, return the children
  │     • deadline elapses → record `None`, return the children anyway
  └─ the scan finishes normally either way
```

The overlap is **structural**, not timed: the scan does not proceed past
`list_children` until the reader has either answered or blown the deadline, so
there is no window to miss. The only clock is the failure detector.

**`READER_DEADLINE = 120 s`**, and the reason is worth a comment in the file: on
the broken code the reader blocks on the mutex and never answers, so a deadline
is the only way to tell "never" from "in a moment" — but on the fixed code the
answer arrives in microseconds and the deadline is never approached. The red run
therefore costs two minutes once; a green line that flips red under a loaded
parallel `cargo test` would cost a wrong-place investigation every time. This
crate has already produced one load-flake (#468). Do **not** put a `sleep`
anywhere in this file.

Two tests:

- `a_browse_read_completes_while_a_scan_holds_the_writer` — seed the library with
  one committed track (a first `scan` with a plain one-track source, the shape of
  `lib_tests.rs:16-82`), then `set_tree_uri` again with the blocking two-track
  source and `scan`. Assert the recorded answer is `Some(Ok(window))`. Failure
  message must name the defect, e.g. `"a library read did not complete while a
  scan held the writer"`.
- `a_read_during_a_scan_sees_the_library_as_it_was_before_the_scan_committed` —
  same run; assert the mid-scan window's `total == 1`, and that `list_tracks`
  after `scan` returns `total == 2` (H3).

**Verify:** `cargo test -p reprise-android-ffi read_during_scan` — **both fail**,
by assertion, with those messages. Quote the output and the wall time, and
confirm the run did not hang.

**Commit:** `test: prove the Android library is unreadable during a scan`
(a red test is committed on its own, per `AGENTS.md` point 2).

---

### Task 2 — the reader handle

**Entry points:** `crates/reprise-android-ffi/src/library_types.rs`,
`crates/reprise-android-ffi/src/lib.rs`.

1. Replace `state: Mutex<LibraryState>` with `writer: Mutex<Db>` and
   `reader: Mutex<Db>`.
2. Rename `lock()` → `writer()` and add `reader()` (`library_types.rs:30-39`).
   Both map poisoning to `LibraryError::Database` exactly as today. **Rename
   rather than keep `lock()` as an alias:** the compiler then enumerates all 28
   call sites and none can be forgotten. That is the point of the rename.
3. `MusicLibrary::open` (`lib.rs:81-94`): `Db::open_migrated` first, then
   `Db::open_ready(&db_path)` — in that order (H5), with a comment saying why.
4. Move the 16 methods of §1.1's read table to `reader()`. Leave the 9 write
   sites on `writer()` (D4).
5. Update the existing tests the rename touches: `appearance.rs:249`, `:287`,
   `:336`, `:371`; `mobile_sync.rs:126` (`library.state.try_lock()` →
   `library.writer` — the assertion's meaning is unchanged) and
   `mobile_sync.rs:193`.

**Verify:**
- `cargo test -p reprise-android-ffi read_during_scan` — both tests green.
- `cargo test -p reprise-android-ffi` — the Task 0 count plus 2, no other change.
- **Mutation (production code):** in `MusicLibrary::list_tracks` (`lib.rs:145`)
  change `self.reader()?` to `self.writer()?`; rerun; the first test goes red with
  its own message. Revert. One token, and that is the honest granularity — do not
  store the handles in an array to manufacture a single-character variant.
- Cross-target check, with the NDK toolchain:
  ```bash
  toolchain=/opt/android-ndk/toolchains/llvm/prebuilt/linux-x86_64/bin
  CC_aarch64_linux_android=$toolchain/aarch64-linux-android21-clang \
  AR_aarch64_linux_android=$toolchain/llvm-ar \
  CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$toolchain/aarch64-linux-android21-clang \
  cargo check --locked -p reprise-android-ffi --target aarch64-linux-android
  ```

**Commit:** `fix: give the Android library reads their own database handle`

---

### Task 3 — the configured tree leaves the writer mutex

**Entry points:** `library_types.rs`, `lib.rs`, `mobile_sync.rs`,
`artwork_tests.rs`.

Red first. Add to `read_during_scan_tests.rs`:

- `track_artwork_answers_while_a_scan_holds_the_writer` — same rendezvous, the
  reader thread calls `track_artwork(uri, AndroidArtworkSize::List)` and the test
  asserts it returned `Ok(_)`. It does **not** assert a picture: a track with no
  cover is `Ok(None)` and that is an answer (`lib.rs:216-236`). The claim is that
  it answered at all.

Then:

1. `tree: Mutex<Option<ConfiguredTree>>` as its own field; delete `LibraryState`.
2. Add
   `configured_tree(&self) -> Result<(PathBuf, Arc<BridgedSource>), LibraryError>`
   that takes the lock, clones both, drops the guard **inside the helper**, and
   returns `LibraryError::TreeNotConfigured` for `None`. Making the drop
   structural is the point: no caller can hold the tree lock across work.
3. `scan` (`lib.rs:114-142`): `writer()` **first**, then `configured_tree()` (D3).
   Add the lock-order rule as a comment on the struct — an ordinary `//`
   comment, **not** `///`. Corrected 2026-08-14 during the code phase: the
   struct carries `#[derive(uniffi::Object)]`, and UniFFI copies doc comments
   into the generated Kotlin, so a `///` here breaks Task 5's byte-identical
   binding proof. The rule is for Rust readers; it has no business in the
   exported surface. `//` satisfies both requirements.
4. `track_artwork` (`lib.rs:242-246`) and `import_track_analysis`
   (`mobile_sync.rs:8-17`) use `configured_tree()`.
5. Re-point `artwork_tests.rs:177-193`: it poisons `state` (`artwork_tests.rs:180`)
   and asserts `track_artwork` reports `Err(LibraryError::Database)` (`:186-192`).
   After this task `track_artwork` no longer touches the writer, so the test must
   poison `tree` instead. Its doc comment (`artwork_tests.rs:152-157`) still
   holds; only the field changes. **This is the test that would silently start
   passing for the wrong reason if it were left alone — do not leave it alone.**

**Verify:**
- `cargo test -p reprise-android-ffi read_during_scan` — three green.
- **Mutation (production code):** in `track_artwork`, replace the
  `configured_tree()` call with `writer()` + a direct field read, so the artwork
  path takes the writer again; the new test goes red. Revert. Record the exact
  edit in the commit message.
- Full `cargo test -p reprise-android-ffi`, Task 0 count plus 3.

**Commit:** `fix: keep the Android artwork path out of the scan's mutex`

---

### Task 4 — prove reads still see writes across the two handles

The one thing D1 could plausibly break is read-your-writes: a setting written on
`writer` and read back on `reader`. WAL guarantees it (each read starts a new read
transaction after the write's commit), but "SQLite guarantees it" is not this
repo's standard of proof.

Two existing tests cross the boundary after Task 2 and become free proofs — check
they are still green and say so in the commit message:
- `appearance.rs:241-276` writes `set_theme` at `:268` and reads
  `appearance_settings` back at `:269-276`.
- `lib_tests.rs:425`
  (`track_identity_drives_rating_writes_and_a_missing_id_is_an_error`) writes a
  rating and reads the row back.

Add one test that names the property so it cannot be deleted by accident:
`a_write_on_the_writer_handle_is_visible_to_the_next_read_on_the_reader_handle`.

**Verify:** `cargo test -p reprise-android-ffi`, Task 0 count plus 4. State
plainly in the commit message that this test guards the *handle split*, not the
setting.

**Commit:** `test: pin read-your-writes across the Android library's two handles`

---

### Task 5 — the Android suite and the binding proof

```bash
export ANDROID_HOME=/home/marvin/.local/share/android-sdk
printf 'sdk.dir=%s\n' "$ANDROID_HOME" > android/local.properties
scripts/check-android-suite.sh
```

`scripts/check-android-suite.sh` exists at `origin/dev` (added by #471,
`5721ade95e`); it regenerates the UniFFI bindings from a host release build, runs
`:app:testDebugUnitTest` and `:app:assembleDebug`, proves the JUnit XML is fresh,
and refuses to pass below `ANDROID_TEST_FLOOR = 334` *executed* tests (skips
subtracted, since `8e2e812e19`). It needs a JDK 21 and the SDK, and **no
device**.

**H7's proof lives here.** The script deletes and regenerates
`android/app/src/main/java/uniffi/`. Regenerate once on the branch base and once
on the branch head and `diff -r` the two output directories: they must be
identical. If they are not, this plan has changed the exported surface and §6's
claim is false — stop and report it rather than adapting the Kotlin.

The one non-surface way to fail this that has actually happened: a `///` doc
comment on a `#[derive(uniffi::Object)]` type. UniFFI copies those into the
generated Kotlin, so the diff is non-empty while no signature, record or error
variant moved. Task 3 step 3 is written to avoid it. If it recurs, the fix is to
demote the comment to `//`, never to adapt the Kotlin.

**Verify:** quote the counts the script prints
(`suites=… tests=… failures=… errors=… skips=… verdict=fresh`) and the empty
`diff -r`.

---

### Task 6 — the Kotlin comments (comments only, no behaviour)

See §6 for the argument. Correct only the sentences that would otherwise assert
something false about the Rust side:

- `MobileSurfaceViewModel.kt:67-68`
- `MainActivity.kt:89-90`
- `BrowseScreen.kt:416-419`
- `TrackLoader.kt:32-36`
- `RatingWriter.kt:24-28` — this one is **still true** and must not be weakened:
  writes do still queue behind the scan's writer mutex (§2.1). Check it, change
  nothing but a cross-reference if one is needed.

**Verify:** `scripts/check-android-suite.sh` with the same environment as Task 5
— same counts. A comment change that moves a test count is a bug.

**Commit:** `docs: the Android reads no longer wait on the scan`

---

## 5. Traps

- **`ANDROID_HOME`.** `/home/marvin/.local/share/android-sdk`. Never
  `/opt/android-sdk` — NDK only, no `platforms/`, and Gradle's error looks like a
  broken project. `android/local.properties` is gitignored and absent in a fresh
  worktree; write it before the first Gradle call.
- **Open order in `MusicLibrary::open`.** Writer (`open_migrated`) before reader
  (`open_ready`). Reversed, a fresh install fails with `SchemaNotReady` and
  *every* test in the crate turns red at once — which looks like a catastrophe
  and is one swapped line (H5).
- **Same-thread relock deadlocks.** Do not write a test that takes `writer` and
  then calls a read method on the same thread expecting it to work on the old
  code: `std::sync::Mutex` is not reentrant and it will hang, not fail. Every
  overlap test here uses two threads and a deadline for exactly this reason.
- **`estimated_audio_files` does not walk the source.** It reads the catalog
  (`scanner_progress.rs:36-57`). The rendezvous therefore fires inside the walk,
  which is inside the transaction. Guard it with an `AtomicBool` anyway — a tree
  with subdirectories calls `list_children` more than once.
- **`TMPDIR`.** The Rust tests in this crate depend on `readdir` order and are
  green with `TMPDIR=/tmp`; a worktree with a redirected `TMPDIR` may not be.
- **Generated bindings are gitignored.** Do not commit
  `android/app/src/main/java/uniffi/` or `android/app/src/main/jniLibs/`, even
  though Task 5 regenerates them twice.
- **The 800-line rule** (`scripts/check-architecture.sh:16-23`) covers the new
  test file too. `lib_tests.rs` is already at 610.

---

## 6. The Kotlin side

**This plan changes no Kotlin behaviour, and removes no workaround.**

Every deferral on the Kotlin side has two justifications and this plan removes
only one of them:

- `TrackLoader` (`TrackLoader.kt:29-62`) and `RatingWriter` (`RatingWriter.kt:21-39`)
  exist to keep SQLite off the main thread. That requirement is permanent and has
  nothing to do with the mutex: a read issued inside a composition is an ANR risk
  whether it blocks for a folder walk or for 8 ms of I/O. `BrowseScreen.kt:416-419`
  says the same about the mini player's row.
- `RatingWriter`'s reason is *entirely* intact: writes still serialise behind the
  scan's writer mutex, by design (§2.1). Removing it would be a regression.
- `MobileSurfaceViewModel.openAlbum` (`:58-73`) is kept for two reasons and the
  lock is the second; the first — that an anchor is an index into paged-in rows
  and means nothing to a reloaded window (`:43-57`) — stands untouched.
- `TrackLoader`'s bounded retry (`TrackLoader.kt:14-24`, `ATTEMPTS = 3`) stays. It
  now protects against a genuinely transient failure rather than a guaranteed
  one, which is what it was written for.

So this plan makes the workarounds *unnecessary as lock avoidance* and leaves
them in place as thread-hygiene, which is what they should have been all along.
Their removal — if any is ever removable — belongs to the repository/StateFlow
rework queued behind this. Task 6 corrects the sentences that would otherwise be
false; that is the whole Kotlin footprint.

Explicitly **not** done: no `de.reprise.spike` rename, no Gradle module, no
repository layer, no `StateFlow`, no ViewModel restructuring.

---

## 7. The unlanded scroll branch changes one of these classifications

`feature/android-list-scroll-performance` (worktree
`/home/marvin/Projects/reprise-android-list-scroll-performance`, `phase: refactored`,
14 commits, base `39cc58cf8b`, **not landed as of 2026-08-14**) touches
`crates/reprise-android-ffi/src/lib.rs` and `crates/reprise-android-ffi/src/artwork_tests.rs`
— the two files this plan rewrites hardest.

The collision is not textual, it is semantic. On that branch `track_artwork` calls
`reprise_core::cover::track_file_stamp(&state.db, …)` **inside** the lock block:
it reads the database. §1.1 classifies `track_artwork` as "tree only, never
touches `state.db`", which is true of `origin/dev` and **false of that branch**.

**Whoever rebases the scroll branch after this plan lands must make
`track_artwork` take `reader()` *and* `configured_tree()`** — not
`configured_tree()` alone. A rebase that resolves the conflict textually will
compile and will quietly reintroduce a database read outside the reader handle.
Say so in that branch's rebase commit message.

This plan is built against `dev` on purpose: that is where its classification is
correct, and the scroll branch carries an unmeasured performance claim that must
not become this plan's dependency.

---

## 8. What the grill settled (2026-08-14)

1. **Reads only.** The write path keeps today's behaviour; improving it is F1.
2. **H4 accepted**, not fixed — `reprise-core` stays untouched (F4).
3. **`READER_DEADLINE = 120 s`**, not 30 s: the red run costs two minutes once,
   a flake costs a wrong-place investigation every time.
4. **Gates scoped to the affected crate** plus the Android suite and the
   cross-target check. No full workspace run.
5. **Built against `dev`**, with §7 naming the scroll-branch collision explicitly.
6. **One reader connection**, not a pool (D6, F3).

Corrections made on the way in: the `ANDROID_HOME` path (the draft named an
NDK-only directory), the mutation-proof rule (production code, one token is
honest), and the line references in §1.

---

## 9. Follow-ups this plan deliberately does not do

- **F1 — the write path.** A heart tap during a scan is still answered late.
  Fixing it means splitting the scanner's single transaction
  (`scanner.rs:298` … `:689`) so writers get windows — a `reprise-core` change
  with desktop consequences (the vanish-reconcile is folded into that transaction
  on purpose, `scanner.rs:170-191`). Own plan.
- **F2 — `open_ready_read_only` for the reader.** Would enforce D4 at the OS
  level. Needs `busy_timeout`/`foreign_keys` moved into that path in
  `reprise-core` and a device proof that a WAL `-shm` is writable under Android's
  filesystem.
- **F3 — measure whether readers contend.** After the repository/coroutine rework
  (architecture backlog point 4/5), measure whether concurrent reads queue
  noticeably on the single reader mutex. Only then decide on a pool; it is a field
  change with no call-site change.
- **F4 — `Db::read_snapshot`.** An explicit read transaction so a count and its
  page come from one snapshot (H4). Build it if the straddle ever produces a real
  complaint, not before.
- **The two native handles in one process** (H6) — `MainActivity` and
  `ReprisePlaybackService` each open their own. Unchanged here; this plan makes
  that consolidation easier, not harder.

---

## Parallelität

**Verdict: one strand.** The cut was attempted along three lines and each fails
on a named file.

- **By task.** Tasks 1–4 all write `crates/reprise-android-ffi/src/lib.rs` (Task 2
  re-routes five read methods there, Task 3 rewrites `scan` and `track_artwork` in
  the same file) and `crates/reprise-android-ffi/src/library_types.rs` (Task 2
  replaces the struct, Task 3 replaces it again). They also all write the same new
  test file, `read_during_scan_tests.rs`. Every proposed boundary leaves two
  strands editing `lib.rs`.
- **By "reads" vs "artwork".** Looks separable — the read table spans six files,
  `track_artwork` is one method. It is not: both depend on the same struct rewrite
  in `library_types.rs`, and a strand that moved the reads without the tree split
  would ship a browse list whose covers are all blank during a scan. A half-fix
  whose tests are green while the user is still stuck is exactly the trap this
  plan exists to avoid.
- **By language.** Task 6 (Kotlin comments) is the only genuinely disjoint file
  group. But it may not merge before the Rust change is true — a comment saying
  reads no longer wait on the scan is a lie on `dev` until Task 3 lands. The merge
  order would be forced to Rust → Kotlin anyway, and the Kotlin half is four
  comment blocks. A second worktree costs more than it saves.

**Execution:** one strand, tasks in numbered order, one commit per task
(`AGENTS.md`, "How to resume", point 3). After each task the standing gates from
§4 plus that task's own verification and its mutation arm.
