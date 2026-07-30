//! Testable action layer for the track list's context menu (Stage 3 Task 5).
//!
//! `track_list.rs` owns everything GTK-specific about the menu itself — the
//! `gtk::MultiSelection`, the secondary-click `GestureClick`, and the
//! `gtk::PopoverMenu`/`gio::SimpleAction` wiring — since that's inseparable
//! from the live `ColumnView` widget. This module holds the *logic* those
//! actions invoke: mapping selected row positions to track ids, deciding
//! what an "Add to queue" click should do with them, and performing the
//! playlist-mutating actions against the database. Every function
//! here is callable (and tested) without a running GTK display.
//!
//! ## Position → id seam
//!
//! [`selected_track_ids`] is the one place row positions (`u32`, what
//! `gtk::MultiSelection`/`ColumnView` deal in) become track ids (`i64`, what
//! the database/queue/playlist layers deal in) — via `TrackListModel::
//! track_at`, the exact same lookup row activation already uses
//! (`track_list.rs`'s `wire_activate`). `TrackListModel` is a plain
//! `glib::Object` subclass (not a widget), so this is testable directly
//! against a real instance with no display needed — see that module's own
//! test module for the same pattern.
//!
//! ## Remove from playlist: true `pt.position`, not view row index (Fix
//! Round 1)
//!
//! Manual playlists allow duplicates (the same track added twice — see
//! `library::playlists::add_tracks`'s doc comment). Removing by id would
//! delete *every* occurrence of a duplicated track instead of just the rows
//! the user selected, so [`remove_selected_from_playlist`] must resolve
//! *positions*, not ids — but the positions it resolves to are each
//! selected row's true `playlist_tracks.position`, never the raw ColumnView
//! row index passed in.
//!
//! An earlier version of this function forwarded the raw view row index
//! straight to `library::playlists::remove_positions`, which only happened
//! to be correct when the playlist view showed its own default order
//! (`track_list.rs`'s forced `"playlist_order"` sort) with no search filter
//! active. The moment a column-header sort or a live search filter made the
//! on-screen row order diverge from `pt.position` — both shipped,
//! reachable features — this deleted the wrong row(s): silent data loss,
//! reported as success ("N tracks removed"). See the Task 5 report's "Fix
//! Round 1" section for the full incident.
//!
//! The fix: every `Track` a `Playlist` source query returns now carries its
//! true `pt.position` in `models::Track::playlist_position` (`queries.rs`'s
//! `row_to_playlist_track`, populated regardless of the query's `ORDER BY`
//! or filter). `remove_selected_from_playlist` resolves each selected
//! *view* position to that field via `TrackListModel::track_at` before
//! calling `remove_positions` — so it's correct under any sort, any filter,
//! with duplicates, always. If any selected row's `playlist_position`
//! cannot be resolved (`track_at` returns `None`, or the row isn't a
//! playlist row at all — both should be unreachable in practice, since this
//! is only ever called while viewing a real `ViewSource::Playlist`), the
//! *entire* remove is aborted with [`RemoveFromPlaylistError::Unresolvable`]
//! and nothing is deleted — a remove is all-correct-or-nothing, never a
//! best-effort guess.

use std::rc::Rc;

use reprise_core::db::Db;

use crate::ui::track_list_model::TrackListModel;
use reprise_core::library::{playlist_membership, playlists};

/// Maps selected row positions to track ids via `TrackListModel::track_at`,
/// in the order `positions` was given (selection order, not id order).
/// Skips any position that no longer resolves to a row (e.g. the underlying
/// query changed between selection and action) — logged, not treated as a
/// hard error, matching every other fallible `track_at` call site in
/// `track_list.rs`.
pub fn selected_track_ids(positions: &[u32], model: &TrackListModel) -> Vec<i64> {
    positions
        .iter()
        .filter_map(|&position| {
            let track = model.track_at(position);
            if track.is_none() {
                tracing::warn!(
                    position,
                    "context menu: no track at selected position; skipping"
                );
            }
            track
        })
        .map(|track| track.id)
        .collect()
}

/// Decides what the "Add to queue" menu action should do with `ids`: `None`
/// for an empty selection (nothing to add), otherwise `Some(ids)` — the
/// exact argument `PlayerController::append_to_queue` expects.
pub fn queue_selected_ids(ids: &[i64]) -> Option<Vec<i64>> {
    if ids.is_empty() {
        return None;
    }
    Some(ids.to_vec())
}

/// "Add to playlist" menu action: appends `ids` to the end of `playlist_id`
/// via `library::playlist_membership::add_unique_tracks`. Returns the number
/// of newly inserted rows; tracks already present are skipped. The caller (`track_
/// list.rs`) turns either outcome into a toast. A no-op (`Ok(0)`, no
/// connection borrow taken) for an empty `ids` slice.
pub fn add_selected_to_playlist(
    conn: &Rc<Db>,
    playlist_id: i64,
    ids: &[i64],
) -> Result<u32, rusqlite::Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    let conn = &conn;
    playlist_membership::add_unique_tracks(conn, playlist_id, ids)
}

/// Error from [`remove_selected_from_playlist`] — see the module doc's
/// `## Remove from playlist` section.
#[derive(Debug, thiserror::Error)]
pub enum RemoveFromPlaylistError {
    /// The underlying `library::playlists::remove_positions` call failed.
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    /// Safety backstop: at least one selected view row's true `playlist_
    /// tracks.position` could not be resolved via `model`. Nothing was
    /// removed — a remove is all-correct-or-nothing, never a best-effort
    /// guess at which rows the caller meant.
    #[error("could not resolve a selected row's true playlist position")]
    Unresolvable,
}

/// "Remove from playlist" menu action — see the module doc's `## Remove from
/// playlist` section for why `positions` (raw ColumnView view-row indices,
/// *not* ids) are resolved through `model` to each row's true `playlist_
/// tracks.position` before reaching `library::playlists::remove_positions`,
/// rather than being forwarded as-is. Returns the number of rows removed on
/// success. A no-op (`Ok(0)`, no connection borrow, no model lookups) for an
/// empty `positions` slice.
pub fn remove_selected_from_playlist(
    conn: &Rc<Db>,
    playlist_id: i64,
    positions: &[u32],
    model: &TrackListModel,
) -> Result<u32, RemoveFromPlaylistError> {
    if positions.is_empty() {
        return Ok(0);
    }

    let mut true_positions = Vec::with_capacity(positions.len());
    for &view_position in positions {
        let resolved = model
            .track_at(view_position)
            .and_then(|track| track.playlist_position)
            .and_then(|position| u32::try_from(position).ok());
        let Some(true_position) = resolved else {
            tracing::warn!(
                view_position,
                playlist_id,
                "remove-from-playlist: could not resolve the true playlist position for a \
                 selected row; aborting the whole remove rather than guessing"
            );
            return Err(RemoveFromPlaylistError::Unresolvable);
        };
        true_positions.push(true_position);
    }

    let conn = &conn;
    Ok(playlists::remove_positions(
        conn,
        playlist_id,
        &true_positions,
    )?)
}

/// "Add to playlist -> New playlist…" menu action: creates a playlist named
/// `name` and appends `ids` to it, in one transaction — via `library::
/// playlists::create_with_tracks`, the same transactional primitive
/// `ui::playlist_io`'s M3U import already uses. Returns `(new_playlist_id,
/// inserted_count)` on success.
///
/// Task 9 review fold-in: this used to call `playlists::create` and `add_
/// tracks` as two separate statements — if `add_tracks` failed partway
/// (e.g. an id that no longer exists, tripping the `playlist_tracks.
/// track_id` foreign key), the `create` had already committed, leaving an
/// orphaned empty playlist behind with no rows and no way for the caller to
/// clean it up (the error return gives it no id). `create_with_tracks` wraps
/// both steps in one transaction, so a failure at either step rolls back the
/// whole thing — no orphan, ever. `ids.is_empty()` is still a legitimate
/// success case (`create_with_tracks` itself treats an empty slice as
/// "create only," matching this function's prior behavior exactly), though
/// `track_list.rs`'s menu only offers this action when the selection is
/// non-empty.
pub fn create_playlist_and_add(
    conn: &Rc<Db>,
    name: &str,
    ids: &[i64],
) -> Result<(i64, u32), rusqlite::Error> {
    let conn = &conn;
    let playlist_id = playlists::create_with_tracks(conn, name, ids)?;
    Ok((playlist_id, ids.len() as u32))
}

/// "Remove from library" menu action (Stage 3 Task 8, reachable only while
/// viewing `ViewSource::Missing`): deletes each of `ids` via `queries::
/// remove_missing_tracks` — the batch, TRANSACTIONAL primitive (Stage-3
/// close-out; see that function's doc comment for the DATABASE-ONLY delete
/// guarantee, its defensive presence-predicate guard, and why the batch/
/// transactional form is required: it also renumbers every playlist position gap the delete's
/// `ON DELETE CASCADE` would otherwise leave behind). Returns the ids
/// actually deleted (a subset of `ids`, in input order) — the caller
/// (`ui::track_list_context_menu::handle_remove_from_library`) needs the
/// exact ids, not just a count, to purge the same set from the playback
/// queue via `ui::player_controller::PlayerController::purge_queue_ids`. A
/// track that somehow wasn't/isn't-anymore missing is silently skipped, not
/// an error. A no-op (`Ok(vec![])`, with no database operation) for an empty
/// `ids` slice. Core owns the transaction needed by `remove_missing_tracks`.
pub fn remove_missing_selected(conn: &Rc<Db>, ids: &[i64]) -> Result<Vec<i64>, rusqlite::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let conn = &conn;
    reprise_core::queries::remove_missing_tracks(conn, ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::prelude::ListModelExt;
    use rusqlite::params;

    /// Seeds a fresh in-memory DB with `count` tracks (ids 1..=count) and a
    /// `TrackListModel` queried over the whole library in title order —
    /// mirrors `track_list_model.rs`'s own `seeded_model` helper, but lives
    /// here too since that helper is private to that module.
    fn seeded_model(count: i64) -> TrackListModel {
        let db = crate::test_db::open().unwrap();
        for id in 1..=count {
            crate::test_db::connection(&db)
                .execute(
                    "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (?1, ?2, ?3, ?4, 0)",
                    params![
                        id,
                        format!("/x/{id}.flac"),
                        format!("Track {id:03}"),
                        "Artist"
                    ],
                )
                .unwrap();
        }
        let model = TrackListModel::new(Rc::new(db));
        model.set_query(
            &reprise_core::view_source::ViewSource::Library,
            "title",
            "asc",
            "",
            &[],
        );
        model
    }

    fn seeded_conn_with_tracks(count: i64) -> Rc<Db> {
        let db = crate::test_db::open().unwrap();
        for id in 1..=count {
            crate::test_db::connection(&db)
                .execute(
                    "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (?1, ?2, ?3, ?4, 0)",
                    params![id, format!("/x/{id}.flac"), format!("Track {id}"), "Artist"],
                )
                .unwrap();
        }
        Rc::new(db)
    }

    #[test]
    fn selected_track_ids_maps_positions_in_given_order() {
        let model = seeded_model(5);
        // Title order is Track 001..005, ids 1..5 in the same order.
        let ids = selected_track_ids(&[3, 0, 4], &model);
        assert_eq!(ids, vec![4, 1, 5]);
    }

    #[test]
    fn selected_track_ids_skips_out_of_range_positions() {
        let model = seeded_model(3);
        let ids = selected_track_ids(&[0, 99, 2], &model);
        assert_eq!(ids, vec![1, 3]);
    }

    #[test]
    fn selected_track_ids_empty_selection_yields_empty_ids() {
        let model = seeded_model(3);
        assert!(selected_track_ids(&[], &model).is_empty());
    }

    #[test]
    fn queue_selected_ids_empty_is_none() {
        assert_eq!(queue_selected_ids(&[]), None);
    }

    #[test]
    fn queue_selected_ids_passes_ids_through() {
        assert_eq!(queue_selected_ids(&[1, 2, 3]), Some(vec![1, 2, 3]));
    }

    #[test]
    fn add_selected_to_playlist_inserts_and_counts() {
        let conn = seeded_conn_with_tracks(5);
        let playlist_id = playlists::create(&conn, "P1").unwrap();

        let inserted = add_selected_to_playlist(&conn, playlist_id, &[1, 3, 5]).unwrap();
        assert_eq!(inserted, 3);

        let track_ids: Vec<i64> = crate::test_db::connection(&conn)
            .prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![playlist_id], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(track_ids, vec![1, 3, 5]);
    }

    #[test]
    fn add_selected_to_playlist_empty_ids_is_a_no_op() {
        let conn = seeded_conn_with_tracks(2);
        let playlist_id = playlists::create(&conn, "P1").unwrap();
        let inserted = add_selected_to_playlist(&conn, playlist_id, &[]).unwrap();
        assert_eq!(inserted, 0);
    }

    #[test]
    fn add_selected_to_playlist_does_not_duplicate_existing_tracks() {
        let conn = seeded_conn_with_tracks(3);
        let playlist_id = playlists::create(&conn, "P1").unwrap();
        assert_eq!(
            add_selected_to_playlist(&conn, playlist_id, &[1, 2]).unwrap(),
            2
        );
        assert_eq!(
            add_selected_to_playlist(&conn, playlist_id, &[2, 3, 3]).unwrap(),
            1
        );
        let count: i64 = crate::test_db::connection(&conn)
            .query_row(
                "SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id=?1",
                [playlist_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 3);
    }

    /// The duplicate-safety case `remove_selected_from_playlist` exists
    /// for: the same track id appears twice in the playlist (rows at
    /// positions 0 and 2); removing "by position 2" must remove only that
    /// occurrence, leaving the other instance (position 0) intact — using
    /// ids instead of positions could not express this at all (an id-based
    /// remove would have to delete both, or neither). The view here is the
    /// playlist's own default order (`"playlist_order"`, no filter), so view
    /// row 2 and true `pt.position` 2 coincide — this pins that the
    /// resolve-through-the-model step is a no-op (identity) in the common
    /// case, not just in the sorted/filtered cases covered below.
    #[test]
    fn remove_selected_from_playlist_uses_positions_not_ids_for_duplicates() {
        let conn = seeded_conn_with_tracks(5);
        let playlist_id = playlists::create(&conn, "P1").unwrap();
        {
            let conn_mut = &conn;
            // Track id 1 appears at both position 0 and position 2.
            playlists::add_tracks(conn_mut, playlist_id, &[1, 2, 1, 3]).unwrap();
        }
        let model = TrackListModel::new(conn.clone());
        model.set_query(
            &reprise_core::view_source::ViewSource::Playlist(playlist_id),
            "playlist_order",
            "asc",
            "",
            &[],
        );

        let removed = remove_selected_from_playlist(&conn, playlist_id, &[2], &model).unwrap();
        assert_eq!(removed, 1);

        let track_ids: Vec<i64> = crate::test_db::connection(&conn)
            .prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![playlist_id], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        // The first occurrence of id 1 (position 0) survives; only the
        // second occurrence (position 2) was removed.
        assert_eq!(track_ids, vec![1, 2, 3]);
    }

    /// Seeds a fresh in-memory DB, creates playlist `"P1"`, and appends five
    /// tracks (titles A..E, ids 1..5, appended in that order so `pt.position`
    /// == id - 1) with artists chosen so an artist-ascending sort produces a
    /// *different* row order than `pt.position` — the exact divergence
    /// `remove_selected_from_playlist` must handle correctly (see the module
    /// doc's `## Remove from playlist` section). Artist order (ascending,
    /// `COLLATE NOCASE`) is Alpha < Beta < Delta < Epsilon < Zeta, i.e.
    /// track C < E < B < D < A — matching this task's bug report's own
    /// repro (`[A,B,C,D,E]` -> `[C,A,E,B,D]` on an Artist header click:
    /// track C lands first either way). Returns `(conn, playlist_id)`.
    fn seeded_playlist_with_divergent_artist_order() -> (Rc<Db>, i64) {
        let db = crate::test_db::open().unwrap();
        let tracks = [
            (1, "A", "Zeta"),
            (2, "B", "Delta"),
            (3, "C", "Alpha"),
            (4, "D", "Epsilon"),
            (5, "E", "Beta"),
        ];
        for (id, title, artist) in tracks {
            crate::test_db::connection(&db)
                .execute(
                    "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (?1, ?2, ?3, ?4, 0)",
                    params![id, format!("/x/{id}.flac"), title, artist],
                )
                .unwrap();
        }
        let playlist_id = playlists::create(&db, "P1").unwrap();
        playlists::add_tracks(&db, playlist_id, &[1, 2, 3, 4, 5]).unwrap();
        (Rc::new(db), playlist_id)
    }

    /// Current surviving track ids for `playlist_id`, in `pt.position` order.
    fn playlist_track_ids_in_position_order(conn: &Rc<Db>, playlist_id: i64) -> Vec<i64> {
        crate::test_db::connection(conn)
            .prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![playlist_id], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// Regression for the Task 5 Fix Round 1 data-loss bug: the playlist is
    /// `[A,B,C,D,E]` at `pt.position` 0..4; the user has clicked the Artist
    /// column header, so the view shows track C (id 3, true `pt.position`
    /// 2) first. Selecting the first *visible* row and invoking "Remove
    /// from playlist" must remove track C — not whatever happens to sit at
    /// `pt.position` 0 (track A, id 1). This is exercised end-to-end through
    /// a real `TrackListModel` queried in artist-ascending order (never
    /// assuming which id lands at view row 0 — that's read back from the
    /// model itself, exactly like `track_list_context_menu.rs`'s real
    /// call path), so the test fails if the durable fix's data flow (`Track::
    /// playlist_position` -> `remove_selected_from_playlist`'s model lookup)
    /// is missing or wrong in either direction.
    #[test]
    fn remove_selected_from_playlist_removes_the_visible_row_not_position_zero() {
        let (conn, playlist_id) = seeded_playlist_with_divergent_artist_order();

        let model = TrackListModel::new(conn.clone());
        model.set_query(
            &reprise_core::view_source::ViewSource::Playlist(playlist_id),
            "artist",
            "asc",
            "",
            &[],
        );
        // Read back which track is actually visible at view row 0, rather
        // than hardcoding the expected id — this is exactly what a real
        // right-click-first-row selection resolves to.
        let visible_row_zero_id = model.track_at(0).unwrap().id;
        assert_eq!(
            visible_row_zero_id, 3,
            "sanity check: artist order should put track C (id 3) first"
        );

        let removed = remove_selected_from_playlist(&conn, playlist_id, &[0], &model).unwrap();
        assert_eq!(removed, 1);

        let remaining = playlist_track_ids_in_position_order(&conn, playlist_id);
        assert!(
            !remaining.contains(&visible_row_zero_id),
            "the row visibly selected under the artist sort (track C, id 3) must be removed, \
             not whatever track happens to sit at pt.position 0"
        );
        assert_eq!(
            remaining,
            vec![1, 2, 4, 5],
            "the other four tracks must survive, renumbered contiguously"
        );
    }

    /// Non-contiguous multi-row remove under a sort, mirroring the Fix
    /// Round 1 headless E2E run: artist-ascending view order is C (true
    /// position 2), E (position 4), B (position 1), D (position 3), A
    /// (position 0). Selecting view rows 0 and 2 (C and B) selects true
    /// positions `{2, 1}` — non-contiguous *and* out of ascending order —
    /// which must still all be removed correctly and the remainder
    /// renumbered gaplessly (`library::playlists::remove_positions`'s own
    /// invariant, exercised here through the model-resolution layer on top
    /// of it rather than assumed).
    #[test]
    fn remove_selected_from_playlist_removes_non_contiguous_true_positions_when_sorted() {
        let (conn, playlist_id) = seeded_playlist_with_divergent_artist_order();

        let model = TrackListModel::new(conn.clone());
        model.set_query(
            &reprise_core::view_source::ViewSource::Playlist(playlist_id),
            "artist",
            "asc",
            "",
            &[],
        );
        // View order (artist ascending): C, E, B, D, A.
        let view_row_0_id = model.track_at(0).unwrap().id;
        let view_row_2_id = model.track_at(2).unwrap().id;
        assert_eq!(
            (view_row_0_id, view_row_2_id),
            (3, 2),
            "sanity check: C then B"
        );

        let removed = remove_selected_from_playlist(&conn, playlist_id, &[0, 2], &model).unwrap();
        assert_eq!(removed, 2);

        let remaining = playlist_track_ids_in_position_order(&conn, playlist_id);
        assert_eq!(
            remaining,
            vec![1, 4, 5],
            "tracks A, D, E survive (their original relative pt.position order), \
             renumbered 0..2; C and B are gone"
        );
    }

    /// Same divergence, driven by a live search filter instead of a column
    /// sort: filtering to "Delta" (track B's artist, and nothing else's
    /// title/artist/album/genre) drops every other track from the view
    /// entirely, so the filtered view's row 0 (track B, true position
    /// `pt.position == 1`) no longer lines up with the view's row 0 index
    /// either. Selecting it and removing must still remove track B
    /// specifically.
    #[test]
    fn remove_selected_from_playlist_removes_the_visible_row_under_a_live_filter() {
        let (conn, playlist_id) = seeded_playlist_with_divergent_artist_order();

        let model = TrackListModel::new(conn.clone());
        // Default playlist order, but filtered down to track B alone —
        // still in pt.position order, so the filtered view's row 0 is
        // track B (pt.position 1).
        model.set_query(
            &reprise_core::view_source::ViewSource::Playlist(playlist_id),
            "playlist_order",
            "asc",
            "Delta",
            &[],
        );
        assert_eq!(model.n_items(), 1, "filter should isolate track B alone");
        let visible_row_zero_id = model.track_at(0).unwrap().id;
        assert_eq!(visible_row_zero_id, 2, "track B is id 2");

        let removed = remove_selected_from_playlist(&conn, playlist_id, &[0], &model).unwrap();
        assert_eq!(removed, 1);

        let remaining = playlist_track_ids_in_position_order(&conn, playlist_id);
        assert_eq!(
            remaining,
            vec![1, 3, 4, 5],
            "only track B (id 2, true pt.position 1) is removed; the rest survive, renumbered"
        );
    }

    /// Safety backstop: if a selected view position cannot be resolved to a
    /// true playlist position (e.g. it's out of range for the model's
    /// current query), the whole remove must abort with nothing deleted —
    /// never guess. See the module doc's `## Remove from playlist` section.
    #[test]
    fn remove_selected_from_playlist_aborts_entirely_on_an_unresolvable_position() {
        let (conn, playlist_id) = seeded_playlist_with_divergent_artist_order();
        let model = TrackListModel::new(conn.clone());
        model.set_query(
            &reprise_core::view_source::ViewSource::Playlist(playlist_id),
            "playlist_order",
            "asc",
            "",
            &[],
        );

        // Position 1 resolves fine; position 99 is out of range and cannot
        // be resolved. The whole batch must be rejected — not a partial
        // remove of just position 1.
        let result = remove_selected_from_playlist(&conn, playlist_id, &[1, 99], &model);
        assert!(matches!(result, Err(RemoveFromPlaylistError::Unresolvable)));

        let remaining = playlist_track_ids_in_position_order(&conn, playlist_id);
        assert_eq!(
            remaining,
            vec![1, 2, 3, 4, 5],
            "nothing must be removed when any selected row is unresolvable"
        );
    }

    #[test]
    fn remove_selected_from_playlist_empty_positions_is_a_no_op() {
        let conn = seeded_conn_with_tracks(3);
        let playlist_id = playlists::create(&conn, "P1").unwrap();
        {
            let conn_mut = &conn;
            playlists::add_tracks(conn_mut, playlist_id, &[1, 2, 3]).unwrap();
        }
        // An empty selection never touches the model — a fresh, unqueried
        // one is fine here.
        let model = TrackListModel::new(conn.clone());
        let removed = remove_selected_from_playlist(&conn, playlist_id, &[], &model).unwrap();
        assert_eq!(removed, 0);
    }

    #[test]
    fn create_playlist_and_add_creates_and_inserts_in_one_call() {
        let conn = seeded_conn_with_tracks(4);
        let (playlist_id, inserted) =
            create_playlist_and_add(&conn, "New Playlist", &[2, 4]).unwrap();
        assert!(playlist_id > 0);
        assert_eq!(inserted, 2);

        let playlists = playlists::list(&conn).unwrap();
        let created = playlists.iter().find(|p| p.id == playlist_id).unwrap();
        assert_eq!(created.name, "New Playlist");
        assert_eq!(created.track_count, 2);
    }

    /// Task 9 review fold-in regression: mirrors `library::playlists`'s own
    /// `create_with_tracks_rolls_back_playlist_row_on_fk_violation` test, one
    /// layer up — proves the transactional adoption actually reaches this
    /// function's callers (the "New playlist…" context-menu action), not
    /// just the primitive it now calls. A `track_id` that doesn't exist
    /// (9999, never inserted by `seeded_conn_with_tracks`) trips the
    /// `playlist_tracks.track_id` foreign key partway through the insert
    /// loop; before this fix (separate `playlists::create` + `add_tracks`
    /// calls), the `create` half would have already committed, leaving an
    /// orphaned empty playlist row behind. With `create_with_tracks`, the
    /// whole thing rolls back — no playlist row survives at all.
    #[test]
    fn create_playlist_and_add_rolls_back_playlist_row_on_a_bad_track_id() {
        let conn = seeded_conn_with_tracks(3);
        let before = playlists::list(&conn).unwrap().len();

        let result = create_playlist_and_add(&conn, "Bad Playlist", &[1, 9999]);
        assert!(result.is_err());

        let after = playlists::list(&conn).unwrap().len();
        assert_eq!(before, after, "no playlist row should survive the rollback");
    }

    #[test]
    fn create_playlist_and_add_with_no_ids_still_creates_the_playlist() {
        let conn = seeded_conn_with_tracks(1);
        let (playlist_id, inserted) = create_playlist_and_add(&conn, "Empty", &[]).unwrap();
        assert!(playlist_id > 0);
        assert_eq!(inserted, 0);
    }

    fn mark_missing(conn: &Rc<Db>, id: i64) {
        crate::test_db::connection(conn)
            .execute(
                "UPDATE tracks SET missing_since = 1, missing_reason = 'unknown' WHERE id = ?1",
                params![id],
            )
            .unwrap();
    }

    #[test]
    fn remove_missing_selected_deletes_missing_rows() {
        let conn = seeded_conn_with_tracks(3);
        mark_missing(&conn, 1);
        mark_missing(&conn, 3);

        let mut removed = remove_missing_selected(&conn, &[1, 3]).unwrap();
        removed.sort_unstable();

        assert_eq!(removed, vec![1, 3]);
        let count: i64 = crate::test_db::connection(&conn)
            .query_row("SELECT count(*) FROM tracks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "only the untouched track (id 2) survives");
    }

    #[test]
    fn remove_missing_selected_skips_a_track_that_is_not_missing() {
        let conn = seeded_conn_with_tracks(2);
        mark_missing(&conn, 1);
        // id 2 is left alone (still present, missing_since NULL).

        let removed = remove_missing_selected(&conn, &[1, 2]).unwrap();

        assert_eq!(
            removed,
            vec![1],
            "only the actually-missing track is removed"
        );
        let count: i64 = crate::test_db::connection(&conn)
            .query_row("SELECT count(*) FROM tracks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn remove_missing_selected_empty_ids_is_a_no_op() {
        let conn = seeded_conn_with_tracks(2);
        let removed = remove_missing_selected(&conn, &[]).unwrap();
        assert!(removed.is_empty());
    }

    /// Stage-3 close-out regression: removing a missing track that's also in
    /// a playlist must leave that playlist's positions gapless — exercised
    /// here one layer above `queries::remove_missing_tracks`'s own direct
    /// tests, proving the transactional compaction reaches this real call
    /// path (mirrors `create_playlist_and_add_rolls_back_playlist_row_on_a_
    /// bad_track_id`'s "prove it reaches the caller" pattern above).
    #[test]
    fn remove_missing_selected_compacts_a_playlist_the_removed_track_belonged_to() {
        let conn = seeded_conn_with_tracks(5);
        let playlist_id = reprise_core::library::playlists::create(&conn, "P1").unwrap();
        {
            let conn_mut = &conn;
            reprise_core::library::playlists::add_tracks(conn_mut, playlist_id, &[1, 2, 3, 4, 5])
                .unwrap();
        }
        mark_missing(&conn, 3); // the middle track

        let removed = remove_missing_selected(&conn, &[3]).unwrap();
        assert_eq!(removed, vec![3]);

        let positions: Vec<i64> = crate::test_db::connection(&conn)
            .prepare(
                "SELECT position FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![playlist_id], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(positions, vec![0, 1, 2, 3], "positions must stay gapless");
    }
}
