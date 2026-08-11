# Releases: a deleted album stays deleted

Date: 2026-08-11
Status: design approved, not yet implemented
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

There is no state to consult instead. "Remove from library" writes a
tombstone (`removed_at`) purely for the ten-second undo and
`purge_tombstones` (`queries/maintenance.rs:761`) then deletes the row for
real; the trash path (`library/trash_tracks.rs`) removes the row outright.
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
2. **Threshold** — a release is remembered only when nothing of it is left:
   after the deletion no track of that album remains in the library. A
   single deleted song off an album changes nothing. For singles, which
   NR-24 matches by song title, the deleted song itself is the unit.
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

New table, created by migration **v63** (highest version on `origin/dev` is
62 — re-check before writing the file):

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

## Write path — where the memory is recorded

At the two points where the deletion becomes final, never at the point where
it is requested:

- `library::trash_tracks::trash_tracks_with` — after `trash_action`
  succeeded and the rows are gone.
- `queries::purge_tombstones` — when the undo window has closed. An undone
  removal thus leaves no memory, for free.

Deliberately **not** in `remove_tracks_impl`: that is also the auto-clean
path for rows the scanner found gone, which decision 1 excludes.

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

One function, called from two places:

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
- Hiding reuses `set_release_hidden_in` (`artist_news_query.rs:372`), which
  also stamps `hidden_at`. Rows already hidden are left untouched.

Called from:

1. `remember_deleted_releases`, right after writing.
2. `sync_releases` (`artist_news_pipeline.rs:463`), inside its existing
   transaction after the upsert loop — this is what catches a release the
   catalog had not fetched yet when the deletion happened. Without it, the
   next MusicBrainz fetch hands the gap straight back.

**Re-acquisition:** the same pass drops entries whose release is back in the
library (present in `LocalLibraryIndex`) and un-hides those rows. The
`hidden` flag is provably the memory's own in that case — a manual hide
never writes a memory entry, and "Show again" deletes it (below) — so
clearing it cannot overwrite a user decision.

## Reversal path

`set_release_hidden_in(conn, mbid, false)` — the single definition of
un-hidden, reached from both `restore_release`
(`artist_news_history.rs:316`) and the view's per-row action — deletes every
`deleted_releases` entry matching that row's artist and title, in both
scopes. Hiding (`true`) writes nothing: only a deletion creates memory.

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
- `nr_32_deleting_one_track_of_an_album_keeps_the_gap` (threshold)
- `nr_32_deleted_song_hides_only_its_single_row`
- `nr_32_missing_file_writes_no_memory` (missing sibling survives → no entry)
- `nr_32_undone_removal_writes_no_memory` (tombstone + undo inside the window)
- `nr_32_memory_applies_to_a_release_fetched_later` (sync_releases path)
- `nr_32_album_memory_also_hides_the_ep_twin`
- `nr_32_show_again_forgets_the_deletion` (unhide → next sync leaves it visible)
- `nr_32_reacquiring_the_album_forgets_the_deletion`
- `nr_32_badge_and_popover_follow_the_memory` (NR-26/NR-29 coherence)
- migration v63: table exists, is idempotent, empty on upgrade

GTK: `nr_33_column_contract` pins the header row (display test, needs an X
server — run it isolated, do not report it green unless it ran).

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
