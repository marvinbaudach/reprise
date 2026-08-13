---
slug: android-remove-albums-and-favourites-tabs
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-12
---
# Removing the Albums and Favourites tabs from the Android app

Implementation plan for
`docs/superpowers/specs/2026-08-12-android-remove-albums-and-favourites-tabs-design.md`.
This document orders the work; every decision it needed has been taken, and the
ones that were open at draft time are now written into the step that carries
them.

**Where this plan overrules the spec.** The spec's regression guard says the
existing favourite tests "must pass unchanged". Two of them cannot: they read
the rating write out of the Favourites *list*, which is the thing being removed.
This plan retargets those two onto a surviving surface, keeping the assertion
word for word and changing only the window it is read through (step 11, *The
guard, part two*). That divergence is decided, not proposed — do not reopen it by
citing the spec's wording back at step 7.

Every code reference below was verified against `origin/dev`. Branch from
`origin/dev`, never from a local `dev-*` checkout.

## How to read the file references

Paths and line numbers are **orientation, not an allow-list**. They mark where
the measurement started; they do not bound what the change may touch. Removing
two enum constants that are switched on, saved, tagged, labelled and asserted in
many places is by nature a repo-wide edit. Where a step says "expected finds",
treat that as the floor: run the sweep commands, follow every hit the compiler
and the greps produce, and change whatever is genuinely affected — including
files nobody listed. A step is done when the greps come back clean, not when the
listed files have been edited.

The one hard boundary is in step 10: the sweep stops at historical records.

## Verification commands

Used verbatim by the steps below.

Rust workspace gate (from the repo root):

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace --locked
cargo audit          # only accepted advisory: RUSTSEC-2024-0436
```

Focused Rust runs:

```bash
cargo test --locked -p reprise-core queries::library_views
env TMPDIR=/tmp cargo test --locked -p reprise-android-ffi
```

**`TMPDIR=/tmp` is mandatory for `reprise-android-ffi`, and it is mandatory in
that direction.** Several browse tests compare track *ids*, which come from scan
insertion order, which follows the order the filesystem hands back directory
entries. Measured on the same commit: green on tmpfs (`/tmp`), four false
failures with `TMPDIR` pointed at the nvme (`~/.cache`). Redirecting `TMPDIR`
away from tmpfs to keep it tidy manufactures a regression that is not there.

Core purity proof, required after any `reprise-core` change:

```bash
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # MUST be empty
```

Android unit suite (Robolectric 4.16.1). **JDK 21 — JDK 26 breaks Robolectric:**

```bash
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk
export ANDROID_HOME="$HOME/.local/share/android-sdk"
cd android && ./gradlew \
  --max-workers=2 \
  -Pkotlin.compiler.execution.strategy=in-process \
  :app:testDebugUnitTest
```

The `--max-workers=2 -Pkotlin.compiler.execution.strategy=in-process` pair is
taken from `scripts/verify-now-playing-scene.sh` (the repo's only Gradle
wrapper); it keeps the run inside a sane core budget. A focused run appends
`--tests de.reprise.spike.<Suite>`. Note that Gradle reports `BUILD SUCCESSFUL`
for a task that ran no tests at all — confirm a focused run actually executed
something by checking that
`android/app/build/test-results/testDebugUnitTest/TEST-*.xml` was written for the
suite you named, and never treat a filtered run as the gate.

Redirect long output to a log and grep it rather than reading it whole. When
grepping a Rust run for failures, match `^test result: FAILED` and nothing
looser — a plain search for "FAILED" or for "failed" hits the per-test summary
lines of green runs too, and reports a red suite that is not there.

## Ordering and why

Step 0 comes first and blocks everything: without a recorded baseline of what is
already red, later steps cannot tell a real regression from inherited noise.

The new capability is then built bottom-up **before** anything is deleted
(steps 1–6), and only then is the old surface removed (steps 7–9). The reverse
order is tempting — delete first, it is less code — but it strands the album
page: the moment `BrowseTab.ALBUMS` goes, the album page has no entry point, and
it would stay unreachable for however many steps the artist page takes to build.
Building first means every step ends with a tree that compiles, a suite that
passes, and an app in which every album is still reachable by some route.

Within the build-up, the layers go Rust query → FFI → Kotlin port → Compose
because each layer's test needs the layer below to exist: the FFI test asserts on
rows the core query produces, the Kotlin port test asserts on what the FFI
returns, and the Compose test needs a port to fake. Going the other way would
mean writing tests against types that do not exist yet.

Steps 7 and 8 are deliberately split along the FFI boundary so neither half is
ever red. `BrowseDestinationSettings.kt` maps the Rust enum onto `BrowseTab`; if
both enums shrank in one step the intermediate state would not compile. Step 7
shrinks `BrowseTab` while the Rust variants still exist and folds them onto
`Titles`; step 8 then removes the Rust variants, at which point the stored ids
fall through to `Unsupported`, which already maps to `Titles`.

## Parallelism

- **Step 0 blocks everything.** It must finish, and its record must exist, before
  the first edit.
- **Steps 1 and 2** are independent of each other. Both are additive functions in
  `library_views.rs`; if two agents run them, the file is a merge point. Step 1
  also introduces the shared selection helper that step 2 sits next to — if run
  concurrently, step 1 owns that helper.
- **Steps 8 and 9** are independent of each other once step 7 has landed.
- **Everything else is strictly sequential**: 1 and 2 need 0, 3 needs 1+2, 4
  needs 3, 5 needs 4, 6 needs 5, 7 needs 5+6, 10 needs 7+8+9, 11 needs
  everything.

The single-agent order 0 → 1 → 2 → … → 11 is correct and is the recommended
default; the parallel pairs are an option, not a requirement.

---

## Step 0 — Baseline: write down what is already red

**Goal.** A written record, made on the fresh worktree **before the first edit**,
of which gates and which suites are already failing. Step 11 compares against
this record instead of against "everything green".

**Why this step exists.** Gates on `dev` are documented as partly red, and the
GTK display suites are flaky when they run in a herd. Without a baseline the
implementer reads inherited red as self-inflicted red, and either chases a bug
that is not theirs or — worse — "fixes" a foreign test to make the gate pass.

**Do this.** Check out the worktree from `origin/dev`, change nothing, and run
both gates into logs:

```bash
mkdir -p .tmp/baseline
cargo fmt --check                                     > .tmp/baseline/fmt.log 2>&1
cargo clippy --all-targets --workspace -- -D warnings > .tmp/baseline/clippy.log 2>&1
cargo test --workspace --locked                       > .tmp/baseline/rust.log 2>&1
env TMPDIR=/tmp cargo test --locked -p reprise-android-ffi > .tmp/baseline/ffi.log 2>&1
cd android && ./gradlew --max-workers=2 \
  -Pkotlin.compiler.execution.strategy=in-process \
  :app:testDebugUnitTest > ../.tmp/baseline/android.log 2>&1
```

Then read the answers out with grep — do not read the logs whole:

```bash
grep -n "^test result: FAILED" .tmp/baseline/rust.log
grep -n "^error" .tmp/baseline/clippy.log | head
grep -h "<testsuite " android/app/build/test-results/testDebugUnitTest/TEST-*.xml
grep -h -B2 "<failure" android/app/build/test-results/testDebugUnitTest/TEST-*.xml \
  | grep "<testcase"
```

The Android JUnit XML carries `tests=`, `failures=` and `errors=` per suite, so
the second-to-last command gives both the totals and the red suites in one pass.

**Record.** Write `.tmp/baseline/baseline.md` (uncommitted) holding:

1. the commit sha the baseline was taken at,
2. every Rust crate whose suite reported `test result: FAILED`, with the failing
   test names,
3. every Android suite with a non-zero `failures`/`errors`, with the failing test
   names,
4. the **Android test total** (`tests=` summed across suites) — step 11 needs this
   number to account for the drop,
5. whether `cargo fmt --check`, `cargo clippy` and `cargo audit` were clean, and
   for `audit` whether anything beyond RUSTSEC-2024-0436 appeared.

If a display or GTK suite is red here, that is a baseline fact and this plan does
not fix it. Note it and move on.

**Verify.** `.tmp/baseline/baseline.md` exists and names a commit sha. Nothing
else in the tree has changed — `git status` is clean apart from `.tmp/`.

**Depends on.** Nothing. Blocks everything.

---

## Step 1 — Core: albums by one artist, windowed, over a shared selection rule

**Goal.** `query_artist_albums(db, artist, window) -> AlbumWindow` plus
`query_artist_album_count(db, artist) -> i64` in
`crates/reprise-core/src/queries/library_views.rs`, **and** the extraction of the
"album by this artist" selection rule into one shared piece of SQL that both this
new query and the existing `query_artist_detail_albums` use. Additive plus one
refactor whose observable behaviour is nil.

### The shared selection rule

`query_artist_detail_albums` (`library_views.rs:402`) already selects albums for
one artist, for the GTK desktop, with this `WHERE`:

```sql
WHERE {PRESENT} AND TRIM(album) <> '' AND {EFFECTIVE_ALBUM_ARTIST} = ?1 COLLATE NOCASE
```

The two queries stay **separate** — the desktop one is unwindowed, projects
`Vec<ArtistAlbum>`, and sorts newest year first; the Android one is windowed and
projects `AlbumWindow`/`AlbumSummary`. What must not stay separate is the rule
for *what counts as an album of this artist*. Extract it once, in the style of the
file's existing `album_summary_filter_clause(has_filter, param_index)` and
`artist_summary_filter_clause(...)` helpers, which already parameterise the bind
index for exactly this reason:

```rust
/// What counts as an album by one artist: a present track, a non-blank album
/// title, and an exact case-insensitive match on the effective album artist.
/// Shared so the desktop's artist detail and the Android artist page cannot
/// drift apart on the definition.
fn artist_albums_selection(param_index: u8) -> String {
    format!(
        "{PRESENT} AND TRIM(album) <> '' \
         AND {EFFECTIVE_ALBUM_ARTIST} = ?{param_index} COLLATE NOCASE"
    )
}
```

The index has to be a parameter: the desktop query binds the artist at `?1`,
while the windowed query binds limit and offset at `?1`/`?2` and needs `?3`.

Note this changes the spelling proposed for the new query from
`LOWER(...) = LOWER(?)` to `= ? COLLATE NOCASE`. Both are ASCII-only case folding
in SQLite and behave identically here; adopting the desktop's spelling is what
makes one shared rule possible at all.

**Hard condition — the desktop must not move a byte.** `query_artist_detail_albums`
keeps its projection, its `GROUP BY album_key`, its
`ORDER BY grouped.year DESC, TRIM(tracks.album) COLLATE NOCASE ASC`, and its
`Vec<ArtistAlbum>` return type. Only the `WHERE` text is now produced by the
helper, and the string it produces must be identical to the literal it replaced.

**Measured caveat, for honesty about the evidence.** On `origin/dev`,
`query_artist_detail_albums` has no production caller: `git grep detail_albums`
returns its definition (`:402`), a doc cross-reference (`:366`), its one unit test
(`library_views_tests.rs:299`), and a historical plan document. So "desktop
behaviour unchanged" is carried by that single test plus the byte-identity of the
generated SQL — which is why the byte-identity check below is not optional. Do
**not** read the missing callers as licence to delete or restructure the query;
that is a follow-up question, recorded under *Open questions*.

### Test first

Add to `crates/reprise-core/src/queries/library_views_tests.rs` (the existing test
module for this file — it already exercises `query_artist_detail_albums` at
line 299, so the fixture style is there):

1. **Byte-identity of the shared rule.** `artist_albums_selection(1)` returns
   exactly the string the desktop query used before the refactor. Write the
   expected literal out in the test. This is the guard that a later "tidy-up" of
   the helper cannot silently re-word the desktop's `WHERE`.
2. **A prefix test.** Seed tracks whose effective album artist is `Bad` and
   others whose effective album artist is `Bad Religion`. `query_artist_albums(db,
   "Bad", …)` must return only `Bad`'s albums. This is the test that fails if
   anyone reaches for `LIKE`: an exact match returns 1 album, a prefix match
   returns both. Write it before the function exists so it fails to compile, then
   — and this is the part that matters — after the function compiles, temporarily
   swap the predicate to `LIKE` and confirm the test goes **red on the assertion,
   not on compilation**. A test that only ever failed with "cannot find function"
   has not proven anything about the predicate.
3. **An agreement test:** `query_artist_album_count` equals `query_artist_albums(…,
   full window).rows.len()`, and equals the `total` on the returned
   `AlbumWindow`. Mutation check: make the count query drop the artist predicate
   and confirm this test alone turns red.
4. **A windowing test:** a short window returns fewer rows but still reports the
   full `total` and `has_more == true`.
5. **A blank-album test:** a track by the artist with an empty `album` tag appears
   in **no** album row. This is the pre-condition for step 2's complementarity
   claim.
6. **A sort test** (see the ordering decision below): seed three albums by one
   artist — two with different years, one with no year — and pin the full
   expected order.

`artist_albums_are_newest_first` (`library_views_tests.rs:290`) is the desktop's
existing test for this query. It must stay green **without being edited**. An
edit there means the refactor changed desktop behaviour and the step has failed.

### Album ordering — decided

`ORDER BY year DESC, TRIM(album) COLLATE NOCASE ASC`: newest release year first,
ties broken alphabetically by title. This is identical to the desktop artist view
and is a decision, not an assumption.

One trap the sort test above is there to pin: modelled on `query_albums`, the
window's `year` is `Option<i32>` built as `MIN(CASE WHEN year > 0 THEN year END)`,
so an album with no tagged year comes back as `NULL`. SQLite orders `NULL` below
every value, so `year DESC` places untagged-year albums **last** — which is what
we want, but it should be pinned by a test rather than left to be rediscovered.
(The desktop query instead uses `COALESCE(MAX(year), 0)`. The two projections
differ and stay differing; only the selection rule is shared.)

### Implementation

Model the query on `query_albums` (`library_views.rs:76`) — the same
`WITH grouped` CTE, the same `AlbumSummary` projection, the same
`super::surface_browse::has_more` — with the CTE's `WHERE` supplied by
`artist_albums_selection(3)` and the artist bound at `?3`.
`query_artist_album_count` follows `query_album_count` (`library_views.rs:195`)
with the same shared rule at its own index, so the two agree by construction.

`library_views.rs` is 523 lines on `origin/dev`; the repo's hard limit is 800.
Steps 1 and 2 together add well under that, but check the count before adding
more and split the file rather than exceeding it.

**Verify.**

```bash
cargo test --locked -p reprise-core queries::library_views
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # empty
```

**Depends on.** Step 0. Parallel with step 2.

---

## Step 2 — Core: an artist's untagged tracks, windowed

**Goal.** A windowed query for tracks by one artist carrying no album tag — the
"Other titles" section. Additive only.

**Test first.** In `library_views_tests.rs`:

1. **Complementarity.** Seed one artist with some album-tagged tracks and some
   untagged ones. Assert that the untagged query returns exactly the tracks
   step 1's album rows do not account for, and that the two counts sum to the
   artist's total track count.
2. **The `TRIM` is load-bearing.** Seed one track with an empty album (`''`) and
   one with a whitespace-only album (`'   '`); both must appear in the untagged
   window. Then drop the `TRIM` from the predicate and confirm the whitespace-only
   track disappears — red on the assertion.

   Note on how *not* to write this: the schema is `album TEXT NOT NULL DEFAULT ''`
   (`crates/reprise-core/src/db.rs:120`), so a `NULL` album cannot be seeded — the
   insert would violate the constraint. "No album" has exactly one spelling in
   this database, the empty string, plus whitespace, which is what `TRIM` is for.
   A predicate written as `album IS NULL` is not merely wrong, it matches nothing
   at all.
3. **Exact artist match**, same `Bad` / `Bad Religion` shape as step 1.
4. **Windowing:** short window, full `total`, `has_more` true.

**Implementation.** Shape it after `query_artist_track_window`
(`library_views.rs:433`) and `query_artist_track_count` (`:472`), adding:

```sql
AND TRIM(album) = ''
```

Do **not** filter client-side over `list_artist_tracks` — that would require
loading every track of the artist before knowing which are untagged, defeating
the windowing every other list uses.

### The seam — decided

Those two helpers are `pub(super)` and are reached through
`ViewSource` / `LibraryTrackScope` (`surface_browse.rs:83–91`). **Add a
self-contained `pub fn` pair** in `library_views.rs` that takes `&Db` and returns
a `TrackWindow`, bypassing that dispatch — the same way
`query_artist_detail_albums` already sits outside it. Do **not** add a
`ViewSource` or `LibraryTrackScope` variant, and do not encode "no album" as a
`BrowseFilter`.

Why, measured on `origin/dev`:

- `ViewSource` is matched on 1111 lines across 134 files — 75 in
  `crates/reprise-core/src/queries/mod.rs`, 61 in
  `crates/reprise-core/src/browser.rs`, and around 695 across `reprise-gnome`
  alone. Many of those matches are exhaustive `match` arms that would each have
  to grow a case. Paying that across the whole desktop UI and its tests for a
  function only the Android artist page calls is out of proportion.
- `BrowseFilter.album = Some("")` is not available as a sentinel either.
  `browse_clause` (`browse.rs:164–183`) emits `AND album = ?n` for *any* `Some`,
  so an empty string would be treated as an ordinary facet value, not as a mode;
  `None` — not `Some("")` — is the type's own spelling for "no filter"
  (`BrowseFilter::is_empty`, `browse.rs:31–39`); and `BrowseFilter` derives
  `serde::Serialize`/`Deserialize` (`browse.rs:12`) and is persisted into saved
  sessions, so the sentinel would be written to disk and read back by code that
  has no idea it means anything special.

If a cheaper seam turns up while implementing, take it — but not one that changes
desktop behaviour, and not one that widens `ViewSource`.

**Verify.** Same commands as step 1.

**Depends on.** Step 0. Parallel with step 1 (both edit `library_views.rs` and
`library_views_tests.rs` — merge point if run concurrently; step 1 owns the shared
selection helper).

---

## Step 3 — FFI: expose both queries to Android

**Goal.** On `MusicLibrary` in `crates/reprise-android-ffi`:

- `list_artist_albums(artist: String, window: WindowRange) -> Result<AlbumWindow, LibraryError>`
- `list_artist_untagged_tracks(artist: String, window: WindowRange) -> Result<TrackWindow, LibraryError>`

Additive only — no removals in this step. `filtered_browse.rs` is the natural
home: `list_artist_tracks` already lives there (`filtered_browse.rs:49`) and
stays.

**Test first.** The inline `mod tests` in
`crates/reprise-android-ffi/src/filtered_browse.rs` already has a
`filtered_library()` fixture (line 102) that builds a temp library — extend it
rather than inventing a second fixture. New tests:

1. `list_artist_albums` returns the artist's albums and nothing else, with
   `total` and `has_more` consistent across a short window — mirroring
   `a_short_artist_window_still_counts_every_track_the_tile_promised`
   (`filtered_browse.rs:212`), which is the existing precedent for the
   window/total contract.
2. An unknown artist is an **empty window, not an error** — the same rule
   `artist_values_without_matches_are_empty_windows_not_errors`
   (`filtered_browse.rs:231`) already fixes for tracks.
3. `list_artist_untagged_tracks` returns only the untagged tracks, empty window
   for an artist that has none.

Because the fixture writes files into `TMPDIR`, run these with `TMPDIR=/tmp` from
the first red run onward; a red caused by readdir order looks exactly like a red
caused by the new code.

**Implementation.** Thin wrappers: lock state, call the step 1/2 core queries,
map `rusqlite::Error` to `LibraryError::Query { detail }` the way the neighbours
do, and reuse the existing `From<queries::AlbumWindow>` / `From<queries::TrackWindow>`
conversions in `browse.rs` (lines 117 and 127). `AlbumRow` / `AlbumWindow` already
cross the boundary for `search_albums`, so no new uniffi types are needed.

**Verify.**

```bash
env TMPDIR=/tmp cargo test --locked -p reprise-android-ffi
```

**Depends on.** Steps 1 and 2.

---

## Step 4 — Kotlin port: carry both calls across

**Goal.** `listArtistAlbums` and `listArtistUntaggedTracks` on the
`LibrarySessionPort` interface (`LibrarySession.kt:13`), on `LibrarySession`
itself, and on `AndroidLibrarySessionPort`. Additive only.

**Test first.** `LibraryScreenStateTest.kt` already carries a port double
(around line 358) that implements every `LibrarySessionPort` method. Adding two
interface methods makes that double fail to compile, which is the compiler
telling you where the fakes are — follow it to every one of them (expected finds
include the doubles in `LibraryScreenStateTest.kt`, `BrowseSurfaceTest.kt:541`
`RecordingBrowsePort`, and any other file implementing the interface; search,
do not assume the list is complete).

Write, in `LibraryScreenStateTest.kt` or a focused new suite:

1. `listArtistAlbums` delegates the literal artist name and the literal window
   offset/limit to the port, unmodified — the existing recording-port pattern in
   `BrowseSurfaceTest.kt::browseSearchesDelegateLiteralTextAndWindowToTheirPortMethods`
   (line 457) records calls as strings like `"search-albums: slow :200:75"`;
   follow it. This proves the window is not silently re-clamped on the way down.
   It goes red on a missing recorded call, not on a compile error, once the
   method exists but is stubbed.
2. The same for `listArtistUntaggedTracks`.

**Implementation.** Follow `listArtistTracks` (`LibrarySession.kt:36` and
`AndroidLibrarySessionPort.kt:91`) exactly: same `LibraryWindowRange.toFfi()`
conversion (`AndroidLibrarySessionPort.kt:138`), same
`FfiAlbumWindow.toLibraryAlbums()` / `FfiTrackWindow.toLibraryTracks()` mappers
(lines 146 and 140).

The generated uniffi Kotlin bindings must be regenerated for the new FFI methods
to exist on the Kotlin side — do not hand-edit generated bindings.

**Verify.**

```bash
cd android && ./gradlew --max-workers=2 \
  -Pkotlin.compiler.execution.strategy=in-process \
  :app:testDebugUnitTest
```

(with `JAVA_HOME` on JDK 21, as above). A focused first run may use `--tests
de.reprise.spike.LibraryScreenStateTest`, but the step is only green on the full
suite, because adding interface methods touches every port double in the module.

**Depends on.** Step 3.

---

## Step 5 — Compose: the artist page and the album nested under it

**Goal.** The Artists tab detail becomes a real artist page: an **Albums**
section and an **Other titles** section, with an album opening the existing
album page one level deeper. `BrowseTab.ALBUMS` still exists at the end of this
step — nothing is removed yet.

### The second section — decided

- Its heading is **"Other titles"**. Use that string verbatim; it is not a
  placeholder.
- It is **hidden when the artist has no album-less tracks**, exactly as the albums
  section is hidden when there are no albums.
- The string lives **inline in the Kotlin source**, alongside its neighbours.
  That is the house style on this side: `BrowseTab` carries its tab labels inline
  in its constructor (`BrowseScreen.kt:52–58`) and `LibraryText.kt` holds the
  library surface's short strings as plain Kotlin. There is no gettext on Android,
  and `android/app/src/main/res/values/strings.xml` holds only `app_name` and one
  plural — it is not where UI copy goes in this app.

**Test first.** `BrowseSurfaceTest.kt` is 654 lines on `origin/dev` and the repo
caps code files at 800; put the new artist-page tests in a **new focused suite**
(e.g. `ArtistDetailSurfaceTest.kt`) rather than growing that file past the limit.

1. *An artist page lists the artist's albums.* Fake port returns two albums;
   assert both rows render under the artist. Red first because the artist detail
   currently renders a flat track list — confirm the failure message is "album
   row not found", not a fixture error.
2. *Albums are newest first, ties alphabetical.* Assert the rendered order for a
   fake port returning years out of order, so the step 1 decision is visible at
   the surface the user actually sees.
3. *Opening an album from an artist reaches the album page.* Reuse
   `openingAnAlbumUsesItsCoreIdentityAndOrder` (`BrowseSurfaceTest.kt:485`) —
   the spec keeps this test and retargets it at the artist-nested entry point.
   Move or adapt it; do not duplicate it.
4. *Playing from an album reached through an artist uses the album snapshot.*
   Same treatment for `playingFromAlbumDetailUsesTheAlbumSnapshot`
   (`BrowseSurfaceTest.kt:497`). This is the test that proves the nested route
   reuses `AlbumTrackList.playbackSelection(index)`
   (`LibraryScreenState.kt:136–142`) instead of growing a parallel queue path.
5. *An artist with untagged tracks shows them under "Other titles".* Assert on
   the literal heading.
6. *An artist with no untagged tracks shows no "Other titles" heading at all* —
   `assertDoesNotExist`, not an empty section.
7. *An artist with no albums* renders the page with the albums section absent and
   the other titles carrying it — no empty screen.
8. *Restore after process death restores artist and album as a pair.*
   `MobileSurfaceViewModel.kt:66–67` holds `openAlbum` and `openArtist` as two
   independent nullable fields. Assert that a restore which has an open album
   also has its artist — the failing case to write first is "album restored,
   artist null", which today's two independent values permit. Fix by restoring
   them as one value (or by dropping a parented album whose artist is absent),
   whichever the code makes honest.

**Implementation.** Touch points: `BrowseTabs.kt` (`ArtistsTab` at line 193,
`AlbumRows` at 305, `AlbumRow` at 354), `BrowseScreen.kt` (the artist branch at
line 560, `selectedArtist` / `selectedAlbum` state at 132–133, the
`artistRequestedOffset` pagination at 138), `LibraryScreenState.kt`
(`ArtistTrackList` at 144 — it will need album and untagged windows alongside
its tracks), `MobileSurfaceViewModel.kt` (the restore record), and
`MainActivitySurface.kt` (`MainActivitySurfaceDependencies` carries every library
lambda the screen uses — the two new calls have to be threaded through it and
through `MainActivity.kt`). Search for further call sites; that list is a
starting point.

Add `LibraryListKey.ARTIST_ALBUMS` (`MobileSurfaceViewModel.kt:28`) so the album
section gets its own scroll anchor, and give it a test tag in the
`LibraryListKey.testTag()` dispatch (`LibraryTrackRows.kt:165`) — that `when` is
exhaustive, so the compiler will demand it.

**Verify.** Full Android suite, command as in step 4.

**Depends on.** Step 4.

---

## Step 6 — Artists tab search covers albums and artists

**Goal.** With text in the Artists tab search field, results render as two
labelled sections — **albums first, then artists**. An album hit opens the album
page directly. With an empty field the tab lists artists only. The field label
changes from artists-only wording to one naming both.

**Test first.** In the same focused suite as step 5:

1. *A search returns album hits and artist hits in separate sections, albums
   first.* Fake port whose `searchAlbums` and `searchArtists` both return rows;
   assert both section headings exist and assert the album section precedes the
   artist section. Red first on the missing album section.
2. *An album hit opens the album page in one step* — not via the artist.
3. *An empty search shows artists only*, no album section. This is the test that
   keeps the removed library-wide album list from creeping back in through the
   search path.
4. *The field label names both.* Assert on the new label text; this is what makes
   the string change deliberate rather than incidental.

**Implementation.** `LibrarySearchField` (`BrowseTabs.kt:83`), `ArtistsTab`
(`:193`), and the `artistsFor` wiring in `BrowseScreen.kt` (the search dispatch
at line 254 and the tab load at 319). `searchAlbums` stays on the port and keeps
its FFI counterpart; it is repurposed, not removed.

**Verify.** Full Android suite.

**Depends on.** Step 5 (the album hit needs the nested album route to exist).

---

## Step 7 — Remove the two tabs from the Kotlin surface

**Goal.** `BrowseTab.ALBUMS` and `BrowseTab.FAVOURITES` are gone. Three tabs
remain: `TITLES`, `ARTISTS`, `QUEUE`. The Rust enums are untouched in this step.

**Test first.** The tab-count test is the one to flip:
`BrowseSurfaceTest.kt::libraryFrameUsesTheExactTwoAMetricsAndAllFiveBrowseDestinations`
(line 340) asserts all five destinations by name (lines 353–357). Rewrite it
first to expect exactly three, in order — it goes red immediately and honestly,
because the enum still has five. Rename it too; a test whose name says "all five"
while asserting three is a defect waiting to confuse the next reader.

Also update `emptyBrowseMessagesNameTheFilteredDestination`
(`BrowseSurfaceTest.kt:477`) — drop its `ALBUMS` and `FAVOURITES` cases (lines
479 and 481) and keep the `TITLES` / `ARTISTS` ones as the control, so the test
still proves the message mechanism works rather than becoming vacuous.

Then delete, in the same step, the tests that only exist for the removed tabs:

- `MainActivityMusicPathsTest.kt::favouritesKeepTheBoundaryOrderAndExplainAnEmptyList`
  (line 91) — it clicks the "Favourites" tab, asserts "Play favourites" and "No
  favourites yet.". This is a *list* test, not a marking test, and it goes with
  the tab. The spec's testing section does not name it; it was found by sweeping.
  Its `application.favouritesEmpty` fixture field goes with it.
- The `searchFavourites` half of
  `browseSearchesDelegateLiteralTextAndWindowToTheirPortMethods`
  (`BrowseSurfaceTest.kt:457`, assertion at line 470).
- The `albums` and `favourites` parameters of `RecordingBrowsePort`
  (`BrowseSurfaceTest.kt:544, 547`) and its `listFavourites` / `searchFavourites`
  overrides (lines 616, 621) — but **not** its `setFavourite` override (line
  651), which belongs to the surviving feature.

And retarget — do not delete — the two favourite-marking tests that currently
navigate through the removed tabs. The full instruction, including the rule that
their assertions may not be weakened, is in step 11 under *The guard, part two*.
Do the retargeting here, in the commit that forces it.

**Implementation.** Expected finds, all of which the compiler will point at once
the enum shrinks — this is a case where following the compiler is the sweep:

- `BrowseScreen.kt`: the enum (52–56); the `FAVOURITES`
  invalidate-on-select branch in `selectDestination` (153–154); `albumsFor`
  (218) and `favouritesFor` (230); the search dispatch arms (250, 258); the tab
  load arms (321, 323); `loadMoreFavourites` (392–400); the content-description
  arms (446, 449); the page composition arms (533, 587); the
  `visibleAlbums` / `visibleFavourites` / `albumsRequestedOffset` /
  `favouritesRequestedOffset` state (125–139).
- `BrowseTabs.kt`: `FavouritesTab` (260–286) goes entirely; the **list half** of
  `AlbumsTab` (122) goes — `AlbumRows` (305) and its `library-albums-list` tags
  (325, 344) with it — while the album *page* survives.
- `LibraryListKey` (`MobileSurfaceViewModel.kt:28`): `ALBUMS` and `FAVOURITES`
  and their arms in `LibraryListKey.testTag()` (`LibraryTrackRows.kt:165`).
- `LibraryScreenState.Browse` (`LibraryScreenState.kt:98`): the `albums` and
  `favourites` windows, and the same fields on the restore record
  (`MobileSurfaceViewModel.kt:55, 57`) and its catalog-shape fingerprint (`:82`).
- `MainActivitySurface.kt`: `listFavourites` and `searchFavourites` on
  `MainActivitySurfaceDependencies`, and `listAlbums` — but **keep**
  `searchAlbums`, which step 6 now depends on.
- `BrowseDestinationSettings.kt`: `toBrowseTab()` maps
  `AndroidStoredLibraryDestination.Albums` and `.Favourites` onto
  `BrowseTab.TITLES` (joining the existing `Unset` / `Unsupported` arm);
  `toLibraryDestinationChoice()` loses its `ALBUMS` / `FAVOURITES` arms. The Rust
  variants still exist at this point, so the `when` stays exhaustive and the file
  compiles. Step 8 finishes the job.

### The pager has to shrink with the enum

This was an open question at draft time; it is measured and closed. **Nothing
persists a `BrowseTab` ordinal.** The stored choice is a string id —
`AndroidStoredLibraryDestination::from_setting` matches `Some("albums")` and
`Some("favourites")` as literal strings (`appearance.rs:108–117`). Ordinals are
used only inside a session, by the pager, at exactly six places in
`BrowseScreen.kt`:

- `:148` `initialPage = selectedTab.ordinal`
- `:149` `pageCount = { BrowseTab.entries.size }`
- `:165–166` the tab-change effect compares against and animates to
  `selectedTab.ordinal`
- `:173` the settled-page collector maps back with `BrowseTab.entries[page]`
- `:512` the pager's `key = { page -> BrowseTab.entries[page] }`
- `:514` the page body's `BrowseTab.entries[page]`

So there is no migration to write here — the remaining task is to make the pager
follow `entries` from five down to three:

- `pageCount` follows automatically; confirm it, do not assume it.
- Because pages are keyed by the **constant** (`:512`) and not by the index, the
  saved page state of a removed constant is discarded rather than inherited by a
  surviving tab. Assert this: a session that had scrolled the Albums page must
  not hand that scroll state to whatever tab now occupies index 2.
- The two index paths (`:173`, `:514`) must never receive an index outside
  `entries`. With `pageCount` derived from `entries` they cannot, but the swipe
  path deserves a test: swiping between the three remaining tabs selects the
  right destination.
- `initialPage = selectedTab.ordinal` must be in range for a session whose
  remembered tab was Albums — covered end to end by step 8's behavioural test.

**Verify.** Full Android suite. Also confirm the three-tab test actually ran (see
the results-XML note under *Verification commands*).

**Depends on.** Steps 5 and 6.

---

## Step 8 — Migration: a saved Albums or Favourites tab lands on Titles

This is the named migration step. A user who last used the Albums tab must open
the app on **Titles** — no crash, no empty screen.

**Goal.** Remove `Albums` and `Favourites` from
`AndroidStoredLibraryDestination` and `AndroidLibraryDestinationChoice`
(`crates/reprise-android-ffi/src/appearance.rs:98` and `:122`), and prove the
stored strings still resolve.

**How it works on `origin/dev`.** `from_setting` (`appearance.rs:108–117`)
matches `Some("albums")` and `Some("favourites")` explicitly today, with a
catch-all `Some(id) => Self::Unsupported { id }`. Deleting the two explicit arms
makes those stored ids fall into `Unsupported` **by construction**, and
`BrowseDestinationSettings.toBrowseTab()` already maps `Unsupported` to
`BrowseTab.TITLES`. The mechanism exists; this step's job is to prove it, not to
build it.

**Test first.** Two tests, one per side of the boundary:

1. Rust, in the `mod tests` of `appearance.rs` (it already covers `Unset`,
   `Unsupported` and a `set_library_destination` round-trip at lines 341–364 —
   extend that module): a settings row holding the literal string `"albums"`
   reads back as `AndroidStoredLibraryDestination::Unsupported { id: "albums" }`,
   and likewise for `"favourites"`. Write this **while the `Albums` variant still
   exists** — it fails, asserting `Unsupported` against an actual `Albums`, and
   that failure is the proof the test is wired to the real parser and not to a
   stub. Then delete the variants and watch it go green.
2. Kotlin, in a focused suite: `AndroidStoredLibraryDestination.Unsupported("albums")
   .toBrowseTab() == BrowseTab.TITLES`, same for `"favourites"`, and
   `Unset.toBrowseTab() == BrowseTab.TITLES`. Then the behavioural one that
   matters: an activity restored with a stored destination of `"albums"` opens on
   the Titles tab **with titles rendered** — not a blank page. Assert on visible
   content, not just on the selected-tab value; "no crash" and "no empty screen"
   are two different claims and only the second needs content.

**Implementation.** Delete the four enum variants and their `from_setting` /
`setting_id` arms; drop the now-dead arms in `BrowseDestinationSettings.kt`.
Regenerate the uniffi bindings. Leave the stale `"albums"` string in the user's
settings row alone — it is harmless, it resolves to `Unsupported`, and it is
overwritten the next time the user picks a tab. Do not write a migration that
rewrites stored rows; there is nothing to gain and a settings write on startup is
a new failure mode.

**Verify.**

```bash
env TMPDIR=/tmp cargo test --locked -p reprise-android-ffi appearance
cd android && ./gradlew --max-workers=2 \
  -Pkotlin.compiler.execution.strategy=in-process :app:testDebugUnitTest
```

**Depends on.** Step 7. Parallel with step 9.

---

## Step 9 — Remove the dead data paths

**Goal.** Delete the query paths that no longer have a caller.

**Test first.** Nothing new is written here; this step is proven by *deletions*
staying green and by one guard: before deleting `filtered_track_window`
(`filtered_browse.rs:11`), grep for callers and confirm `search_favourites` is
the only one. It is, on `origin/dev` — but confirm rather than trust, because a
step 5 or 6 change could have added one.

**Implementation.** Expected finds:

- Rust: `list_favourites` (`filtered_browse.rs:76`), `search_favourites` (`:80`),
  `filtered_track_window` (`:11`), and their tests
  `favourites_are_exactly_fives_in_artist_album_track_order` (`:252`),
  `favourite_search_keeps_the_filter_count_and_window_in_step` (`:281`),
  `empty_favourites_are_an_empty_window_not_an_error` (`:317`). `filtered_browse.rs`
  itself **stays** — `list_artist_tracks` and the step 3 additions live there.
- Rust: `list_albums` (`lib.rs:153`) and the test using it (`lib_tests.rs:202`).
  **`search_albums` (`lib.rs:157`) stays** — step 6 depends on it — as do the
  `search_albums` tests (`lib_tests.rs:260–278`) and the `AlbumRow` /
  `AlbumWindow` types.
- Kotlin: `listAlbums`, `listFavourites`, `searchFavourites` on
  `LibrarySessionPort` (`LibrarySession.kt:28, 41, 43`), on `LibrarySession`
  (`:132, 171, 174`), and on `AndroidLibrarySessionPort` (`:75, 97, 100`).
  **`searchAlbums` stays** at all three levels. Also the `BrowseTab.ALBUMS`-guarded
  prefetch in `LibrarySession.kt` around lines 242–246, which the spec notes is
  the only caller of `listAlbums` outside the tab body.
- Every port double in `android/app/src/test/` that overrides the removed
  methods — the compiler finds them.

**Do not touch** `crates/reprise-mcp/src/data.rs::search_albums` or the
`music_search_albums` MCP tool. Same name, different subsystem, no relation to
Android. Equally, do not delete `query_artist_detail_albums` on the grounds that
it has no production caller — see *Open questions*.

**Verify.**

```bash
cargo test --workspace --locked
env TMPDIR=/tmp cargo test --locked -p reprise-android-ffi
cd android && ./gradlew --max-workers=2 \
  -Pkotlin.compiler.execution.strategy=in-process :app:testDebugUnitTest
```

**Depends on.** Step 7. Parallel with step 8.

---

## Step 10 — Repo-wide sweep for orphans

**Goal.** No orphaned reference to the two removed tabs survives anywhere in the
repository. The compiler catches Kotlin and Rust; it does not catch strings, test
tags, shell scripts, or prose. This step is where those are found.

**Test first.** The sweep's own test is the grep coming back empty. Run these
from the repo root **before** editing, record the hits, then re-run after and
require the only remaining hits to be the historical records named below:

```bash
git grep -n "BrowseTab\.\(ALBUMS\|FAVOURITES\)"
git grep -n "LibraryListKey\.\(ALBUMS\|FAVOURITES\)"
git grep -n -i "listAlbums\|listFavourites\|searchFavourites"
git grep -n -i "list_albums\|list_favourites\|search_favourites\|filtered_track_window"
git grep -n "library-destination-ALBUMS\|library-destination-FAVOURITES"
git grep -n "library-page-ALBUMS\|library-page-FAVOURITES"
git grep -n "library-albums-list\|library-favourites-list"
git grep -n -i "favouritesEmpty\|Play favourites\|No favourites yet"
git grep -n "AndroidStoredLibraryDestination\|AndroidLibraryDestinationChoice"
git grep -n "Individual titles"
```

Categories to work through, with expected finds as a starting point — search for
more, do not stop at this list:

- **Generated test tags.** `testTag("library-destination-${destination.name}")`
  (`BrowseScreen.kt:518`) and `testTag("library-page-${tab.name}")`
  (`LibraryFrame.kt:186, 235`) are built from enum names, so the *producers* fix
  themselves — but the *assertions* are literal strings. Expected finds:
  `MobileBottomTabsTest.kt:52, 77–87`, `MobileHeaderRowTest.kt:100–101`,
  `MainActivityQueueTest.kt:145`, `MobileSurfaceStateTest.kt:146–147`.
- **Compose behaviour fixtures.** `ComposeBehaviorTest.kt` passes
  `listAlbums`/`searchAlbums` in several places (around lines 179, 282, 375, 579,
  637, 778). `searchAlbums` stays; `listAlbums` goes.
- **Labels and strings.** The `BrowseTab` constructor carries `label` and
  `symbol` (`BrowseScreen.kt:52`), so the tab labels leave with the constants;
  check `android/app/src/main/res/` for any remaining literal. The last grep above
  catches any stray "Individual titles" left over from the drafting of step 5 —
  the heading is "Other titles".

**The sweep stops here — do not rewrite historical records.** Completed plans,
handoffs, verification protocols, progress logs and mockup transcripts under
`docs/` record what was true when they were written. Editing them destroys the
record and produces a diff nobody can review. Specifically leave alone:
`.superpowers/sdd/progress.md`, `docs/superpowers/plans/2026-08-0*-mobile-*.md`
entries that are marked complete, `docs/plans/*.HANDOFF.md`,
`docs/research/android-spike-2026-08.md`, `docs/adr/`, and any plan whose ledger
entry in `AGENTS.md` reads `shipped`. If a historical document is actively
misleading, the fix is a dated note appended to the ledger — not a rewrite.

Living documentation is a different matter: update docs that describe the mobile
browse surface *as it currently is* — expected finds include
`docs/superpowers/plans/2026-08-08-mobile-bottom-tabs.md` (describes a four-tab
bottom bar) and any living reference under `docs/`. Judge each by whether it
claims to describe the app now or to record what was done then.

**Two traps that look like hits but are not:**

1. `scripts/cua-e2e/run.sh:250`, `scripts/cua-e2e/responsive_window.sh:199` and
   `scripts/tests/cua-e2e.sh:405, 688–712` mention "Albums". These are **GTK
   desktop** assertions — `run.sh:250` asserts the desktop track table has *no*
   "Albums" mode tab. The desktop is explicitly out of scope. Leave them.
2. `crates/reprise-mcp/` `search_albums` / `music_search_albums` — a different
   subsystem that happens to share a name. Leave it.

**Verify.** The greps above return only historical-record hits, plus a full green
gate battery, judged against the step 0 baseline.

**Depends on.** Steps 7, 8, 9.

---

## Step 11 — Regression guard and full gate

**Goal.** Prove the favourite *feature* survived while the favourite *list* left,
and prove the gates are no worse than they were at step 0.

**The guard, part one — must pass unchanged.** No edits to their bodies, no
retargeting. If any of these needs touching, the change went further than the
spec allows and that is a stop-and-reconsider signal, not a test to fix:

- `MainActivityRatingTest.kt::sheetDockAndLibraryReadOneHeartWithoutRestoreMemory`
  (line 76) — one heart state read consistently by sheet, dock and library.
- `MainActivityRatingTest.kt::refusedHeartWriteMovesNoSurfaceAndExplainsTheFailure`
  (line 176).
- `MaterialSymbolFillTest.kt::theFavouriteAndOrdinaryHeartAreNotTheSameBitmap`
  (line 82) — the filled/outline heart still renders as two distinct bitmaps.
- `ComposeBehaviorTest.kt::failedFavouriteShowsTheErrorWithoutMovingTheHeart`
  (line 96).
- `BrowseSurfaceTest.kt::theDisconnectedControlsDoNothingAndNeverClaimARatingWasSaved`
  (line 116) and `::aRepeatedRatingFailureIsANewMessageWithItsOwnLifetime`
  (line 100) — the two `controls.setFavourite(...)` tests the spec names.
- `RatingWriterTest.kt` in full — the write path
  (`PlaybackControls.setFavourite` → `MainActivity` → `RatingWriter` →
  `set_track_rating`).
- The `setFavourite` path in `TrackContextMenuTest.kt`.

**The guard, part two — retargeted, not deleted, and not weakened.** Two
favourite-marking tests prove the write *by observing the Favourites list*, so
they cannot pass unchanged once the list is gone. They are still the right tests,
asking the right question through the wrong window. This is the decided
resolution of the spec's "must pass unchanged" wording, and it is not open for
re-litigation: deleting them would remove exactly the evidence this step exists to
produce.

**The binding rule: the claim may not get smaller.** Each retargeted test keeps
every assertion it makes today, word for word in meaning — 5 written, then 0
written, and both survive a fresh activity read. Only the surface the state is
read from changes. If the retargeted version asserts less than the original — if
the post-`recreate()` check is dropped, if `ratingRequests`/`trackRatings` stop
being asserted, if "the heart shows the unfavourited state" degrades to "the click
was recorded" — **the step is not done**. A weaker test that passes is worse than
the red one it replaced.

- `MainActivityRatingTest.kt::libraryHeartWritesFiveThenZeroAndBothSurviveAFreshActivityRead`
  (line 54) — the strongest guard in the suite, and the one that breaks. It calls
  the private helper `favouriteTrack()` (line 198) at lines 63, 69 and 72, and
  that helper is defined as a node with
  `hasAnyAncestor(hasTestTag("library-page-FAVOURITES"))` — a hard-coded page tag.
  The test therefore reads "rating written" as "the row appeared in / vanished
  from the Favourites tab".

  Retarget it onto `libraryHeart(description)` (line 187) in the same file, which
  already asserts on the heart's own content description on the track row and
  resolves its page tag from `application.rememberedDestination.name` rather than
  from a literal — so it follows whichever tab survives. The replacement helper is
  already in the file; no new mechanism is needed. What must remain after the
  edit: the `application.controls.ratingRequests` and `application.trackRatings`
  assertions unchanged, and the `recreate()` at the end followed by an assertion
  that the freshly read activity still shows the *unfavourited* heart. That last
  line is the whole point of the test's name.
- `MainActivityConfigurationTest.kt` (the walk around lines 242–255) clicks
  `"Favourites"`, asserts a heart inside `library-page-FAVOURITES` (line 249),
  then clicks `"Albums"` and opens `DEEP_ALBUM`. Both halves need rerouting: the
  heart assertion onto a surviving list, and the deep-album navigation through the
  artist page built in step 5. The walk must still cover the same ground — a
  long-list scroll, a heart toggle that changes what is displayed, and a deep
  album reached by navigation — not a shortened version of it. Its
  `library-page-FAVOURITES` reference is one of the sweep hits from step 10.

Do the retargeting in step 7, alongside the removal that forces it, and say so in
the commit message — an unexplained edit to the strongest favourite test in a
commit titled "remove the favourites tab" is exactly what a reviewer should
challenge.

Equally untouched in production code: `FavouriteHeart.kt` and its call sites in
`LibraryTrackRows.kt`, `DockMode.kt`, `NowPlayingScene.kt`, `NowPlayingSheet.kt`,
plus `ActivityPlaybackControls.kt:41, 62–66`. A diff that touches
`FavouriteHeart.kt` at all is a signal something drifted.

**Full gate.**

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace --locked
cargo audit
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # empty
env TMPDIR=/tmp cargo test --locked -p reprise-android-ffi
cd android && ./gradlew --max-workers=2 \
  -Pkotlin.compiler.execution.strategy=in-process :app:testDebugUnitTest
```

**Judge it against the step 0 baseline, not against "all green".** Write the
comparison down:

- A suite red at baseline and still red the same way is **not a finding**. Name it
  and move on; do not repair it inside this change.
- A suite green at baseline and red now **is a finding** and blocks the step.
- A suite red at baseline that this change was supposed to fix must be named
  explicitly as fixed, with the test that proves it.
- If no baseline record exists, this step cannot be completed. Do not reconstruct
  one after the fact from a dirty tree — a baseline taken after the edits proves
  nothing.

Compare the Android test total against the **baseline total from step 0**, not
against a number written here. Removing tabs removes tests, so the total
legitimately drops — account for the drop test by test against the deletions
listed in steps 7 and 9 rather than accepting it. A drop larger than those
deletions means a suite silently stopped running.

Confirm the file-size rule: every code file created or substantially edited ends
under 800 lines. `BrowseScreen.kt` (734), `BrowseTabs.kt` (455) and
`BrowseSurfaceTest.kt` (654) are the ones near the line; all three should shrink,
but check rather than assume. `library_views.rs` starts at 523 and grows in steps
1 and 2 — check it too.

**Depends on.** Everything.

---

## Open questions

Only one thing is genuinely open, and it is **a follow-up task after this rework,
not a decision for the implementer of this plan**. Do not act on it inside these
eleven steps.

1. **Should the two artist→albums queries be unified outright?** After step 1
   there are still two of them in `library_views.rs`: `query_artist_detail_albums`
   (unwindowed, `Vec<ArtistAlbum>`, `COALESCE(MAX(year), 0)`) and the new
   `query_artist_albums` (windowed, `AlbumWindow`/`AlbumSummary`,
   `MIN(CASE WHEN year > 0 THEN year END)`). Step 1 shares the *selection rule*
   between them, which is what stops the definition of "album by this artist"
   from drifting; it deliberately leaves the projections, the windowing and the
   return types separate, because unifying those would change desktop code that
   this spec puts out of scope.

   Two measured facts belong in that later decision. First,
   `query_artist_detail_albums` currently has **no production caller** on
   `origin/dev` — only its definition, a doc cross-reference, one unit test, and a
   historical plan document mention it. Second, the year projections genuinely
   disagree (`MAX` vs `MIN` over non-zero years), so "unify" is not a rename; it
   is a behaviour decision about which year an album with inconsistent tags
   should report. Whoever picks this up should establish whether the desktop
   still wants that query at all before merging anything into it.
