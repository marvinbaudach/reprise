//! Testable action layer for the track list's context menu (Stage 3 Task 5).
//!
//! `track_list.rs` owns everything GTK-specific about the menu itself — the
//! `gtk::MultiSelection`, the secondary-click `GestureClick`, and the
//! `gtk::PopoverMenu`/`gio::SimpleAction` wiring — since that's inseparable
//! from the live `ColumnView` widget. This module holds the *logic* those
//! actions invoke: mapping selected row positions to track ids, deciding
//! what a "Play"/"Add to queue" click should do with them, and performing
//! the two playlist-mutating actions against the database. Every function
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
//! ## Remove from playlist: positions, not ids
//!
//! Manual playlists allow duplicates (the same track added twice — see
//! `library::playlists::add_tracks`'s doc comment). Removing by id would
//! delete *every* occurrence of a duplicated track instead of just the rows
//! the user selected, so [`remove_selected_from_playlist`] forwards the raw
//! ColumnView row positions straight to `library::playlists::
//! remove_positions`. This is exactly right whenever the playlist view is
//! showing its own default order (`track_list.rs`'s forced `"playlist_
//! order"` sort for `ViewSource::Playlist`, via `default_sort_for_source`)
//! with no search filter active — the common case, and the only one this
//! task's brief calls for. A column-header sort or an active search filter
//! while viewing a playlist would make the row's on-screen position diverge
//! from its `playlist_tracks.position`, which would remove the wrong row(s)
//! — a known, documented limitation (see the Task 5 report for the full
//! discussion) rather than something this task closes off, since fixing it
//! would require plumbing `playlist_tracks.position` through `queries.rs`/
//! `models::Track` for every source, well beyond this task's scope.

use std::cell::RefCell;
use std::rc::Rc;

use rusqlite::Connection;

use crate::library::playlists;
use crate::ui::track_list_model::TrackListModel;

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

/// Decides what the "Play" menu action should do with `ids` (selected track
/// ids, in selection order): `None` for an empty selection (nothing to
/// play — the menu item shouldn't have been reachable, but a defensive
/// no-op costs nothing), otherwise `Some((ids, 0))` — Rhythmbox's context-
/// menu-play semantics: start at the first selected row, with every other
/// selected row queued right after it. `ids` as given is already exactly the
/// shape `PlayerController::play_from_view`'s two parameters need, hence the
/// tuple — deliberately not "queue the whole view starting here", which is
/// `track_list.rs`'s row-activation seam (`queue_ids_for_activation`)
/// instead.
pub fn play_selected_ids(ids: &[i64]) -> Option<(Vec<i64>, usize)> {
    if ids.is_empty() {
        return None;
    }
    Some((ids.to_vec(), 0))
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
/// via `library::playlists::add_tracks`. Returns the number of rows
/// inserted (should always equal `ids.len()` — `add_tracks` has no partial-
/// failure mode) or the underlying `rusqlite::Error`; the caller (`track_
/// list.rs`) turns either outcome into a toast. A no-op (`Ok(0)`, no
/// connection borrow taken) for an empty `ids` slice.
pub fn add_selected_to_playlist(
    conn: &Rc<RefCell<Connection>>,
    playlist_id: i64,
    ids: &[i64],
) -> Result<u32, rusqlite::Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    let mut conn = conn.borrow_mut();
    playlists::add_tracks(&mut conn, playlist_id, ids)
}

/// "Remove from playlist" menu action — see the module doc's `## Remove from
/// playlist` section for why this takes raw row *positions*, not ids.
/// Returns the number of rows removed, or the underlying error. A no-op
/// (`Ok(0)`) for an empty `positions` slice.
pub fn remove_selected_from_playlist(
    conn: &Rc<RefCell<Connection>>,
    playlist_id: i64,
    positions: &[u32],
) -> Result<u32, rusqlite::Error> {
    if positions.is_empty() {
        return Ok(0);
    }
    let mut conn = conn.borrow_mut();
    playlists::remove_positions(&mut conn, playlist_id, positions)
}

/// "Add to playlist -> New playlist…" menu action: creates a playlist named
/// `name` and immediately appends `ids` to it, in one connection borrow.
/// Returns `(new_playlist_id, inserted_count)` on success. A creation
/// failure short-circuits before any `add_tracks` call is attempted (`?`
/// propagates `playlists::create`'s error); `add_tracks` is only skipped
/// (returning `Ok((id, 0))`) if `ids` is empty — a playlist can legitimately
/// be created via this path with nothing selected, though `track_list.rs`'s
/// menu only offers this action when the selection is non-empty.
pub fn create_playlist_and_add(
    conn: &Rc<RefCell<Connection>>,
    name: &str,
    ids: &[i64],
) -> Result<(i64, u32), rusqlite::Error> {
    let mut conn = conn.borrow_mut();
    let playlist_id = playlists::create(&conn, name)?;
    if ids.is_empty() {
        return Ok((playlist_id, 0));
    }
    let inserted = playlists::add_tracks(&mut conn, playlist_id, ids)?;
    Ok((playlist_id, inserted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    /// Seeds a fresh in-memory DB with `count` tracks (ids 1..=count) and a
    /// `TrackListModel` queried over the whole library in title order —
    /// mirrors `track_list_model.rs`'s own `seeded_model` helper, but lives
    /// here too since that helper is private to that module.
    fn seeded_model(count: i64) -> TrackListModel {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for id in 1..=count {
            conn.execute(
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
        let model = TrackListModel::new(Rc::new(RefCell::new(conn)));
        model.set_query(
            &crate::view_source::ViewSource::Library,
            "title",
            "asc",
            "",
            &[],
        );
        model
    }

    fn seeded_conn_with_tracks(count: i64) -> Rc<RefCell<Connection>> {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for id in 1..=count {
            conn.execute(
                "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (?1, ?2, ?3, ?4, 0)",
                params![id, format!("/x/{id}.flac"), format!("Track {id}"), "Artist"],
            )
            .unwrap();
        }
        Rc::new(RefCell::new(conn))
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
    fn play_selected_ids_empty_is_none() {
        assert_eq!(play_selected_ids(&[]), None);
    }

    #[test]
    fn play_selected_ids_starts_at_first_selected() {
        assert_eq!(play_selected_ids(&[42, 7, 9]), Some((vec![42, 7, 9], 0)));
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
        let playlist_id = playlists::create(&conn.borrow(), "P1").unwrap();

        let inserted = add_selected_to_playlist(&conn, playlist_id, &[1, 3, 5]).unwrap();
        assert_eq!(inserted, 3);

        let track_ids: Vec<i64> = conn
            .borrow()
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
        let playlist_id = playlists::create(&conn.borrow(), "P1").unwrap();
        let inserted = add_selected_to_playlist(&conn, playlist_id, &[]).unwrap();
        assert_eq!(inserted, 0);
    }

    /// The duplicate-safety case `remove_selected_from_playlist` exists
    /// for: the same track id appears twice in the playlist (rows at
    /// positions 0 and 2); removing "by position 2" must remove only that
    /// occurrence, leaving the other instance (position 0) intact — using
    /// ids instead of positions could not express this at all (an id-based
    /// remove would have to delete both, or neither).
    #[test]
    fn remove_selected_from_playlist_uses_positions_not_ids_for_duplicates() {
        let conn = seeded_conn_with_tracks(5);
        let playlist_id = playlists::create(&conn.borrow(), "P1").unwrap();
        {
            let mut conn_mut = conn.borrow_mut();
            // Track id 1 appears at both position 0 and position 2.
            playlists::add_tracks(&mut conn_mut, playlist_id, &[1, 2, 1, 3]).unwrap();
        }

        let removed = remove_selected_from_playlist(&conn, playlist_id, &[2]).unwrap();
        assert_eq!(removed, 1);

        let track_ids: Vec<i64> = conn
            .borrow()
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

    #[test]
    fn remove_selected_from_playlist_empty_positions_is_a_no_op() {
        let conn = seeded_conn_with_tracks(3);
        let playlist_id = playlists::create(&conn.borrow(), "P1").unwrap();
        {
            let mut conn_mut = conn.borrow_mut();
            playlists::add_tracks(&mut conn_mut, playlist_id, &[1, 2, 3]).unwrap();
        }
        let removed = remove_selected_from_playlist(&conn, playlist_id, &[]).unwrap();
        assert_eq!(removed, 0);
    }

    #[test]
    fn create_playlist_and_add_creates_and_inserts_in_one_call() {
        let conn = seeded_conn_with_tracks(4);
        let (playlist_id, inserted) =
            create_playlist_and_add(&conn, "New Playlist", &[2, 4]).unwrap();
        assert!(playlist_id > 0);
        assert_eq!(inserted, 2);

        let playlists = playlists::list(&conn.borrow()).unwrap();
        let created = playlists.iter().find(|p| p.id == playlist_id).unwrap();
        assert_eq!(created.name, "New Playlist");
        assert_eq!(created.track_count, 2);
    }

    #[test]
    fn create_playlist_and_add_with_no_ids_still_creates_the_playlist() {
        let conn = seeded_conn_with_tracks(1);
        let (playlist_id, inserted) = create_playlist_and_add(&conn, "Empty", &[]).unwrap();
        assert!(playlist_id > 0);
        assert_eq!(inserted, 0);
    }
}
