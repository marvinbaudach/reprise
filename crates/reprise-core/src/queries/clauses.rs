//! Shared SQL fragment builders and row-mapping functions used by every
//! `ViewSource`'s query module: the sort whitelist, the LIKE-filter clause,
//! the parameterized library/missing query builders, and the `rusqlite::Row`
//! -> `Track`/`id` mappers. Split out of the former single-file `queries.rs`
//! (Refactoring & Extensibility Task 1) — a pure move, no behavior change.

use crate::library::playlists;
use crate::models::{MissingReason, Track};

use super::queue::QUEUE_LIMIT;
use super::{browse::browse_clause, BrowseFilter};

/// The one truth for "row is visible": file present (`missing_since IS
/// NULL`), not tombstoned (`removed_at IS NULL`) — Task 1.2's centralized
/// replacement for the legacy `missing = 0` literal that used to be
/// scattered across every window/count/id query in this module tree. A
/// boolean flag plus a date can drift out of sync (whoever updates one can
/// forget the other), and a planned auto-clean feature deletes rows based on
/// how long `missing_since` has been set — a row with an unclear start date
/// can never be safely auto-removable — so `missing_since` alone, not
/// `missing`, decides presence from this task onward (see `db::SCHEMA_V10`'s
/// doc comment for the full migration rationale). `removed_at` is folded in
/// here too, ahead of the tombstone feature (a later task) actually writing
/// it: every view becomes tombstone-aware today, so no query here needs
/// revisiting once removals start setting it for the 10-second undo window.
pub(crate) const PRESENT: &str = "missing_since IS NULL AND removed_at IS NULL";
/// The complement of [`PRESENT`]: awaiting relink or removal — file gone,
/// not (yet) tombstoned. Powers `ViewSource::Missing` and every hard-delete
/// guard that must only ever touch an already-missing row.
pub(crate) const MISSING: &str = "missing_since IS NOT NULL AND removed_at IS NULL";

/// `"playlist_order"` is a *sentinel* entry, not a real column: it only
/// resolves to valid SQL (`pt.position`) inside a query that actually joins
/// `playlist_tracks AS pt` — see the module doc's `Playlist(id)` section for
/// why that's safe (only `ViewSource::Playlist` queries ever pass it).
const SORT_WHITELIST: [(&str, &str); 11] = [
    ("title", "title COLLATE NOCASE"),
    (
        "artist",
        "artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no",
    ),
    ("album", "album COLLATE NOCASE, track_no"),
    ("track_no", "track_no"),
    ("genre", "genre COLLATE NOCASE, artist COLLATE NOCASE"),
    ("year", "year"),
    ("duration_ms", "duration_ms"),
    ("rating", "rating"),
    ("play_count", "play_count"),
    ("added_at", "added_at"),
    ("playlist_order", "pt.position"),
];

/// Shared LIKE-filter clause on `(title, artist, album, genre)`, parameterized
/// by the positional index of the bound `?N` placeholder: callers bind the
/// filter value at whatever placeholder index is free once their own
/// preceding parameters (limit/offset, a playlist id, smart-rules params,
/// …) are accounted for. Every source's window/count queries build their
/// WHERE clause through this one function so the filtered columns and LIKE
/// semantics can never drift apart between a count and the rows it
/// describes (DRY).
pub(super) fn filter_clause(has_filter: bool, param_index: u8) -> String {
    if has_filter {
        format!(
            " AND (title LIKE ?{param_index} ESCAPE '\\' OR artist LIKE ?{param_index} ESCAPE '\\' \
             OR album LIKE ?{param_index} ESCAPE '\\' OR genre LIKE ?{param_index} ESCAPE '\\')"
        )
    } else {
        String::new()
    }
}

/// The composable "hide AI-manipulated/-generated tracks" clause (plan 2.4/8,
/// Beschluss 17). Empty when the filter is off, so it drops cleanly into any
/// `tracks` query alongside [`PRESENT`], [`filter_clause`] and the browse
/// clause. It keys on the **`track_provenance` flag, never on a path** — the
/// dedicated folder is only layout, while the DB flag is the truth (files can
/// move; embedded tags carry provenance across rescans). Carries **no bound
/// parameter** (`ai = 1` is a literal), so appending it never shifts any
/// caller's `?N` numbering — the property that makes it freely composable.
/// The correlated `NOT EXISTS` references the outer `tracks.id`, so it only
/// belongs in a query whose flat source table is `tracks`.
pub(super) fn ai_exclude_clause(exclude_ai: bool) -> &'static str {
    if exclude_ai {
        " AND NOT EXISTS (SELECT 1 FROM track_provenance tp \
          WHERE tp.track_id = tracks.id AND tp.ai = 1)"
    } else {
        ""
    }
}

/// Builds the bound `%…%` LIKE pattern for a trimmed filter value — always
/// through `library::playlists::escape_like` (Stage-3 close-out finding:
/// this used to be a bare `format!("%{}%", filter.trim())` at every call
/// site below, so a literal `%`/`_` typed into the search box acted as a
/// live wildcard instead of matching itself, inconsistent with the smart-
/// rule `contains` operator's own escaping). Every `filter_clause` LIKE
/// site in this module builds its bound value through this one function so
/// the two can never drift apart again.
pub(super) fn like_pattern(filter_trimmed: &str) -> String {
    format!("%{}%", playlists::escape_like(filter_trimmed))
}

/// Resolves `sort_field`/`sort_dir` to a complete whitelisted `ORDER BY`
/// clause body. The direction is applied to every term of a compound sort,
/// so descending Artist reverses Artist itself as well as its stable
/// year/album/track-number tie-breakers.
///
/// `sort_field` is only ever used as a lookup key into `SORT_WHITELIST` —
/// never interpolated into SQL directly — so caller input cannot inject
/// arbitrary SQL. Unknown sort fields silently fall back to sorting by title.
pub(super) fn order_clause(sort_field: &str, sort_dir: &str) -> String {
    let order_expr = SORT_WHITELIST
        .iter()
        .find(|(k, _)| *k == sort_field)
        .map_or("title COLLATE NOCASE", |(_, v)| *v);
    let dir = if sort_dir.eq_ignore_ascii_case("desc") {
        "DESC"
    } else {
        "ASC"
    };
    order_expr
        .split(',')
        .map(|term| format!("{} {dir}", term.trim()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Resolves a `missing_flag` (`0` for the library view, `1` for the
/// missing-files view — a Rust-side literal, never caller input) to the
/// matching presence predicate. The one place `build_track_query_base`/
/// `build_track_ids_query_base` decide which half of `tracks` a `0`/`1`
/// flag means, so `PRESENT`/`MISSING` stay the single source of truth
/// for both.
fn presence_clause(missing_flag: u8) -> &'static str {
    if missing_flag == 0 {
        PRESENT
    } else {
        MISSING
    }
}

/// Builds the parameterized library/missing SELECT for a track window;
/// `missing_flag` is `0` for the library view, `1` for the missing-files
/// view — a Rust-side literal (`0`/`1`), never caller input, resolved to
/// `PRESENT`/`MISSING` via `presence_clause`. `sort_field` is only ever
/// used to look up an entry in `SORT_WHITELIST` — it is never interpolated
/// into the SQL string directly, so caller input cannot inject arbitrary
/// SQL. Unknown sort fields silently fall back to sorting by title.
pub(super) fn build_track_query_base(
    missing_flag: u8,
    sort_field: &str,
    sort_dir: &str,
    has_filter: bool,
) -> String {
    build_track_query_base_browsed(
        missing_flag,
        sort_field,
        sort_dir,
        has_filter,
        &BrowseFilter::default(),
        false,
    )
}

pub(super) fn build_track_query_base_browsed(
    missing_flag: u8,
    sort_field: &str,
    sort_dir: &str,
    has_filter: bool,
    browse: &BrowseFilter,
    exclude_ai: bool,
) -> String {
    let order = order_clause(sort_field, sort_dir);
    let filter_clause = filter_clause(has_filter, 3);
    let browse_first_param = if has_filter { 4 } else { 3 };
    let (browse_clause, _) = browse_clause(browse, browse_first_param);
    let ai_clause = ai_exclude_clause(exclude_ai);
    let presence = presence_clause(missing_flag);
    format!(
        "SELECT id, path, title, artist, album, album_artist, year, track_no, genre, \
         duration_ms, bitrate_kbps, rating, play_count, last_played_at, added_at, \
         file_mtime, missing_since, missing_reason, untagged, file_size, device, inode \
         FROM tracks WHERE {presence}{filter_clause}{browse_clause}{ai_clause} \
         ORDER BY {order} LIMIT ?1 OFFSET ?2"
    )
}

/// Builds the parameterized SELECT for a library window (`PRESENT`).
/// See `build_track_query_base`'s doc comment for the whitelist guarantee.
/// Its only callers are this module's tests.
pub fn build_track_query(sort_field: &str, sort_dir: &str, has_filter: bool) -> String {
    build_track_query_base(0, sort_field, sort_dir, has_filter)
}

pub(super) fn build_track_query_browsed(
    sort_field: &str,
    sort_dir: &str,
    has_filter: bool,
    browse: &BrowseFilter,
    exclude_ai: bool,
) -> String {
    build_track_query_base_browsed(0, sort_field, sort_dir, has_filter, browse, exclude_ai)
}

/// Builds the parameterized `SELECT id` for the queue seam
/// (`query_track_ids`, library/missing shape): every id matching
/// `(missing_flag, sort_field, sort_dir, filter)`, capped at `QUEUE_LIMIT` —
/// a literal, not a bound parameter, since it's a fixed Rust-side constant
/// rather than caller input (nothing to inject). Shares `order_clause`/
/// `filter_clause` with `build_track_query_base` so the queue's ordering can
/// never drift from the track list's.
pub(super) fn build_track_ids_query_base(
    missing_flag: u8,
    sort_field: &str,
    sort_dir: &str,
    has_filter: bool,
) -> String {
    let order = order_clause(sort_field, sort_dir);
    let filter_clause = filter_clause(has_filter, 1);
    let presence = presence_clause(missing_flag);
    format!(
        "SELECT id FROM tracks WHERE {presence}{filter_clause} \
         ORDER BY {order} LIMIT {QUEUE_LIMIT}"
    )
}

/// Builds the parameterized `SELECT id` for the library queue seam
/// (`PRESENT`). See `build_track_ids_query_base`'s doc comment.
pub fn build_track_ids_query(sort_field: &str, sort_dir: &str, has_filter: bool) -> String {
    build_track_ids_query_base(0, sort_field, sort_dir, has_filter)
}

pub(super) fn build_track_ids_query_browsed(
    sort_field: &str,
    sort_dir: &str,
    has_filter: bool,
    browse: &BrowseFilter,
    exclude_ai: bool,
) -> String {
    let order = order_clause(sort_field, sort_dir);
    let filter_clause = filter_clause(has_filter, 1);
    let browse_first_param = if has_filter { 2 } else { 1 };
    let (browse_clause, _) = browse_clause(browse, browse_first_param);
    let ai_clause = ai_exclude_clause(exclude_ai);
    format!(
        "SELECT id FROM tracks WHERE {PRESENT}{filter_clause}{browse_clause}{ai_clause} \
         ORDER BY {order} LIMIT {QUEUE_LIMIT}"
    )
}

pub(super) fn row_to_track(r: &rusqlite::Row) -> rusqlite::Result<Track> {
    Ok(Track {
        id: r.get(0)?,
        path: r.get(1)?,
        title: r.get(2)?,
        artist: r.get(3)?,
        album: r.get(4)?,
        album_artist: r.get(5)?,
        year: r.get(6)?,
        track_no: r.get(7)?,
        genre: r.get(8)?,
        duration_ms: r.get(9)?,
        bitrate_kbps: r.get(10)?,
        rating: r.get(11)?,
        play_count: r.get(12)?,
        last_played_at: r.get(13)?,
        added_at: r.get(14)?,
        file_mtime: r.get(15)?,
        missing_since: r.get(16)?,
        // `MissingReason::parse` never fails — an unrecognized/`NULL` value
        // just falls back within the `Option`'s `Some` arm (`NULL` itself
        // short-circuits to `None` via `Option::as_deref`'s `?`-propagated
        // `Option<String>`), matching the enum's own doc comment.
        missing_reason: r
            .get::<_, Option<String>>(17)?
            .as_deref()
            .map(MissingReason::parse),
        untagged: r.get::<_, i64>(18)? != 0,
        file_size: r.get(19)?,
        device: r.get(20)?,
        inode: r.get(21)?,
        playlist_position: None,
    })
}

/// Same 22-column shape as `row_to_track`, plus a trailing `pt.position`
/// column (index 22) — used only by `query_track_window_playlist`, the one
/// query that actually joins `playlist_tracks AS pt`. See `Track::
/// playlist_position`'s doc comment for why this is the sole populating
/// call site.
pub(super) fn row_to_playlist_track(r: &rusqlite::Row) -> rusqlite::Result<Track> {
    let mut track = row_to_track(r)?;
    // `row_to_track` consumes indices 0..=21; `pt.position` is the next
    // column the playlist SELECTs project.
    track.playlist_position = Some(r.get(22)?);
    Ok(track)
}

pub(super) fn row_to_id(r: &rusqlite::Row) -> rusqlite::Result<i64> {
    r.get(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_count_is_a_whitelisted_numeric_sort() {
        assert_eq!(order_clause("play_count", "desc"), "play_count DESC");
    }

    #[test]
    fn descending_compound_sort_reverses_every_order_term() {
        assert_eq!(
            order_clause("artist", "desc"),
            "artist COLLATE NOCASE DESC, year DESC, album COLLATE NOCASE DESC, track_no DESC"
        );
    }

    #[test]
    fn ai_exclude_clause_is_empty_when_off_and_parameter_free_when_on() {
        assert_eq!(ai_exclude_clause(false), "");
        let clause = ai_exclude_clause(true);
        assert!(clause.contains("NOT EXISTS"));
        assert!(clause.contains("track_provenance"));
        assert!(clause.contains("tp.ai = 1"));
        // No bound parameter, so it never shifts a caller's ?N numbering.
        assert!(!clause.contains('?'));
        // Keyed on the DB flag via the track id — never on a path.
        assert!(!clause.contains("path"));
    }

    #[test]
    fn browse_builder_appends_the_ai_clause_only_when_excluding() {
        let off = build_track_query_browsed("title", "asc", false, &BrowseFilter::default(), false);
        assert!(
            !off.contains("NOT EXISTS"),
            "no exclude clause when the filter is off"
        );
        let on = build_track_query_browsed("title", "asc", false, &BrowseFilter::default(), true);
        assert!(on.contains("NOT EXISTS"));
        // The exclude clause sits inside the WHERE, before ORDER BY.
        let where_start = on.find("WHERE").unwrap();
        assert!(on.find("NOT EXISTS").unwrap() > where_start);
        assert!(on.find("NOT EXISTS").unwrap() < on.find("ORDER BY").unwrap());
    }

    /// Seeds two present tracks — an original and an AI instrumental — plus a
    /// *missing* AI track, then proves the filter's semantics and its
    /// composition with `PRESENT`.
    #[test]
    fn ai_exclude_hides_ai_tracks_includes_originals_and_composes_with_present() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (id, path, title, artist, added_at, file_mtime, file_size) \
               VALUES (1, '/a.flac', 'Original', 'A', 1, 1, 1);
             INSERT INTO tracks (id, path, title, artist, added_at, file_mtime, file_size) \
               VALUES (2, '/b.flac', 'Instrumental', 'A', 1, 1, 1);
             INSERT INTO tracks (id, path, title, artist, added_at, file_mtime, file_size, missing_since) \
               VALUES (3, '/c.flac', 'Missing AI', 'A', 1, 1, 1, 100);
             INSERT INTO track_provenance (track_id, kind, ai, created_at) VALUES (2, 'vocals-removed', 1, 0);
             INSERT INTO track_provenance (track_id, kind, ai, created_at) VALUES (3, 'vocals-removed', 1, 0);",
        )
        .unwrap();

        let titles = |exclude_ai: bool| -> Vec<String> {
            let sql = build_track_query_browsed(
                "title",
                "asc",
                false,
                &BrowseFilter::default(),
                exclude_ai,
            );
            let mut stmt = conn.prepare(&sql).unwrap();
            stmt.query_map([1000i64, 0i64], |row| row.get::<_, String>(2))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };

        // Filter off: both present tracks show (the missing one is already
        // hidden by PRESENT).
        assert_eq!(titles(false), ["Instrumental", "Original"]);
        // Filter on: the present AI track is hidden, the original stays, and
        // the missing AI track remains hidden (PRESENT still holds).
        assert_eq!(titles(true), ["Original"]);
    }

}
