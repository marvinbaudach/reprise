---
slug: releases-deleted-memory
worktree:
branch:
phase: reviewed
codex_session:
created: 2026-08-11
---
# Releases: a deleted album stays deleted

Design spec: `docs/superpowers/specs/2026-08-11-releases-deleted-memory-design.md`
— copy it into the worktree and commit it alongside the implementation. Every
decision in it was grilled with the user on 2026-08-11 and is final: do not
re-open them, do not add scope beyond them.

## Goal

Two things, both visible in the Releases (gap catalog) view:

1. A release the listener **deliberately deleted** must not come back as a
   gap — not in the table, not in the sidebar badge, not in the Updates
   popover. Ownership is computed live from `tracks`, so today deleting an
   album restores its gap row permanently.
2. The second text column is headed `Title` while its rows are albums, EPs
   and singles. It becomes `Release`.

## Baseline and repo rules that bind this task

- Branch from `origin/dev` (`worktree.sh` already does). The main checkout is
  far behind; never read the shape of this feature from it.
- `docs/ux-rules.md` is the binding UX contract. Rule IDs are **append-only**:
  never edit an old rule's text — mark it `[replaced by <NEW-ID>]` and append
  the new one. A rule flips to `[active]` in the same commit that implements
  it, so NR-32 and NR-33 land `[active]` here.
- Every `[active]` rule needs a rule-named test (`fn nr_32_…`).
- Every file created or substantially edited ends **< 800 lines**. Several
  files this task touches are already close (`artist_news_query.rs` 494,
  `artist_news_history.rs` 555, `queries/maintenance.rs` 796,
  `releases_columns.rs` 798). Put new logic in a **new module** rather than
  growing them; `maintenance.rs` and `releases_columns.rs` have no room at all.
- Immutability: build new values, never mutate shared state in place.
- One definition per decision. The normalization used to match releases
  against the library is `artist_news::normalize` — do not write a second one.
- Never run `git stash` — the repo is shared with parallel worktrees.
- Make focused commits. Do not touch files outside this worktree.

## Package 1 — the memory itself (TDD, core)

New module, e.g. `crates/reprise-core/src/deleted_releases.rs`, re-exported
where callers need it. Tests first.

### 1.1 Schema

Migration **v69** — the highest `user_version` in the worktree is 68; verify
before writing and take the next free number if the tree has moved again.
Follow the existing migration pattern (own
`db_*.rs` file with `migrate_vNN`, registered in `db.rs`, guarded by
`PRAGMA user_version`).

```sql
CREATE TABLE deleted_releases (
  artist_key TEXT NOT NULL,
  title_key  TEXT NOT NULL,
  scope      TEXT NOT NULL,   -- 'album' | 'track'
  deleted_at INTEGER NOT NULL,
  PRIMARY KEY (artist_key, title_key, scope)
);
```

Keys are `artist_news::normalize` output. `artist_key` uses `album_artist`
falling back to `artist`, exactly like `local_library_index`. Writes are
`ON CONFLICT DO NOTHING`, so re-deleting keeps the original `deleted_at`.

### 1.2 `remember_deleted_releases(conn, ids, now)`

Runs inside the caller's transaction, **before** the rows are gone.

1. Read `artist`, `album_artist`, `album`, `title` for `ids`.
2. Write an `album` entry for a distinct *(artist key, album key)* only when
   no other track of that album survives:
   `removed_at IS NULL AND id NOT IN (ids)` — deliberately **without** a
   `missing_since` filter, so a merely missing sibling still counts as owned
   and an unmounted drive cannot fabricate a memory. This predicate is wider
   than `local_library_index`'s on purpose; say why in a comment at the site.
3. Write a `track` entry for a distinct *(artist key, title key)* under the
   same surviving-row test.
4. A track with an empty album contributes the `track` entry only.

### 1.3 `apply_deleted_release_memory(conn) -> usize`

- Load `deleted_releases` and the catalog rows' `(release_group_mbid,
  artist_name, title, release_type, hidden)`; match in Rust with `normalize`.
  Do not normalize in SQL — that would be the second definition.
- `album` scope hides matching rows of **any** type (a work MusicBrainz
  carries twice, as album and EP, must not survive as its EP twin).
- `track` scope hides matching rows of type `single` only.
- Hide through `set_release_hidden_in`, which also stamps `hidden_at`. Rows
  already hidden stay untouched.
- **Re-acquisition:** in the same pass, drop entries whose release is back in
  the library (present in `LocalLibraryIndex`) and un-hide those rows. This
  cannot overwrite a user decision: a manual hide never writes an entry, and
  "Show again" deletes it (1.4).
- Returns how many rows it hid, for the tests.

### 1.4 Reversal

`set_release_hidden_in(conn, mbid, false)`, reached from `restore_release` and
the view's row action, deletes the memory scopes that hid the selected row and
un-hides every row those entries hid. Album/EP twins therefore return
together, while an unrelated track scope remains. Re-acquisition reverses only
the scope that returned. Hiding (`true`) writes nothing: only a deletion
creates memory.

### 1.5 Tests (package 1)

- `nr_32_deleting_the_last_track_of_an_album_hides_its_gap`
- `nr_32_deleting_one_track_keeps_the_album_gap_but_hides_its_single`
- `nr_32_deleted_song_hides_only_its_single_row`
- `nr_32_missing_sibling_writes_no_memory`
- `nr_32_album_memory_also_hides_the_ep_twin`
- `nr_32_show_again_forgets_the_selected_release_scope`
- `nr_32_show_again_restores_every_row_hidden_by_the_same_album_memory`
- `nr_32_reacquiring_the_album_forgets_the_deletion`
- `nr_32_reacquiring_an_album_keeps_its_absent_same_titled_single_hidden`
- migration: table exists, upgrade is idempotent, starts empty

## Package 2 — wiring the two deliberate deletion paths (TDD, core)

Depends on package 1.

- `library::trash_tracks::trash_tracks_with` — call
  `remember_deleted_releases` after `trash_action` succeeded and before the
  rows are removed, then immediately hide matching catalog rows.
- `queries::exclude_tracks_matching_paths` — do the same inside the
  user-facing "Remove from library" transaction that records scan exclusions.
- Deliberately **not** `purge_tombstones` or the shared id-only
  `remove_tracks_impl`: those serve Missing-files cleanup and auto-clean for
  rows the scanner found gone. Leave a comment so a later reader does not
  "fix" the omission. Undo re-applies existing memory only to reconcile a
  returned sibling; it never creates memory.
- Keep `maintenance.rs` under 800 lines — it is at 796. The logic lives in
  package 1's module; this package only calls it.

Tests: `nr_32_missing_file_writes_no_memory` covers a whole missing album,
`nr_32_undone_removal_writes_no_memory` covers tombstone undo, and one test
per deliberate path proves memory is written on completion.
`nr_32_auto_clean_writes_no_memory` pins the other shared id-only cleanup path.

## Package 3 — the catalog sync applies the memory (TDD, core)

The refresh pipeline calls `apply_deleted_release_memory` once after every
artist's upsert batch, inside one transaction. This catches a release the
catalog had not fetched when deletion happened without repeating full library
and catalog scans once per artist.

Tests: `nr_32_memory_applies_to_a_release_fetched_later`,
`nr_32_badge_and_popover_follow_the_memory` (NR-26/NR-29 coherence: badge
count and `delta_candidates` both drop the hidden row).

## Package 4 — the column is named Release (GTK)

Independent of packages 1–3; only `docs/ux-rules.md` is shared, and that file
belongs to package 5 alone.

- `RELEASES_TITLE` becomes `N_!("Release")` in `strings_releases.rs`.
- Regenerate `po/reprise.pot`; translate the string as `Release` in all seven
  catalogs and leave no fuzzy entry.
- Display test `nr_33_column_contract` pins the header row
  `Cover · Date · Release · Artist · Type · Status · Link`.

## Package 5 — rules (`docs/ux-rules.md`, section R)

Mark **NR-31** `[replaced by NR-33]` (text otherwise untouched) and append,
both `[active]`:

- **NR-32** `[core]` `[gtk]` — A release the listener deliberately deleted
  does not return as a gap. When "move to trash" or "remove from library"
  finally removes the last track of an album, or the song a single is matched
  by, that release is remembered and hidden — including when the catalog only
  learns of it later. Deleting one song while keeping its album leaves the
  album row visible but hides a same-titled `single`. Files that merely go
  missing never trigger this. "Show again" restores every row hidden by the
  selected memory scope; re-acquisition forgets only the scope acquired.
- **NR-33** `[gtk]` — replaces NR-31. The gap view's columns are `Cover ·
  Date · Release · Artist · Type · Status · Link`; the second text column is
  named `Release` because its rows are albums, EPs and singles, not songs.
  Sorting, filters, counts, activation semantics, the trailing action column
  and zero-result recovery remain exactly as NR-31 specified.

Keep `docs/plans/ux-rules-acceptance-tests.md` in sync if it names NR-31.

## Verification

- `cargo test -p reprise-core artist_news`, then `cargo test -p reprise-core`.
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt`.
- `scripts/check-architecture.sh`, `scripts/check-ux-traceability.sh`.
- GTK display tests need an X server and will not run in this sandbox. Do not
  fake them, do not delete them, do not report them as passing — state plainly
  which suites ran and which could not.
- Check a suite's **pass count**, not just its exit status: a filter that
  matches nothing still prints `ok`.

## Out of scope

- No backfill of deletions that happened before this ships — `change_log`
  keeps no metadata for a deleted track, so it is not reconstructable. Say so
  in the summary; the user hides those by hand once.
- No toast when deleting, no separate "Deleted" chip, no preference.
- No change to the scanner, Missing/auto-clean eligibility, retention, or what
  the fetch requests. Tombstone undo only re-evaluates already-existing memory
  after a sibling returns; Missing and auto-clean never create it.
