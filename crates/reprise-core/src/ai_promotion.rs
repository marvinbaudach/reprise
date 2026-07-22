//! Promotion — turning a finished, undecided staging render into a real,
//! permanent, clearly-labelled library track (Beschluss 13/14; plan 2.4/3).
//!
//! The save decision runs this once per render: write the final tags (standard
//! fields with a `" (Instrumental)"` title suffix, the album left unchanged,
//! plus the AI-provenance scheme), copy the render into a dedicated subfolder
//! **inside the library root**, register it through the existing scanner
//! metadata path, and record provenance + the job's `save` event. **No
//! re-render.**
//!
//! ## Safety
//!
//! * A **path guard** refuses any destination outside
//!   `<library_root>/<subfolder>` — the one place Reprise ever creates audio
//!   files (plan 6). It is a lexical containment check plus per-component
//!   sanitisation, so a hostile artist/title tag can neither escape the
//!   subtree nor inject path separators.
//! * The render is **copied, not moved**, then registered, and only then is
//!   the staging original discarded. Any failure leaves staging intact
//!   (retryable) and removes the stray destination copy — no orphans. The one
//!   non-atomic seam is between the scanner's own registration transaction and
//!   the provenance/`save` transaction; a crash there leaves a track whose
//!   embedded AI tags let [`crate::provenance::reconstruct_provenance`] rebuild
//!   the row on the next scan (Beschluss 13), so the system self-heals.

use std::path::{Component, Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};

use crate::ai_jobs::{self, JobState};
use crate::ai_staging::StagingStore;
use crate::library::scanner::{self, ScanOutcome};
use crate::library::tag_edit::{self, TagPatch};
use crate::provenance::{self, AiTagSet, ProvenanceInput, KIND_VOCALS_REMOVED};

/// The default dedicated subfolder, relative to the library root (Beschluss
/// 13). Configurable via [`PromotionConfig::subfolder`].
pub const DEFAULT_INSTRUMENTAL_SUBFOLDER: &str = "Reprise Instrumentals";

/// The suffix appended to the title tag and filename (Beschluss 14).
const INSTRUMENTAL_SUFFIX: &str = " (Instrumental)";

/// Where and how promoted instrumentals are filed.
#[derive(Debug, Clone)]
pub struct PromotionConfig {
    /// The single library scan root.
    pub library_root: PathBuf,
    /// The dedicated subfolder under the root (Beschluss 13, configurable).
    pub subfolder: String,
}

impl PromotionConfig {
    /// A config using the default subfolder under `library_root`.
    pub fn new(library_root: impl Into<PathBuf>) -> Self {
        Self {
            library_root: library_root.into(),
            subfolder: DEFAULT_INSTRUMENTAL_SUBFOLDER.to_string(),
        }
    }

    /// The absolute root every promoted file must live under — the guard's
    /// allowed subtree.
    pub fn instrumentals_root(&self) -> PathBuf {
        self.library_root.join(&self.subfolder)
    }
}

/// What a successful promotion produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionOutcome {
    pub result_track_id: i64,
    pub path: PathBuf,
}

/// Why a promotion could not complete.
#[derive(Debug, thiserror::Error)]
pub enum PromotionError {
    #[error("job {0} does not exist")]
    JobNotFound(i64),
    #[error("job {0} is not a finished, unsaved render")]
    NotPromotable(i64),
    #[error("no staging render exists for job {0}")]
    StagingMissing(i64),
    #[error("source metadata for the render is unavailable")]
    SourceMetadataUnavailable,
    #[error("refusing to write outside the instrumentals folder: {attempted}")]
    PathGuard { attempted: String },
    #[error("could not write final tags: {0}")]
    Tag(String),
    #[error("filesystem error during promotion: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not register the promoted track: {0}")]
    Registration(String),
    #[error("database error during promotion: {0}")]
    Db(#[from] rusqlite::Error),
}

/// The source track's fields the final tags are built from.
struct SourceMeta {
    title: String,
    artist: String,
    album: String,
    album_artist: String,
    year: Option<i32>,
    track_no: Option<i32>,
    genre: String,
}

/// Promotes the finished staging render of `job_id` into the library.
pub fn promote(
    conn: &mut Connection,
    staging: &StagingStore,
    config: &PromotionConfig,
    job_id: i64,
    now: i64,
) -> Result<PromotionOutcome, PromotionError> {
    let job = ai_jobs::get_job(conn, job_id)?.ok_or(PromotionError::JobNotFound(job_id))?;
    // Only a finished, not-yet-saved render is promotable; re-saving a job is a
    // no-op guarded here (idempotent from the caller's view).
    if job.state != JobState::Done || job.result_track_id.is_some() {
        return Err(PromotionError::NotPromotable(job_id));
    }
    let staging_path = staging.path_for_job(job_id);
    if !staging_path.is_file() {
        return Err(PromotionError::StagingMissing(job_id));
    }
    let source = read_source_meta(conn, job.source_track_id, &staging_path)?;
    let destination = resolve_destination(config, &source)?;

    write_final_tags(&staging_path, &source, &job.params_fingerprint)?;

    // Copy (not move) into place, then register, then discard staging — so any
    // failure leaves the staging original for a retry and removes the stray copy.
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&staging_path, &destination)?;

    let result = register_and_record(conn, &source, &job, &destination, now);
    match result {
        Ok(result_track_id) => {
            // Success: the render now lives in the library; drop the staging copy.
            let _ = staging.discard(job_id);
            Ok(PromotionOutcome {
                result_track_id,
                path: destination,
            })
        }
        Err(error) => {
            // Roll back the filesystem side; staging stays for a retry.
            let _ = std::fs::remove_file(&destination);
            Err(error)
        }
    }
}

/// Registers the copied file and records provenance + the job `save` event.
fn register_and_record(
    conn: &mut Connection,
    source: &SourceMeta,
    job: &ai_jobs::AiJob,
    destination: &Path,
    now: i64,
) -> Result<i64, PromotionError> {
    // Register through the existing scanner metadata path (its own transaction).
    match scanner::scan_folder(conn, destination) {
        Ok(ScanOutcome::Completed(report)) if report.errors == 0 => {}
        Ok(ScanOutcome::Completed(report)) => {
            return Err(PromotionError::Registration(format!(
                "scan reported {} error(s)",
                report.errors
            )));
        }
        Ok(ScanOutcome::RootUnavailable { root }) => {
            return Err(PromotionError::Registration(format!(
                "instrumentals root unavailable: {}",
                root.display()
            )));
        }
        Err(error) => return Err(PromotionError::Registration(error.to_string())),
    }
    let result_track_id: i64 = conn
        .query_row(
            "SELECT id FROM tracks WHERE path = ?1",
            [destination.to_string_lossy()],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| {
            PromotionError::Registration("registered track vanished before provenance".to_string())
        })?;

    let source_text = format!("{} — {}", source.artist, source.title);
    // Provenance + the staged->saved job transition land in one transaction.
    crate::events::in_txn(conn, |conn| {
        provenance::insert_provenance(
            conn,
            result_track_id,
            &ProvenanceInput {
                kind: KIND_VOCALS_REMOVED.to_string(),
                ai: true,
                // The link is by id only while the original still exists; the
                // embedded tags carry the textual reference regardless.
                source_track_id: job.source_track_id,
                source_text: Some(source_text),
                source_mbid: None,
                model: Some(job.params_fingerprint.clone()),
            },
            now,
        )?;
        ai_jobs::attach_result_track(conn, job.id, result_track_id)?;
        Ok(())
    })?;
    Ok(result_track_id)
}

/// Reads the source track's metadata from the DB, falling back to the staging
/// render's own embedded tags when the original has been deleted (its id was
/// nulled on the job). Either way the standard tags and the textual source
/// reference get sensible values.
fn read_source_meta(
    conn: &Connection,
    source_track_id: Option<i64>,
    staging_path: &Path,
) -> Result<SourceMeta, PromotionError> {
    if let Some(id) = source_track_id {
        let row = conn
            .query_row(
                "SELECT title, artist, album, album_artist, year, track_no, genre \
                 FROM tracks WHERE id = ?1",
                [id],
                |row| {
                    Ok(SourceMeta {
                        title: row.get(0)?,
                        artist: row.get(1)?,
                        album: row.get(2)?,
                        album_artist: row.get(3)?,
                        year: row.get(4)?,
                        track_no: row.get(5)?,
                        genre: row.get(6)?,
                    })
                },
            )
            .optional()?;
        if let Some(meta) = row {
            return Ok(meta);
        }
    }
    // Fallback: the original is gone — read what the render itself carries.
    let tagged = tag_edit::read_editable_tags(staging_path)
        .map_err(|_| PromotionError::SourceMetadataUnavailable)?;
    Ok(SourceMeta {
        title: tagged.title,
        artist: tagged.artist,
        album: tagged.album,
        album_artist: tagged.album_artist,
        year: tagged.year.map(|year| year as i32),
        track_no: tagged.track_no.map(|track| track as i32),
        genre: tagged.genre,
    })
}

/// Writes the final tags onto the staging render: standard fields through the
/// ordinary lofty path (title gains the suffix, album is untouched), then the
/// AI-provenance scheme.
fn write_final_tags(
    staging_path: &Path,
    source: &SourceMeta,
    model_id: &str,
) -> Result<(), PromotionError> {
    let patch = TagPatch {
        title: Some(format!("{}{INSTRUMENTAL_SUFFIX}", source.title)),
        artist: Some(source.artist.clone()),
        album: Some(source.album.clone()),
        album_artist: Some(source.album_artist.clone()),
        year: Some(source.year.map(|year| year.max(0) as u32)),
        track_no: Some(source.track_no.map(|track| track.max(0) as u32)),
        genre: Some(source.genre.clone()),
    };
    tag_edit::apply_patch_to_file(staging_path, &patch)
        .map_err(|error| PromotionError::Tag(error.to_string()))?;
    provenance::write_ai_tags(
        staging_path,
        &AiTagSet {
            kind: KIND_VOCALS_REMOVED.to_string(),
            model: model_id.to_string(),
            source_text: Some(format!("{} — {}", source.artist, source.title)),
            source_mbid: None,
        },
    )
    .map_err(|error| PromotionError::Tag(error.to_string()))?;
    Ok(())
}

/// Builds the destination path and enforces the guard: the file must land at
/// `<root>/<subfolder>/<Artist>/<Title> (Instrumental).flac`, strictly inside
/// the instrumentals subtree.
fn resolve_destination(
    config: &PromotionConfig,
    source: &SourceMeta,
) -> Result<PathBuf, PromotionError> {
    let allowed_root = config.instrumentals_root();
    let artist_dir = sanitize_component(&source.artist);
    let file_name = format!(
        "{}{INSTRUMENTAL_SUFFIX}.flac",
        sanitize_component(&source.title)
    );
    let destination = allowed_root.join(&artist_dir).join(&file_name);
    if !is_within(&allowed_root, &destination) {
        return Err(PromotionError::PathGuard {
            attempted: destination.to_string_lossy().into_owned(),
        });
    }
    Ok(destination)
}

/// Makes one tag value safe as a single path component: strips separators and
/// control characters, neutralises `.`/`..`, trims, and never yields an empty
/// string (so a blank artist/title still produces a valid, contained path).
fn sanitize_component(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|character| {
            if character == '/'
                || character == '\\'
                || character == std::path::MAIN_SEPARATOR
                || character.is_control()
            {
                '_'
            } else {
                character
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        "Unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Whether `candidate` is lexically contained in `root` (both resolved for
/// `.`/`..` without touching the filesystem — the target does not exist yet).
fn is_within(root: &Path, candidate: &Path) -> bool {
    let root = lexical_normalize(root);
    let candidate = lexical_normalize(candidate);
    candidate.starts_with(&root) && candidate != root
}

/// Resolves `.`/`..` components lexically (no filesystem access).
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !matches!(
                    out.last(),
                    None | Some(Component::RootDir | Component::Prefix(_))
                ) {
                    out.pop();
                }
            }
            other => out.push(other),
        }
    }
    out.iter().collect()
}

#[cfg(test)]
#[path = "ai_promotion_tests.rs"]
mod tests;
