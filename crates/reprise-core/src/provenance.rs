//! Track provenance — the DB registry of AI-manipulated/-generated tracks, and
//! the embedded-tag scheme (plan 2.4.5) that carries the same facts inside the
//! file so they survive outside Reprise and reconstruct a fresh database.
//!
//! Two layers:
//!
//! * **DB registry** (`track_provenance`): the queryable truth. The AI filter
//!   (plan 2.4/8, Beschluss 17) keys on the `ai` flag here — never on paths.
//!   `source_track_id` is optional: deleting the original nulls it (FK) while
//!   `source_text` keeps the human reference (Beschluss 16), and a future
//!   generated track never had a source track at all.
//! * **Tag scheme** (`REPRISE_AI*` Vorbis comments + a human comment): the
//!   durable, app-independent disclosure. Reprise only ever produces FLAC
//!   instrumentals (Beschluss 15), so the writer/reader work on FLAC Vorbis
//!   comments — the one place lofty preserves custom keys (its generic `Tag`
//!   drops them). App-internal ids are **never** written to tags (Beschluss
//!   13); the source reference is textual plus an optional MusicBrainz id, so
//!   it outlives any database.

use std::path::Path;

use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::AudioFile;
use lofty::flac::FlacFile;
use lofty::ogg::VorbisComments;
use rusqlite::{params, Connection, OptionalExtension};

/// The `REPRISE_AI` Vorbis key — presence of this key *is* the AI flag in a
/// file. Its value is the manipulation kind.
pub const TAG_AI: &str = "REPRISE_AI";
/// `REPRISE_AI_MODEL` — `"<name>@<version>"` of the model that produced it.
pub const TAG_AI_MODEL: &str = "REPRISE_AI_MODEL";
/// `REPRISE_AI_SOURCE` — the textual `"<Artist> — <Title>"` source reference.
pub const TAG_AI_SOURCE: &str = "REPRISE_AI_SOURCE";
/// `REPRISE_AI_SOURCE_MBID` — the optional MusicBrainz recording id of the
/// source (the only stable, app-independent id ever put in a tag).
pub const TAG_AI_SOURCE_MBID: &str = "REPRISE_AI_SOURCE_MBID";
/// The Vorbis comment key for the human-readable disclosure.
const TAG_COMMENT: &str = "COMMENT";

/// The v1 manipulation kind — vocal removal (Beschluss 19: instrumental only).
pub const KIND_VOCALS_REMOVED: &str = "vocals-removed";

/// The set of provenance facts written into / read from a file's tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiTagSet {
    /// The manipulation kind, e.g. [`KIND_VOCALS_REMOVED`].
    pub kind: String,
    /// `"<name>@<version>"` of the producing model.
    pub model: String,
    /// `"<Artist> — <Title>"` of the source, when there is one.
    pub source_text: Option<String>,
    /// MusicBrainz recording id of the source, when known.
    pub source_mbid: Option<String>,
}

/// A `track_provenance` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackProvenance {
    pub track_id: i64,
    pub kind: String,
    pub ai: bool,
    pub source_track_id: Option<i64>,
    pub source_text: Option<String>,
    pub source_mbid: Option<String>,
    pub model: Option<String>,
    pub created_at: i64,
}

/// The fields needed to register provenance for a track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceInput {
    pub kind: String,
    pub ai: bool,
    pub source_track_id: Option<i64>,
    pub source_text: Option<String>,
    pub source_mbid: Option<String>,
    pub model: Option<String>,
}

/// The human-readable disclosure written to the comment field for a kind.
pub fn human_comment(kind: &str) -> String {
    match kind {
        KIND_VOCALS_REMOVED => "AI-manipulated: vocals removed (Reprise)".to_string(),
        other => format!("AI-manipulated: {other} (Reprise)"),
    }
}

// --- DB registry -----------------------------------------------------------

/// Registers provenance for `track_id`. Idempotent (the track id is the
/// primary key): a re-promotion or a reconstruction pass overwrites cleanly.
/// Runs within the caller's transaction when there is one (promotion bundles
/// this with the track row and its events), otherwise standalone.
pub fn insert_provenance(
    db: &crate::db::Db,
    track_id: i64,
    input: &ProvenanceInput,
    now: i64,
) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    insert_provenance_in(conn, track_id, input, now)
}

fn insert_provenance_in(
    conn: &Connection,
    track_id: i64,
    input: &ProvenanceInput,
    now: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT OR REPLACE INTO track_provenance \
           (track_id, kind, ai, source_track_id, source_text, source_mbid, model, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            track_id,
            input.kind,
            i64::from(input.ai),
            input.source_track_id,
            input.source_text,
            input.source_mbid,
            input.model,
            now,
        ],
    )?;
    Ok(())
}

/// Reads a track's provenance, or `None` when it has none.
pub fn get_provenance(
    db: &crate::db::Db,
    track_id: i64,
) -> Result<Option<TrackProvenance>, rusqlite::Error> {
    let conn = db.conn();
    get_provenance_in(conn, track_id)
}

fn get_provenance_in(
    conn: &Connection,
    track_id: i64,
) -> Result<Option<TrackProvenance>, rusqlite::Error> {
    conn.query_row(
        "SELECT track_id, kind, ai, source_track_id, source_text, source_mbid, model, created_at \
         FROM track_provenance WHERE track_id = ?1",
        [track_id],
        map_provenance_row,
    )
    .optional()
}

/// Whether a track is flagged AI — the predicate the exclude filter and the
/// UI badge both ask. Keyed on the DB flag, never on a path (plan 2.4/8).
pub fn is_ai_track(db: &crate::db::Db, track_id: i64) -> Result<bool, rusqlite::Error> {
    let conn = db.conn();
    let flagged: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM track_provenance WHERE track_id = ?1 AND ai = 1",
            [track_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(flagged.is_some())
}

fn map_provenance_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrackProvenance> {
    Ok(TrackProvenance {
        track_id: row.get(0)?,
        kind: row.get(1)?,
        ai: row.get::<_, i64>(2)? != 0,
        source_track_id: row.get(3)?,
        source_text: row.get(4)?,
        source_mbid: row.get(5)?,
        model: row.get(6)?,
        created_at: row.get(7)?,
    })
}

// --- Tag scheme (FLAC Vorbis comments) -------------------------------------

/// Failure writing the provenance tags.
#[derive(Debug, thiserror::Error)]
pub enum ProvenanceTagError {
    #[error("could not open render for tagging: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not write provenance tags: {0}")]
    Lofty(#[from] lofty::error::LoftyError),
}

/// Writes the `REPRISE_AI*` scheme plus a human-readable comment into a FLAC
/// render's Vorbis comments, preserving any standard tags already present
/// (`insert` replaces only the touched keys). Idempotent — re-writing the same
/// set is a no-op in content.
pub fn write_ai_tags(path: &Path, tags: &AiTagSet) -> Result<(), ProvenanceTagError> {
    let mut reader = std::fs::File::open(path)?;
    let mut flac = FlacFile::read_from(&mut reader, ParseOptions::new())?;
    drop(reader);
    if flac.vorbis_comments().is_none() {
        flac.set_vorbis_comments(VorbisComments::new());
    }
    let comments = flac
        .vorbis_comments_mut()
        .expect("vorbis comments just ensured present");
    comments.insert(TAG_AI.to_string(), tags.kind.clone());
    comments.insert(TAG_AI_MODEL.to_string(), tags.model.clone());
    if let Some(source_text) = &tags.source_text {
        comments.insert(TAG_AI_SOURCE.to_string(), source_text.clone());
    }
    if let Some(source_mbid) = &tags.source_mbid {
        comments.insert(TAG_AI_SOURCE_MBID.to_string(), source_mbid.clone());
    }
    comments.insert(TAG_COMMENT.to_string(), human_comment(&tags.kind));
    flac.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

/// Reads the `REPRISE_AI*` scheme back from a FLAC render, or `None` when the
/// file is not a readable FLAC or carries no `REPRISE_AI` key. Best-effort
/// (Beschluss 13): a non-FLAC or unreadable file simply yields `None` — Reprise
/// only ever writes these tags to FLAC, so nothing else can carry them.
pub fn read_ai_tags(path: &Path) -> Option<AiTagSet> {
    let mut reader = std::fs::File::open(path).ok()?;
    let flac = FlacFile::read_from(&mut reader, ParseOptions::new()).ok()?;
    let comments = flac.vorbis_comments()?;
    let kind = comments.get(TAG_AI)?.to_string();
    Some(AiTagSet {
        kind,
        model: comments.get(TAG_AI_MODEL).unwrap_or_default().to_string(),
        source_text: comments.get(TAG_AI_SOURCE).map(str::to_string),
        source_mbid: comments.get(TAG_AI_SOURCE_MBID).map(str::to_string),
    })
}

// --- Rescan reconstruction (Beschluss 13) ----------------------------------

/// Rebuilds a single track's provenance from its embedded tags when the DB has
/// none — the fresh-database path. The source reference becomes textual only
/// (`source_track_id` stays `NULL`): tags never carry app-internal ids, so the
/// original cannot be re-linked by id, exactly as Beschluss 13/14 intend. A
/// track that already has provenance, or whose file carries no `REPRISE_AI`
/// tag, is left untouched. Returns whether a row was reconstructed.
pub fn reconstruct_provenance(
    db: &crate::db::Db,
    track_id: i64,
    path: &Path,
    now: i64,
) -> Result<bool, rusqlite::Error> {
    let conn = db.conn();
    reconstruct_provenance_in(conn, track_id, path, now)
}

fn reconstruct_provenance_in(
    conn: &Connection,
    track_id: i64,
    path: &Path,
    now: i64,
) -> Result<bool, rusqlite::Error> {
    if get_provenance_in(conn, track_id)?.is_some() {
        return Ok(false);
    }
    let Some(tags) = read_ai_tags(path) else {
        return Ok(false);
    };
    insert_provenance_in(
        conn,
        track_id,
        &ProvenanceInput {
            kind: tags.kind,
            ai: true,
            source_track_id: None,
            source_text: tags.source_text,
            source_mbid: tags.source_mbid,
            model: Some(tags.model),
        },
        now,
    )?;
    Ok(true)
}

/// Reconstructs provenance for every present track that lacks a DB row by
/// reading its embedded tags — the post-rescan sweep for a fresh database
/// (Beschluss 13). Returns how many rows were reconstructed. Best-effort and
/// idempotent: already-known and non-AI tracks are skipped.
pub fn reconstruct_all_missing(db: &crate::db::Db, now: i64) -> Result<usize, rusqlite::Error> {
    let conn = db.conn();
    let mut statement = conn.prepare(
        "SELECT t.id, t.path FROM tracks t \
         LEFT JOIN track_provenance p ON p.track_id = t.id \
         WHERE p.track_id IS NULL AND t.missing_since IS NULL AND t.removed_at IS NULL",
    )?;
    let candidates: Vec<(i64, String)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    drop(statement);
    let mut reconstructed = 0;
    for (track_id, path) in candidates {
        if reconstruct_provenance_in(conn, track_id, Path::new(&path), now)? {
            reconstructed += 1;
        }
    }
    Ok(reconstructed)
}

#[cfg(test)]
#[path = "provenance_tests.rs"]
mod tests;
