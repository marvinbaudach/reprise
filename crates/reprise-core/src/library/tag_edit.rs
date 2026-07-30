//! Selective classic-tag editing. The patch model makes “unchanged” an
//! explicit state so a multi-selection can never clobber per-track values.

use std::path::Path;
use std::path::PathBuf;

use lofty::prelude::*;
use lofty::tag::ItemKey;
use rusqlite::Connection;

use crate::db::Db;

pub use super::tag_edit_seed::{
    live_track_edit_seed_by_path, track_edit_seed_by_id, TrackEditSeed,
};
pub use super::tag_edit_write::{apply_track_writes, TrackWrite};
pub use super::tag_mutation::{classify_write_error, WriteErrorKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MixedValue<T> {
    Uniform(T),
    Mixed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditableTags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: Option<u32>,
    pub track_no: Option<u32>,
    pub genre: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditableTagSummary {
    pub title: MixedValue<String>,
    pub artist: MixedValue<String>,
    pub album: MixedValue<String>,
    pub album_artist: MixedValue<String>,
    pub year: MixedValue<Option<u32>>,
    pub track_no: MixedValue<Option<u32>>,
    pub genre: MixedValue<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagPatch {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<Option<u32>>,
    pub track_no: Option<Option<u32>>,
    pub genre: Option<String>,
}

impl TagPatch {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.artist.is_none()
            && self.album.is_none()
            && self.album_artist.is_none()
            && self.year.is_none()
            && self.track_no.is_none()
            && self.genre.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackEditPatch {
    pub tags: TagPatch,
    pub rating: Option<i32>,
}

impl TrackEditPatch {
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.rating.is_none()
    }
}

pub fn summarize_values<T: Clone + PartialEq>(values: &[T]) -> Option<MixedValue<T>> {
    let first = values.first()?;
    if values[1..].iter().all(|value| value == first) {
        Some(MixedValue::Uniform(first.clone()))
    } else {
        Some(MixedValue::Mixed)
    }
}

pub fn summarize(tags: &[EditableTags]) -> Option<EditableTagSummary> {
    fn field<T: Clone + PartialEq>(
        tags: &[EditableTags],
        get: impl Fn(&EditableTags) -> &T,
    ) -> MixedValue<T> {
        let first = get(&tags[0]);
        if tags[1..].iter().all(|tag| get(tag) == first) {
            MixedValue::Uniform(first.clone())
        } else {
            MixedValue::Mixed
        }
    }

    tags.first()?;
    Some(EditableTagSummary {
        title: field(tags, |tag| &tag.title),
        artist: field(tags, |tag| &tag.artist),
        album: field(tags, |tag| &tag.album),
        album_artist: field(tags, |tag| &tag.album_artist),
        year: field(tags, |tag| &tag.year),
        track_no: field(tags, |tag| &tag.track_no),
        genre: field(tags, |tag| &tag.genre),
    })
}

#[derive(Debug, thiserror::Error)]
pub enum TagEditError {
    #[error("tag operation failed: {0}")]
    Lofty(#[from] lofty::error::LoftyError),
    #[error("audio format has no writable tag type")]
    NoWritableTag,
}

pub fn read_editable_tags(path: &Path) -> Result<EditableTags, TagEditError> {
    let tagged = lofty::read_from_path(path)?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    Ok(EditableTags {
        title: tag
            .and_then(Accessor::title)
            .unwrap_or_default()
            .to_string(),
        artist: tag
            .and_then(Accessor::artist)
            .unwrap_or_default()
            .to_string(),
        album: tag
            .and_then(Accessor::album)
            .unwrap_or_default()
            .to_string(),
        album_artist: tag
            .and_then(|tag| tag.get_string(ItemKey::AlbumArtist))
            .unwrap_or_default()
            .to_string(),
        year: tag
            .and_then(Accessor::date)
            .map(|date| u32::from(date.year)),
        track_no: tag.and_then(Accessor::track),
        genre: tag
            .and_then(Accessor::genre)
            .unwrap_or_default()
            .to_string(),
    })
}

pub fn apply_patch_to_file(path: &Path, patch: &TagPatch) -> Result<(), TagEditError> {
    super::tag_mutation::apply_tag_patch_to_file(path, patch)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagWriteFailure {
    pub id: i64,
    pub path: PathBuf,
    /// Classified reason (FB-3: no raw Lofty prose in user-facing surfaces).
    pub kind: WriteErrorKind,
    pub error: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TagBatchReport {
    pub updated_ids: Vec<i64>,
    pub failures: Vec<TagWriteFailure>,
}

pub(crate) fn validate_registered_track(
    conn: &Connection,
    id: i64,
    path: &Path,
) -> Result<(), String> {
    super::tag_mutation::validate_registered_track(conn, id, path)
}

#[cfg(test)]
fn prepare_tag_reconciliation(conn: &Connection, id: i64, path: &Path) -> Result<(), String> {
    super::tag_mutation::prepare_reconciliation(conn, id, path)
}

/// Like `apply_patch_batch`, but calls `watcher::ignore_path` on each file
/// immediately before *that file's own* write — not upfront for the whole
/// batch, so an early file's ignore window can't expire while later files
/// are still being processed (see `tag_edit_write`'s module doc).
pub fn apply_patch_batch_ignored(
    db: &Db,
    tracks: &[(i64, PathBuf)],
    patch: &TagPatch,
) -> TagBatchReport {
    let conn = db.conn();
    apply_patch_batch_inner(conn, tracks, patch, true)
}

/// Like `apply_track_edit_batch`, but with watcher-ignore support (see
/// `apply_patch_batch_ignored`'s doc comment for the per-file timing).
pub fn apply_track_edit_batch_ignored(
    db: &Db,
    tracks: &[(i64, PathBuf)],
    patch: &TrackEditPatch,
) -> TagBatchReport {
    let conn = db.conn();
    apply_track_edit_batch_inner(conn, tracks, patch, true)
}

pub fn apply_patch_batch(db: &Db, tracks: &[(i64, PathBuf)], patch: &TagPatch) -> TagBatchReport {
    let conn = db.conn();
    apply_patch_batch_inner(conn, tracks, patch, false)
}

fn apply_patch_batch_inner(
    conn: &Connection,
    tracks: &[(i64, PathBuf)],
    patch: &TagPatch,
    ignore_watcher: bool,
) -> TagBatchReport {
    let mut report = TagBatchReport::default();
    if patch.is_empty() {
        return report;
    }

    for (id, path) in tracks {
        let prepared = match super::tag_mutation::prepare_tag_mutation(conn, *id, path, patch) {
            Ok(Some(prepared)) => prepared,
            Ok(None) => continue,
            Err(failure) => {
                let (kind, error, _) = failure.into_parts();
                report.failures.push(TagWriteFailure {
                    id: *id,
                    path: path.clone(),
                    kind,
                    error,
                });
                continue;
            }
        };
        match super::tag_mutation::commit_tag_mutation(conn, &prepared, ignore_watcher) {
            Ok(()) => report.updated_ids.push(*id),
            Err(failure) => {
                let (kind, error, _) = failure.into_parts();
                report.failures.push(TagWriteFailure {
                    id: *id,
                    path: path.clone(),
                    kind,
                    error,
                });
            }
        }
    }
    report
}

pub fn apply_track_edit_batch(
    db: &Db,
    tracks: &[(i64, PathBuf)],
    patch: &TrackEditPatch,
) -> TagBatchReport {
    let conn = db.conn();
    apply_track_edit_batch_inner(conn, tracks, patch, false)
}

fn apply_track_edit_batch_inner(
    conn: &Connection,
    tracks: &[(i64, PathBuf)],
    patch: &TrackEditPatch,
    ignore_watcher: bool,
) -> TagBatchReport {
    if patch.is_empty() {
        return TagBatchReport::default();
    }

    let mut report = if patch.tags.is_empty() {
        let mut report = TagBatchReport::default();
        for (id, path) in tracks {
            if let Err(error) = validate_registered_track(conn, *id, path) {
                report.failures.push(TagWriteFailure {
                    id: *id,
                    path: path.clone(),
                    kind: WriteErrorKind::Io,
                    error,
                });
                continue;
            }
            report.updated_ids.push(*id);
        }
        report
    } else {
        apply_patch_batch_inner(conn, tracks, &patch.tags, ignore_watcher)
    };

    let Some(rating) = patch.rating else {
        return report;
    };
    let eligible_ids = tracks
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| !report.failures.iter().any(|failure| failure.id == *id))
        .collect::<Vec<_>>();
    for id in eligible_ids {
        match crate::library::stats::set_rating_in(conn, id, rating) {
            Ok(()) => {
                if !report.updated_ids.contains(&id) {
                    report.updated_ids.push(id);
                }
            }
            Err(error) => {
                report.updated_ids.retain(|updated| *updated != id);
                let path = tracks
                    .iter()
                    .find(|(track_id, _)| *track_id == id)
                    .map(|(_, path)| path.clone())
                    .unwrap_or_default();
                report.failures.push(TagWriteFailure {
                    id,
                    path,
                    kind: WriteErrorKind::Io,
                    error: format!("could not save rating: {error}"),
                });
            }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    use lofty::picture::{MimeType, Picture, PictureType};
    use lofty::tag::ItemKey;

    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn fixture_copy(dir: &Path, name: &str) -> PathBuf {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
        let destination = dir.join(name);
        std::fs::copy(source, &destination).unwrap();
        destination
    }

    fn seed_full_tag(path: &Path) {
        let mut tagged = lofty::read_from_path(path).unwrap();
        let tag = tagged.primary_tag_mut().unwrap();
        tag.set_title("Old title".into());
        tag.set_artist("Keep artist".into());
        tag.set_album("Keep album".into());
        tag.insert_text(ItemKey::AlbumArtist, "Keep album artist".into());
        tag.set_date(lofty::tag::items::Timestamp {
            year: 1999,
            ..lofty::tag::items::Timestamp::default()
        });
        tag.set_track(7);
        tag.set_genre("Keep genre".into());
        tag.insert_text(ItemKey::Comment, "Keep comment".into());
        tag.push_picture(
            Picture::unchecked(TINY_PNG.to_vec())
                .pic_type(PictureType::CoverFront)
                .mime_type(MimeType::Png)
                .build(),
        );
        tag.save_to_path(path, lofty::config::WriteOptions::default())
            .unwrap();
    }

    fn tags(title: &str, artist: &str) -> EditableTags {
        EditableTags {
            title: title.into(),
            artist: artist.into(),
            album: "Shared album".into(),
            album_artist: "Shared album artist".into(),
            year: Some(2026),
            track_no: Some(1),
            genre: "Rock".into(),
        }
    }

    #[test]
    fn summary_marks_only_differing_fields_mixed() {
        let summary = summarize(&[tags("First", "Artist"), tags("Second", "Artist")]).unwrap();
        assert_eq!(summary.title, MixedValue::Mixed);
        assert_eq!(summary.artist, MixedValue::Uniform("Artist".to_string()));
        assert_eq!(summary.year, MixedValue::Uniform(Some(2026)));
    }

    #[test]
    fn empty_selection_has_no_summary() {
        assert!(summarize(&[]).is_none());
    }

    #[test]
    fn untouched_patch_is_empty_but_clear_is_not() {
        assert!(TagPatch::default().is_empty());
        let patch = TagPatch {
            year: Some(None),
            ..TagPatch::default()
        };
        assert!(!patch.is_empty());
    }

    #[test]
    fn patch_changes_only_dirty_fields_and_preserves_picture_and_custom_item() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture_copy(dir.path(), "selective.flac");
        seed_full_tag(&path);

        apply_patch_to_file(
            &path,
            &TagPatch {
                title: Some("New title".into()),
                ..TagPatch::default()
            },
        )
        .unwrap();

        let tagged = lofty::read_from_path(&path).unwrap();
        let tag = tagged.primary_tag().unwrap();
        assert_eq!(tag.title().as_deref(), Some("New title"));
        assert_eq!(tag.artist().as_deref(), Some("Keep artist"));
        assert_eq!(tag.album().as_deref(), Some("Keep album"));
        assert_eq!(tag.date().map(|date| date.year), Some(1999));
        assert_eq!(tag.track(), Some(7));
        assert_eq!(tag.genre().as_deref(), Some("Keep genre"));
        assert_eq!(tag.get_string(ItemKey::Comment), Some("Keep comment"));
        assert_eq!(tag.pictures().len(), 1);
        assert_eq!(tag.pictures()[0].data(), TINY_PNG);
    }

    #[test]
    fn numeric_patch_can_set_and_clear_year_and_track() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture_copy(dir.path(), "numbers.flac");
        seed_full_tag(&path);
        apply_patch_to_file(
            &path,
            &TagPatch {
                year: Some(Some(2026)),
                track_no: Some(None),
                ..TagPatch::default()
            },
        )
        .unwrap();
        let tags = read_editable_tags(&path).unwrap();
        assert_eq!(tags.year, Some(2026));
        assert_eq!(tags.track_no, None);
    }

    #[test]
    fn empty_patch_leaves_file_bytes_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture_copy(dir.path(), "unchanged.flac");
        seed_full_tag(&path);
        let before = std::fs::read(&path).unwrap();
        apply_patch_to_file(&path, &TagPatch::default()).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), before);
    }

    #[test]
    fn batch_continues_after_failure_and_reconciles_db_without_losing_stats() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture_copy(dir.path(), "batch.flac");
        seed_full_tag(&path);
        let conn = crate::db::Db::open_in_memory().unwrap();
        crate::library::scanner::scan_folder(&conn, &path).unwrap();
        let path_text = path.to_string_lossy().to_string();
        let id: i64 = conn
            .conn()
            .query_row("SELECT id FROM tracks WHERE path=?1", [&path_text], |row| {
                row.get(0)
            })
            .unwrap();
        conn.conn()
            .execute("UPDATE tracks SET rating=4, play_count=9 WHERE id=?1", [id])
            .unwrap();

        let missing = dir.path().join("missing.flac");
        let report = apply_patch_batch(
            &conn,
            &[(id, path.clone()), (999, missing)],
            &TagPatch {
                title: Some("Batch title".into()),
                ..TagPatch::default()
            },
        );

        assert_eq!(report.updated_ids, vec![id]);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].id, 999);
        let row: (String, i32, i64) = conn
            .conn()
            .query_row(
                "SELECT title, rating, play_count FROM tracks WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, ("Batch title".into(), 4, 9));
    }

    #[test]
    fn batch_refuses_a_stale_id_path_pair_before_touching_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let registered = fixture_copy(dir.path(), "registered.flac");
        let other = fixture_copy(dir.path(), "other.flac");
        seed_full_tag(&registered);
        seed_full_tag(&other);
        let conn = crate::db::Db::open_in_memory().unwrap();
        crate::library::scanner::scan_folder(&conn, &registered).unwrap();
        let registered_text = registered.to_string_lossy().to_string();
        let id: i64 = conn
            .conn()
            .query_row(
                "SELECT id FROM tracks WHERE path=?1",
                [&registered_text],
                |row| row.get(0),
            )
            .unwrap();
        let before = std::fs::read(&other).unwrap();

        let report = apply_patch_batch(
            &conn,
            &[(id, other.clone())],
            &TagPatch {
                title: Some("Must not be written".into()),
                ..TagPatch::default()
            },
        );

        assert!(report.updated_ids.is_empty());
        assert_eq!(report.failures.len(), 1);
        assert_eq!(std::fs::read(other).unwrap(), before);
    }

    #[test]
    fn rating_summary_preserves_uniform_and_mixed_selections() {
        assert_eq!(summarize_values(&[4, 4]), Some(MixedValue::Uniform(4)));
        assert_eq!(summarize_values(&[4, 0]), Some(MixedValue::Mixed));
        assert_eq!(summarize_values::<i32>(&[]), None);
    }

    #[test]
    fn rating_only_edit_updates_the_database_without_touching_file_tags() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture_copy(dir.path(), "rating-only.flac");
        seed_full_tag(&path);
        let before = std::fs::read(&path).unwrap();
        let conn = crate::db::Db::open_in_memory().unwrap();
        crate::library::scanner::scan_folder(&conn, &path).unwrap();
        let path_text = path.to_string_lossy().to_string();
        let id: i64 = conn
            .conn()
            .query_row("SELECT id FROM tracks WHERE path=?1", [&path_text], |row| {
                row.get(0)
            })
            .unwrap();

        let report = apply_track_edit_batch(
            &conn,
            &[(id, path.clone())],
            &TrackEditPatch {
                rating: Some(5),
                ..TrackEditPatch::default()
            },
        );

        assert_eq!(report.updated_ids, vec![id]);
        assert!(report.failures.is_empty());
        let rating: i32 = conn
            .conn()
            .query_row("SELECT rating FROM tracks WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(rating, 5);
        assert_eq!(std::fs::read(path).unwrap(), before);
    }

    #[test]
    fn combined_tag_and_rating_edit_reconciles_both_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture_copy(dir.path(), "tag-and-rating.flac");
        seed_full_tag(&path);
        let conn = crate::db::Db::open_in_memory().unwrap();
        crate::library::scanner::scan_folder(&conn, &path).unwrap();
        let path_text = path.to_string_lossy().to_string();
        let id: i64 = conn
            .conn()
            .query_row("SELECT id FROM tracks WHERE path=?1", [&path_text], |row| {
                row.get(0)
            })
            .unwrap();

        let report = apply_track_edit_batch(
            &conn,
            &[(id, path.clone())],
            &TrackEditPatch {
                tags: TagPatch {
                    title: Some("New title and rating".into()),
                    ..TagPatch::default()
                },
                rating: Some(3),
            },
        );

        assert_eq!(report.updated_ids, vec![id]);
        assert!(report.failures.is_empty());
        let row: (String, i32) = conn
            .conn()
            .query_row(
                "SELECT title, rating FROM tracks WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(row, ("New title and rating".into(), 3));
        assert_eq!(
            read_editable_tags(&path).unwrap().title,
            "New title and rating"
        );
    }

    #[test]
    fn combined_edit_applies_rating_when_requested_tag_is_an_exact_noop() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture_copy(dir.path(), "noop-tag-and-rating.flac");
        seed_full_tag(&path);
        let before = std::fs::read(&path).unwrap();
        let conn = crate::db::Db::open_in_memory().unwrap();
        crate::library::scanner::scan_folder(&conn, &path).unwrap();
        let path_text = path.to_string_lossy().to_string();
        let id = conn
            .conn()
            .query_row("SELECT id FROM tracks WHERE path=?1", [&path_text], |row| {
                row.get(0)
            })
            .unwrap();

        let report = apply_track_edit_batch(
            &conn,
            &[(id, path.clone())],
            &TrackEditPatch {
                tags: TagPatch {
                    title: Some("Old title".into()),
                    ..TagPatch::default()
                },
                rating: Some(2),
            },
        );

        assert_eq!(report.updated_ids, vec![id]);
        assert!(report.failures.is_empty());
        assert_eq!(std::fs::read(path).unwrap(), before);
        let rating: i32 = conn
            .conn()
            .query_row("SELECT rating FROM tracks WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(rating, 2);
    }

    #[test]
    fn untouched_track_edit_patch_is_empty_but_rating_zero_is_not() {
        assert!(TrackEditPatch::default().is_empty());
        assert!(!TrackEditPatch {
            rating: Some(0),
            ..TrackEditPatch::default()
        }
        .is_empty());
    }
}

#[cfg(test)]
#[path = "tag_edit_reread_tests.rs"]
mod reread_tests;
