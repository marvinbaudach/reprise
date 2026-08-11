# Releases: a deleted album stays deleted

Date: 2026-08-11
Status: implemented; accepted review findings applied
Baseline: `origin/dev` @ 5995f70e77 (the working checkout is 156 commits
behind; every line reference below is read from `origin/dev`)

## Problem

The gap catalog hides a release once the library owns it — NR-24: complete
coverage, more than half the official track count, or, for a single, any
local track under that title. Ownership is computed live from `tracks`
(`local_library_index`, `artist_news_query.rs:91`).

Delete the album and that computation flips back: no local tracks, no
ownership, the row returns as a gap. The catalog then advertises a record
the listener deliberately threw away, forever, and re-advertises it on every
badge and popover that shares the filter (NR-26, NR-29).

There is no state to consult instead. "Remove from library" records a scan
exclusion and deletes the row through `exclude_tracks_matching_paths`; the
trash path (`library/trash_tracks.rs`) removes the row after the filesystem
action succeeds. The tombstone/`purge_tombstones` path belongs only to the
Missing-files flow, where a removal must never be interpreted as deliberate.
`change_log` records only `entity_id` for a deleted track, so the metadata
needed to identify the release is gone with the row — **nothing can be
backfilled**. The memory starts the day this ships.

A second, unrelated complaint from the same screenshot: the second column is
headed `Title` (`RELEASES_TITLE`, `strings_releases.rs:13`) while the rows
are albums, EPs and singles. Next to `Artist` and `Type`, "Title" reads as a
song title.

## Decisions (grilled 2026-08-11, all final)

1. **Which deletions count** — only deliberate ones: "move to trash" and
   "remove from library". A file that merely goes missing (unmounted drive,
   moved folder) never writes the memory; that state is what the Missing
   view is for.
2. **Threshold** — album-scope memory is written only when nothing of the
   album is left: after the deletion no track of that album remains in the
   library. Deleting one song while keeping another song from the album does
   not hide the album row. It does write track-scope memory for the deleted
   song, so a same-titled catalog `single` is hidden; singles are matched by
   song title under NR-24 and the deleted song itself is their unit.
3. **Effect** — the memory sets the release `hidden`, the same state the
   Hidden chip already shows. Table, sidebar badge and Updates popover all
   exclude hidden rows already (NR-26, NR-29), so they follow with no extra
   wiring, and the decision stays reversible through the existing "Show
   again" action.
4. **Reversal** — un-hiding a release deletes its memory entry. Without
   that, the next catalog sync would silently re-hide what the user just
   restored.
5. **No toast.** Deleting tracks says nothing about the Releases view. The
   Hidden chip is where the explanation lives.
6. **Column rename** — `Title` becomes `Release`.

## Data — table and migration

New table, created by migration **v69** (the implementation base already used
versions through 68):

```sql
CREATE TABLE deleted_releases (
  artist_key TEXT NOT NULL,   -- normalize(album artist, falling back to artist)
  title_key  TEXT NOT NULL,   -- normalize(album) or normalize(track title)
  scope      TEXT NOT NULL,   -- 'album' | 'track'
  deleted_at INTEGER NOT NULL,
  PRIMARY KEY (artist_key, title_key, scope)
);
```

Two scopes because the catalog matches two identities: albums and EPs by
*(album artist, album)*, singles by *(album artist, track title)* — exactly
the split `LocalLibraryIndex` already carries. A deleted song therefore
writes a `track` row, and, when it was the last one of its album, an `album`
row as well.

Keys are produced by `artist_news::normalize` — the same function the
ownership match uses. No second normalization may be introduced; a
divergent predicate here would hide the wrong rows in one place and not the
other.

`ON CONFLICT DO NOTHING`: re-deleting a release keeps the original
`deleted_at`.

Review follow-up migration **v70** adds
`new_releases.hidden_by_deleted_memory INTEGER NOT NULL DEFAULT 0`. Only the
memory reconciler sets this provenance bit; automatic reversal only clears
rows carrying it, so a matching row hidden by hand stays hidden. A partial
index over those rows keeps undo reconciliation proportional to its targets.

## Write path — where the memory is recorded

At the two deliberate deletion paths, inside the transaction and before the
database rows disappear:

- `library::trash_tracks::trash_tracks_with` — after `trash_action`
  succeeds.
- `queries::exclude_tracks_matching_paths` — the user-facing "Remove from
  library" action, together with its persistent scan exclusion.

Deliberately **not** in `purge_tombstones` or the shared id-only
`remove_tracks_impl`: those serve Missing-files cleanup and auto-clean, which
decision 1 excludes. Undoing a tombstone reconciles only the exact ids restored
by the undo because a returned sibling may reacquire an album, but neither
tombstoning nor purging writes new memory or performs a full-library scan on
the UI caller.

Both call one shared core function so the rule exists once:

```rust
pub fn remember_deleted_releases(conn: &Connection, ids: &[i64], now: i64)
```

It runs inside the caller's transaction, **before** the rows disappear:

1. Read `artist`, `album_artist`, `album`, `title` for `ids`.
2. For each distinct *(artist key, album key)*: write an `album` entry only
   if no other track of that album survives the deletion — `removed_at IS
   NULL AND id NOT IN (ids)`, with **no** `missing_since` filter. A merely
   missing sibling counts as still owned, so an unmounted drive cannot
   fabricate a memory. (This predicate is intentionally wider than
   `local_library_index`'s; document why at the call site.)
3. For each distinct *(artist key, title key)*: write a `track` entry under
   the same surviving-row test.
4. Apply the memory immediately (next section), so the row disappears from
   the view within the same interaction.

Tracks with an empty album contribute a `track` entry only — the same
decision `local_library_index` documents.

## Apply path — where the memory takes effect

Reconciliation runs once after a complete catalog refresh, not once per
artist, and returns immediately when the memory table is empty:

```rust
pub(crate) fn apply_deleted_release_memory(conn: &Connection) -> Result<usize, rusqlite::Error>
```

- Loads `deleted_releases` and the catalog rows' `(artist_name, title,
  release_type, release_group_mbid, hidden)`, matches in Rust with
  `normalize` (the catalog is on the order of a thousand rows; matching in
  SQL would mean a second normalization definition).
- An `album`-scope entry hides matching rows of **any** type — a work
  MusicBrainz carries twice, as album and EP, must not survive as its EP
  twin.
- A `track`-scope entry hides matching rows of type `single` only.
- Hiding uses the dedicated memory-owned update, which stamps both `hidden_at`
  and `hidden_by_deleted_memory`. Rows already hidden are left untouched.

The deliberate deletion transaction reuses its survivor scan and reconciles
the catalog once, avoiding a second full library scan while still removing the
catalog row in the same interaction. Each artist upsert transaction hides its
newly fetched remembered rows before the progress callback can repaint. The
refresh pipeline then calls the full reconciliation exactly once after the
loop, even if a mid-loop error stops the refresh. This catches re-acquisitions
without multiplying full library/catalog scans by the number of artists.

**Re-acquisition:** the same pass drops entries whose release is back in the
library (present in `LocalLibraryIndex`) and un-hides those rows. The
provenance bit marks which hidden rows the memory owns, so clearing it cannot
overwrite a user decision.

## Reversal path

`set_release_hidden_in(conn, mbid, false)` — reached from both
`restore_release` (`artist_news_history.rs:316`) and the view's per-row action
— transactionally deletes the memory scopes that hid the selected row and
un-hides each memory-owned catalog row no longer covered by any surviving
scope. Thus showing one album/EP twin also restores the other, while an
independent track-scope memory and manually hidden rows remain intact.
Re-acquisition deletes and reverses only its acquired scope. Hiding (`true`)
writes nothing: only a deliberate deletion creates memory.

## Column rename

`RELEASES_TITLE` becomes `N_!("Release")`. `column_contract`
(`releases_columns.rs:26`) is unchanged in shape; `po/reprise.pot` is
regenerated and `po/de.po` re-translated ("Veröffentlichung" is not used —
the German string is "Release" too, matching the view's own name).

## Rules (`docs/ux-rules.md`, section R)

Append-only, both `[active]` in the implementing commit:

- **NR-32** `[core]` `[gtk]` — A release the listener deliberately deleted
  does not return as a gap. When "move to trash" or "remove from library"
  finally removes the last track of an album, or a song matched by a
  single, that release is remembered and hidden, including when the catalog
  only learns of it later. Missing files never trigger this. Restoring the
  release through "Show again" forgets it, and so does re-acquiring it.
- **NR-33** `[gtk]` — replaces NR-31. The gap view's columns are `Cover ·
  Date · Release · Artist · Type · Status · Link`; the second text column is
  named `Release` because the rows are albums, EPs and singles, not songs.
  Sorting, filters, counts, activation semantics, the trailing action column
  and zero-result recovery remain exactly as NR-31 specified.

NR-31 gets `[replaced by NR-33]`, its text otherwise untouched. Keep
`docs/plans/ux-rules-acceptance-tests.md` in sync.

## Tests (rule-named, TDD)

Core (`artist_news_query_tests.rs`, `artist_news_view_tests.rs`, the
maintenance/trash test modules):

- `nr_32_deleting_the_last_track_of_an_album_hides_its_gap`
- `nr_32_deleting_one_track_keeps_the_album_gap_but_hides_its_single`
- `nr_32_deleted_song_hides_only_its_single_row`
- `nr_32_missing_file_writes_no_memory` (whole missing album is purged → no entry)
- `nr_32_undone_removal_writes_no_memory` (tombstone + undo inside the window)
- `nr_32_memory_applies_to_a_release_fetched_later` (complete refresh path)
- `nr_32_album_memory_also_hides_the_ep_twin`
- `nr_32_show_again_forgets_the_selected_release_scope`
- `nr_32_show_again_restores_every_row_hidden_by_the_same_album_memory`
- `nr_32_reacquiring_the_album_forgets_the_deletion`
- `nr_32_reacquiring_an_album_keeps_its_absent_same_titled_single_hidden`
- `nr_32_badge_and_popover_follow_the_memory` (NR-26/NR-29 coherence)
- migrations v69/v70: the memory table and hide provenance exist, upgrades are
  idempotent, and pre-existing hidden rows remain manual

GTK: `nr_32_deleted_release_memory_is_reflected_in_releases_view` proves the
default rendered catalog omits the hidden row, and `nr_33_column_contract`
pins the header row. The first is a display test and needs an X server — run
it isolated, and do not report it green unless it ran.

## Verification

- `cargo test -p reprise-core artist_news`, `cargo test -p reprise-core`
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt`
- `scripts/check-architecture.sh`, `scripts/check-ux-traceability.sh`
- Display tests need Xvfb and must be run separately; state plainly which
  suites ran. Check pass counts, not exit status.

## Known limits and out of scope

- **No backfill.** Albums deleted before this ships are not remembered —
  `change_log` kept no metadata. Hide them by hand once.
- A memory entry keyed by *(artist, title)* also hides a genuinely different
  release of the same name by the same artist (a re-recording under the same
  title). Accepted: the same key already governs ownership matching.
- No new UI: no toast, no separate "Deleted" chip, no preference.
- The write path does not touch the Missing/auto-clean flows, the scanner,
  or what the fetch requests.
