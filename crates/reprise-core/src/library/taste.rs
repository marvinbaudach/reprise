//! What the library itself says the user listens to, for surfaces that want
//! to suggest something instead of guessing.
//!
//! `RAD-5`: the Add Station chip that used to read a hard-coded "Metal in
//! DE" derives its genre here. The measure is **listening time**, not shelf
//! size: a large unplayed collection should not out-vote what actually gets
//! played. The same fact feeds the podcast add dialog, so both surfaces
//! suggest the same genre rather than each inventing its own rule.

use rusqlite::Connection;

use crate::db::Db;

/// Same clamp `stats_screen` uses: a listen never counts for more than the
/// track's own length, so one stuck position report cannot crown a genre.
const CLAMPED_MS: &str =
    "CASE WHEN duration_ms > 0 THEN MIN(ms_played, duration_ms) ELSE ms_played END";

/// The genre the library has spent the most time on.
///
/// Both fields describe one genre in the two shapes its callers need, so
/// neither of them has to re-derive the other and drift.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopGenre {
    /// What to show and what to type into a catalogue search — the library's
    /// own spelling, in the variant it played most ("Death Metal"), so a
    /// stray lowercase typo does not become the label.
    ///
    /// Multi-value genre fields ("Metal; Rock", "Death Metal/Grindcore")
    /// collapse to their first segment: directories tag one genre at a time
    /// and a combined string matches nothing at all.
    pub name: String,
    /// The same genre as a directory tag: lowercase, which is how
    /// radio-browser stores and matches them.
    pub tag: String,
}

/// `None` when nothing has been played yet, or when everything played
/// carries an empty genre — the caller is expected to drop its suggestion
/// rather than fall back to a genre nobody in this library listens to.
pub fn top_genre(db: &Db) -> Result<Option<TopGenre>, rusqlite::Error> {
    top_genre_in(db.conn())
}

/// The same fact against a bare connection — the seam the tests use, and the
/// one any caller already holding a connection can reach.
pub fn top_genre_in(conn: &Connection) -> Result<Option<TopGenre>, rusqlite::Error> {
    let sql = format!(
        "SELECT genre, COALESCE(SUM({CLAMPED_MS}), 0) AS played \
         FROM listen_events \
         WHERE TRIM(genre) <> '' \
         GROUP BY genre \
         ORDER BY played DESC, genre ASC"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(fold_variants(&rows))
}

/// Folds spelling variants of one genre together — SQLite groups
/// case-sensitively, so "Metal" and "metal" arrive as two rows and would
/// split the very listening time that should crown them.
fn fold_variants(rows: &[(String, i64)]) -> Option<TopGenre> {
    // (folding key, spelling to show, total listening time). Rows arrive
    // heaviest first, so the first spelling seen for a key is already its
    // most-played variant and later variants only add to the total.
    let mut folded: Vec<(String, String, i64)> = Vec::new();
    for (raw, played) in rows {
        let key = raw.trim().to_lowercase();
        if key.is_empty() {
            continue;
        }
        match folded.iter_mut().find(|(existing, _, _)| *existing == key) {
            Some((_, _, total)) => *total += played,
            None => folded.push((key, raw.trim().to_owned(), *played)),
        }
    }
    // Most time first; equal time falls back to the alphabet so the chip does
    // not change on every launch.
    folded.sort_by(|left, right| right.2.cmp(&left.2).then_with(|| left.0.cmp(&right.0)));
    folded.into_iter().next().map(|(_, spelling, _)| {
        let name = primary_segment(&spelling).to_owned();
        TopGenre {
            tag: name.to_lowercase(),
            name,
        }
    })
}

/// The first genre of a multi-value field. `;` is ID3's own separator and
/// `/` the common hand-written one; a comma is left alone because genre
/// names legitimately contain it ("Folk, World, & Country").
fn primary_segment(genre: &str) -> &str {
    genre
        .split([';', '/'])
        .map(str::trim)
        .find(|segment| !segment.is_empty())
        .unwrap_or(genre)
}

#[cfg(test)]
#[path = "taste_tests.rs"]
mod tests;
