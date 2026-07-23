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
const SORT_WHITELIST: [(&str, &str); 10] = [
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

/// The `is_ai` projected column expression (INST-10). When `project_ai` is set
/// it is the correlated provenance `EXISTS` the AI badge reads; otherwise it is
/// a literal `0`, so the query plan carries **no** per-row subquery.
///
/// The badge only renders while the experimental switch is on (`ui::track_list`
/// gates it), so projecting the correlated `EXISTS` on every windowed track
/// query — 50k–500k rows, workspace-wide, once per filtered row before `LIMIT` —
/// was a measured 20–30% cost paid even when nothing reads the column. Callers
/// pass whether they need it (the GTK layer knows `experimental_on`); when they
/// do not, the literal `0` keeps the plan subquery-free. Either way the column
/// is projected at the same position, so `row_to_track`'s fixed index is
/// unaffected — an off-path row simply reads `is_ai = false`.
pub(super) fn ai_projection(project_ai: bool) -> &'static str {
    if project_ai {
        "EXISTS(SELECT 1 FROM track_provenance tp WHERE tp.track_id = tracks.id AND tp.ai = 1)"
    } else {
        "0"
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

/// Resolves `sort_field`/`sort_dir` to a whitelisted `ORDER BY` expression
/// and direction keyword. Shared by every source's window/ids query builder
/// so they can never disagree about what a given sort field/direction
/// means. `sort_field` is only ever used as a lookup key into `SORT_
/// WHITELIST` — never interpolated into SQL directly — so caller input
/// cannot inject arbitrary SQL. Unknown sort fields silently fall back to
/// sorting by title (this is also what makes a DB-tampered smart-playlist
/// `sort_field` degrade safely — see the module doc's `Smart(id)` section).
pub(super) fn order_expr_and_dir(sort_field: &str, sort_dir: &str) -> (&'static str, &'static str) {
    let order_expr = SORT_WHITELIST
        .iter()
        .find(|(k, _)| *k == sort_field)
        .map_or("title COLLATE NOCASE", |(_, v)| *v);
    let dir = if sort_dir.eq_ignore_ascii_case("desc") {
        "DESC"
    } else {
        "ASC"
    };
    (order_expr, dir)
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
    project_ai: bool,
) -> String {
    build_track_query_base_browsed(
        missing_flag,
        sort_field,
        sort_dir,
        has_filter,
        &BrowseFilter::default(),
        false,
        project_ai,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_track_query_base_browsed(
    missing_flag: u8,
    sort_field: &str,
    sort_dir: &str,
    has_filter: bool,
    browse: &BrowseFilter,
    exclude_ai: bool,
    project_ai: bool,
) -> String {
    let (order_expr, dir) = order_expr_and_dir(sort_field, sort_dir);
    let filter_clause = filter_clause(has_filter, 3);
    let browse_first_param = if has_filter { 4 } else { 3 };
    let (browse_clause, _) = browse_clause(browse, browse_first_param);
    let ai_clause = ai_exclude_clause(exclude_ai);
    let is_ai = ai_projection(project_ai);
    let presence = presence_clause(missing_flag);
    format!(
        "SELECT id, path, title, artist, album, album_artist, year, track_no, genre, \
         duration_ms, bitrate_kbps, rating, play_count, last_played_at, added_at, \
         file_mtime, missing_since, missing_reason, untagged, file_size, device, inode, \
         {is_ai} AS is_ai \
         FROM tracks WHERE {presence}{filter_clause}{browse_clause}{ai_clause} \
         ORDER BY {order_expr} {dir} LIMIT ?1 OFFSET ?2"
    )
}

/// Builds the parameterized SELECT for a library window (`PRESENT`).
/// See `build_track_query_base`'s doc comment for the whitelist guarantee.
/// Projects the real `is_ai` column (`project_ai = true`); the AI-gated hot
/// path uses `build_track_query_browsed` to opt out. Its only callers are this
/// module's tests.
pub fn build_track_query(sort_field: &str, sort_dir: &str, has_filter: bool) -> String {
    build_track_query_base(0, sort_field, sort_dir, has_filter, true)
}

pub(super) fn build_track_query_browsed(
    sort_field: &str,
    sort_dir: &str,
    has_filter: bool,
    browse: &BrowseFilter,
    exclude_ai: bool,
    project_ai: bool,
) -> String {
    build_track_query_base_browsed(
        0, sort_field, sort_dir, has_filter, browse, exclude_ai, project_ai,
    )
}

/// Builds the parameterized `SELECT id` for the queue seam
/// (`query_track_ids`, library/missing shape): every id matching
/// `(missing_flag, sort_field, sort_dir, filter)`, capped at `QUEUE_LIMIT` —
/// a literal, not a bound parameter, since it's a fixed Rust-side constant
/// rather than caller input (nothing to inject). Shares `order_expr_and_dir`/
/// `filter_clause` with `build_track_query_base` so the queue's ordering can
/// never drift from the track list's.
pub(super) fn build_track_ids_query_base(
    missing_flag: u8,
    sort_field: &str,
    sort_dir: &str,
    has_filter: bool,
) -> String {
    let (order_expr, dir) = order_expr_and_dir(sort_field, sort_dir);
    let filter_clause = filter_clause(has_filter, 1);
    let presence = presence_clause(missing_flag);
    format!(
        "SELECT id FROM tracks WHERE {presence}{filter_clause} \
         ORDER BY {order_expr} {dir} LIMIT {QUEUE_LIMIT}"
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
    let (order_expr, dir) = order_expr_and_dir(sort_field, sort_dir);
    let filter_clause = filter_clause(has_filter, 1);
    let browse_first_param = if has_filter { 2 } else { 1 };
    let (browse_clause, _) = browse_clause(browse, browse_first_param);
    let ai_clause = ai_exclude_clause(exclude_ai);
    format!(
        "SELECT id FROM tracks WHERE {PRESENT}{filter_clause}{browse_clause}{ai_clause} \
         ORDER BY {order_expr} {dir} LIMIT {QUEUE_LIMIT}"
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
        // INST-10: the `EXISTS(track_provenance … ai = 1) AS is_ai` column every
        // windowed track SELECT projects at index 22.
        is_ai: r.get::<_, i64>(22)? != 0,
    })
}

/// Same 22-column shape as `row_to_track`, plus a trailing `pt.position`
/// column (index 22) — used only by `query_track_window_playlist`, the one
/// query that actually joins `playlist_tracks AS pt`. See `Track::
/// playlist_position`'s doc comment for why this is the sole populating
/// call site.
pub(super) fn row_to_playlist_track(r: &rusqlite::Row) -> rusqlite::Result<Track> {
    let mut track = row_to_track(r)?;
    // `is_ai` sits at index 22 (read by `row_to_track`); `pt.position` follows
    // it at index 23 in the playlist SELECTs.
    track.playlist_position = Some(r.get(23)?);
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
        assert_eq!(
            order_expr_and_dir("play_count", "desc"),
            ("play_count", "DESC")
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
        // The INST-10 `is_ai` projection references `track_provenance` in every
        // build (via `EXISTS(...) AS is_ai`); it is the *exclude* clause
        // (`NOT EXISTS` in the WHERE) that toggles with `exclude_ai`.
        let off =
            build_track_query_browsed("title", "asc", false, &BrowseFilter::default(), false, true);
        assert!(
            !off.contains("NOT EXISTS"),
            "no exclude clause when the filter is off"
        );
        // The is_ai projection is still present regardless of the filter.
        assert!(off.contains("AS is_ai"));
        let on =
            build_track_query_browsed("title", "asc", false, &BrowseFilter::default(), true, true);
        assert!(on.contains("NOT EXISTS"));
        // The exclude clause sits inside the WHERE, before ORDER BY.
        let where_start = on.find("WHERE").unwrap();
        assert!(on.find("NOT EXISTS").unwrap() > where_start);
        assert!(on.find("NOT EXISTS").unwrap() < on.find("ORDER BY").unwrap());
    }

    // INST-10: the windowed track query projects `is_ai` from track_provenance —
    // true for a track with an `ai = 1` row, false otherwise.
    #[test]
    fn windowed_query_projects_is_ai_from_track_provenance() {
        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (id, path, title, artist, added_at, file_mtime, file_size) \
             VALUES (1, '/orig.flac', 'Original', 'A', 1, 1, 1), \
                    (2, '/instr.flac', 'Instrumental', 'A', 1, 1, 1); \
             INSERT INTO track_provenance (track_id, kind, ai, created_at) \
             VALUES (2, 'instrumental', 1, 1);",
        )
        .unwrap();

        let rows = crate::queries::query_track_window(
            &mut conn,
            &crate::view_source::ViewSource::Library,
            "title",
            "asc",
            "",
            0,
            100,
            &[],
        )
        .unwrap();
        let find = |id: i64| rows.iter().find(|t| t.id == id).expect("row present");
        assert!(!find(1).is_ai, "a plain track is not AI-manipulated");
        assert!(find(2).is_ai, "a track with an ai provenance row is AI");
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
                true,
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

    // INST-10 / FIX-4: the `is_ai` projection is gated. With `project_ai` set it
    // is the correlated provenance `EXISTS` the badge reads; without it, a
    // literal `0` so the plan carries no per-row subquery. Either way exactly one
    // `is_ai` column is projected, so `row_to_track`'s fixed index is unaffected.
    #[test]
    fn is_ai_projection_is_gated_and_the_off_path_has_no_subquery() {
        let on =
            build_track_query_browsed("title", "asc", false, &BrowseFilter::default(), false, true);
        assert!(
            on.contains("EXISTS(SELECT 1 FROM track_provenance"),
            "the on-path projects the correlated provenance EXISTS: {on}"
        );

        let off = build_track_query_browsed(
            "title",
            "asc",
            false,
            &BrowseFilter::default(),
            false,
            false,
        );
        assert!(
            off.contains("0 AS is_ai"),
            "the off-path projects a literal 0: {off}"
        );
        assert!(
            !off.contains("track_provenance"),
            "the off-path carries no provenance subquery: {off}"
        );

        assert_eq!(on.matches(" AS is_ai").count(), 1);
        assert_eq!(off.matches(" AS is_ai").count(), 1);
    }

    // FIX-4 plan evidence: EXPLAIN QUERY PLAN confirms the off-path (badge
    // hidden) plans no correlated subquery over `track_provenance`, while the
    // on-path's provenance lookup is backed by `track_provenance`'s INTEGER
    // PRIMARY KEY (a SEARCH, never a full SCAN — so no extra index is needed).
    #[test]
    fn explain_query_plan_confirms_off_path_has_no_provenance_subquery() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();

        let plan = |project_ai: bool| -> String {
            let sql = build_track_query_browsed(
                "title",
                "asc",
                false,
                &BrowseFilter::default(),
                false,
                project_ai,
            );
            let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            stmt.query_map([1000i64, 0i64], |row| row.get::<_, String>(3))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join(" | ")
                .to_lowercase()
        };

        let off = plan(false);
        assert!(
            !off.contains("subquery"),
            "the off-path plan has no correlated subquery: {off}"
        );
        assert!(
            !off.contains("track_provenance"),
            "the off-path plan never touches track_provenance: {off}"
        );

        let on = plan(true);
        assert!(
            on.contains("subquery") || on.contains("track_provenance"),
            "the on-path plan reads provenance for the badge: {on}"
        );
        assert!(
            !on.contains("scan track_provenance"),
            "the on-path provenance lookup is index/PK-backed, not a full scan: {on}"
        );
    }
}
