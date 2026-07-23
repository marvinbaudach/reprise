//! Promotion — turning a finished, undecided staging render into a real,
//! permanent, clearly-labelled library track (Beschluss 13/14; plan 2.4/3).
//!
//! The save decision runs this once per render: copy the render into a
//! dedicated subfolder **inside the library root**, write the final tags onto
//! that copy (standard fields with a `" (Instrumental)"` title suffix, the
//! album left unchanged, plus the AI-provenance scheme) — leaving the staging
//! render pristine for a retry — register it through the existing scanner
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
//!   the row on the next scan (Beschluss 13), so the system self-heals. If the
//!   provenance/`save` transaction *fails* (not a crash), the destination copy
//!   is removed but the scanner-committed `tracks` row is left in place; the
//!   next scan of the instrumentals root marks it missing (its file is gone),
//!   and it is never a promotion result, so the collision resolver correctly
//!   ignores it and a retry reuses the same base path.

use std::path::{Component, Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

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

/// Appends [`INSTRUMENTAL_SUFFIX`] unless the base already ends with it. Making
/// the suffix idempotent means a retry that re-reads an already-suffixed title
/// (e.g. from the render's own tags once the original was deleted) never
/// produces `"Title (Instrumental) (Instrumental)"`.
fn with_instrumental_suffix(base: &str) -> String {
    if base.ends_with(INSTRUMENTAL_SUFFIX) {
        base.to_string()
    } else {
        format!("{base}{INSTRUMENTAL_SUFFIX}")
    }
}

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

/// What completing a render via [`complete_render`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionOutcome {
    /// `mark_done` did not apply — the caller is not the owner, or the job is
    /// not `running`. Nothing else ran.
    NotOwned,
    /// The render was marked `done` and left in staging for a manual save
    /// (the job carried no auto-promote intent).
    Staged,
    /// The render was marked `done` and auto-promoted into the library.
    Promoted(PromotionOutcome),
    /// The render was marked `done`, but the requested auto-promotion failed.
    /// The job stays `done` + unsaved with its render still in staging and
    /// `error_kind` noted, so the promotion is retryable.
    PromotionDeferred { error: String },
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
    let destination = resolve_destination(conn, config, job_id, &source)?;

    // Copy (not move) into place, tag the *copy*, register, then discard
    // staging — so any failure leaves the staging original pristine for a retry
    // and removes the stray destination copy. The final tags are written to the
    // destination, never to the staging render, so a retry after a failure
    // re-reads the original (un-suffixed) title instead of a mutated one.
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&staging_path, &destination)?;

    let result = write_final_tags(&destination, &source, &job.params_fingerprint)
        .and_then(|()| register_and_record(conn, &source, &job, &destination, now));
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

/// The worker completion path: marks `job_id`'s render `done` (owner-guarded,
/// exactly [`ai_jobs::mark_done`]), then — when the job was enqueued with the
/// auto-promote intent (decision 15: MCP/CLI create-instrumental saves by
/// default) — promotes the fresh render in the same call, so a worker needs no
/// promotion logic of its own.
///
/// A promotion failure never corrupts job state: the job is already `done`, and
/// on a failed promotion it stays `done` + unsaved with its staging render
/// intact and `error_kind` noted — exactly the retryable state a manual save
/// resumes from. Only the owner's `running` job transitions; a non-owner or
/// non-running call is [`CompletionOutcome::NotOwned`] and promotes nothing.
pub fn complete_render(
    conn: &mut Connection,
    staging: &StagingStore,
    config: &PromotionConfig,
    job_id: i64,
    worker: i64,
    now: i64,
) -> Result<CompletionOutcome, PromotionError> {
    // The ordinary owner-guarded done transition — unchanged for every worker.
    if !ai_jobs::mark_done(conn, job_id, worker, now)? {
        return Ok(CompletionOutcome::NotOwned);
    }
    // Honor the persisted save-intent; without it the render waits in staging.
    if !ai_jobs::job_auto_promote(conn, job_id)? {
        return Ok(CompletionOutcome::Staged);
    }
    match promote(conn, staging, config, job_id, now) {
        Ok(outcome) => Ok(CompletionOutcome::Promoted(outcome)),
        Err(error) => {
            // Keep the job done + unsaved (retryable), just note why it deferred.
            let message = error.to_string();
            ai_jobs::note_promotion_error(conn, job_id, &message)?;
            Ok(CompletionOutcome::PromotionDeferred { error: message })
        }
    }
}

/// The worker completion path for a render written to a **claim-scoped temp
/// file** — the owner-guarded, publish-safe superset of [`complete_render`].
///
/// The order is what makes it safe against the orphan-resurrection race: the
/// owner-guarded `mark_done` runs **first**, and only the winner then renames
/// `temp_path` to the job's canonical staging path. A straggler whose lease was
/// reclaimed (or whose job has since gone terminal) fails the guard, so it never
/// touches the canonical file; its worthless temp is deleted. This closes the
/// window where a straggler could rename its temp over a staging path the winner
/// had already promoted-and-discarded (or the user had discarded), resurrecting a
/// permanent orphan that no listing shows and no sweep removes.
///
/// `config` is the promotion target, or `None` when no library root is set —
/// then an intent-carrying render is published and simply left staged, waiting
/// for a manual save or a rooted worker. With a root, a job's persisted
/// save-intent is honored exactly as in [`complete_render`]. A promotion failure
/// leaves the job `done` + unsaved with its render in staging and `error_kind`
/// noted (retryable). The temp file is consumed either way: renamed on a win,
/// deleted on a lost guard.
pub fn complete_render_with_publish(
    conn: &mut Connection,
    staging: &StagingStore,
    config: Option<&PromotionConfig>,
    job_id: i64,
    worker: i64,
    temp_path: &Path,
    now: i64,
) -> Result<CompletionOutcome, PromotionError> {
    // 1. Owner-guarded done transition FIRST — the ownership decision. A
    //    straggler (reclaimed lease, or an already-terminal job) fails here and
    //    must never reach the canonical staging path; drop its worthless temp.
    if !ai_jobs::mark_done(conn, job_id, worker, now)? {
        let _ = std::fs::remove_file(temp_path);
        return Ok(CompletionOutcome::NotOwned);
    }
    // 2. We are the sole owner: publish the temp render into its canonical path
    //    with one atomic rename. Only the winner ever writes this file.
    std::fs::rename(temp_path, staging.path_for_job(job_id))?;
    // 3. Honor the persisted save-intent when a library root is configured;
    //    without one the render simply waits in staging.
    let Some(config) = config else {
        return Ok(CompletionOutcome::Staged);
    };
    if !ai_jobs::job_auto_promote(conn, job_id)? {
        return Ok(CompletionOutcome::Staged);
    }
    match promote(conn, staging, config, job_id, now) {
        Ok(outcome) => Ok(CompletionOutcome::Promoted(outcome)),
        Err(error) => {
            let message = error.to_string();
            ai_jobs::note_promotion_error(conn, job_id, &message)?;
            Ok(CompletionOutcome::PromotionDeferred { error: message })
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

/// Writes the final tags onto the promoted file — the destination copy, never
/// the staging render: standard fields through the ordinary lofty path (title
/// gains the suffix, album is untouched), then the AI-provenance scheme. Tagging
/// the copy keeps staging's tags exactly as the worker wrote them, so a retry
/// re-reads the original, un-suffixed title.
fn write_final_tags(
    path: &Path,
    source: &SourceMeta,
    model_id: &str,
) -> Result<(), PromotionError> {
    let patch = TagPatch {
        title: Some(with_instrumental_suffix(&source.title)),
        artist: Some(source.artist.clone()),
        album: Some(source.album.clone()),
        album_artist: Some(source.album_artist.clone()),
        year: Some(source.year.map(|year| year.max(0) as u32)),
        track_no: Some(source.track_no.map(|track| track.max(0) as u32)),
        genre: Some(source.genre.clone()),
    };
    tag_edit::apply_patch_to_file(path, &patch)
        .map_err(|error| PromotionError::Tag(error.to_string()))?;
    provenance::write_ai_tags(
        path,
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
///
/// Collision-aware: two different source tracks that sanitise to the same base
/// name (a cover, a live version, a duplicate import — all sharing Artist +
/// Title) must not resolve to the same file, or the second `fs::copy` would
/// clobber the first render and the re-registration would resolve the *same*
/// `result_track_id`, flipping its provenance. When the base name is already a
/// *different* job's saved result, the name is uniquified deterministically by
/// appending `" (2)"`, `" (3)"`, … before the extension. The suffix is stable
/// across retries of the *same* job: a candidate that is free, or is this job's
/// own earlier file, is not treated as a collision, so a retry reuses its own
/// destination instead of allocating a fresh suffix.
fn resolve_destination(
    conn: &Connection,
    config: &PromotionConfig,
    job_id: i64,
    source: &SourceMeta,
) -> Result<PathBuf, PromotionError> {
    let allowed_root = config.instrumentals_root();
    let directory = allowed_root.join(sanitize_component(&source.artist));
    let stem = with_instrumental_suffix(&sanitize_component(&source.title));

    let mut attempt: u32 = 1;
    loop {
        let file_name = if attempt == 1 {
            format!("{stem}.flac")
        } else {
            format!("{stem} ({attempt}).flac")
        };
        let candidate = directory.join(&file_name);
        if !is_within(&allowed_root, &candidate) {
            return Err(PromotionError::PathGuard {
                attempted: candidate.to_string_lossy().into_owned(),
            });
        }
        if !destination_reserved_by_other_job(conn, &candidate, job_id)? {
            return Ok(candidate);
        }
        attempt += 1;
    }
}

/// Whether `candidate` is already the committed, saved result of a *different*
/// job. Such a path is off-limits: copying onto it overwrites another
/// promotion's render, and re-registering it resolves the *same* track id,
/// which would flip that track's provenance to this source. A path that is free
/// — or that belongs to *this* job (a retry over its own earlier file) — is not
/// reserved, so a retry deterministically reuses its own destination.
fn destination_reserved_by_other_job(
    conn: &Connection,
    candidate: &Path,
    job_id: i64,
) -> Result<bool, PromotionError> {
    let taken: Option<i64> = conn
        .query_row(
            "SELECT j.id FROM ai_jobs j \
             JOIN tracks t ON t.id = j.result_track_id \
             WHERE t.path = ?1 AND j.id != ?2 LIMIT 1",
            params![candidate.to_string_lossy(), job_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(taken.is_some())
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
