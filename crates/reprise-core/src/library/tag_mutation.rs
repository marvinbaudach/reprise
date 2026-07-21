//! The single primitive for a validated tag-file mutation.
//!
//! Preparing is read-only: it validates the registered `(id, path)`, reads
//! the actual file tags, and narrows the request to an exact effective patch.
//! Committing revalidates the identity, performs the one Lofty save, and then
//! reconciles the same `(id, path)` back into the database. Keeping the seam
//! explicit lets journaled callers persist `before` before any file changes.

use std::path::{Path, PathBuf};

use lofty::file::TaggedFile;
use lofty::prelude::*;
use lofty::tag::items::Timestamp;
use lofty::tag::{ItemKey, Tag};
use rusqlite::{Connection, OptionalExtension};

use crate::models::ImportErrorKind;

use super::scanner::ScanOutcome;
use super::tag_edit::{read_editable_tags, EditableTags, TagEditError, TagPatch};

pub(crate) use super::tag_mutation_guarded::{
    commit_guarded_tag_changes, read_tag_field_values, GuardedTagChange, GuardedTagField,
};

/// Duration to suppress watcher events for each written file.
pub(crate) const IGNORE_DURATION: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteErrorKind {
    PermissionDenied,
    NotFound,
    UnsupportedFormat,
    UnreadableTags,
    Io,
}

impl WriteErrorKind {
    pub fn user_message(self) -> &'static str {
        match self {
            Self::PermissionDenied => "No write permission",
            Self::NotFound => "File not found",
            Self::UnsupportedFormat => "Unsupported audio format",
            Self::UnreadableTags => "File's tags could not be read",
            Self::Io => "Could not write tags",
        }
    }

    fn from_import_kind(kind: ImportErrorKind) -> Self {
        match kind {
            ImportErrorKind::PermissionDenied => Self::PermissionDenied,
            ImportErrorKind::UnsupportedFormat => Self::UnsupportedFormat,
            ImportErrorKind::UnreadableTags => Self::UnreadableTags,
            ImportErrorKind::Io | ImportErrorKind::Unknown => Self::Io,
        }
    }
}

pub fn classify_write_error(error: &TagEditError) -> WriteErrorKind {
    use lofty::error::ErrorKind as LoftyErrorKind;

    match error {
        TagEditError::NoWritableTag => WriteErrorKind::UnsupportedFormat,
        TagEditError::Lofty(lofty_error) => match lofty_error.kind() {
            LoftyErrorKind::Io(io_error) if io_error.kind() == std::io::ErrorKind::NotFound => {
                WriteErrorKind::NotFound
            }
            LoftyErrorKind::UnsupportedTag => WriteErrorKind::UnsupportedFormat,
            LoftyErrorKind::TooMuchData => WriteErrorKind::Io,
            _ => {
                let (kind, _) = crate::library::import_errors::classify_lofty(lofty_error);
                WriteErrorKind::from_import_kind(kind)
            }
        },
    }
}

#[derive(Debug)]
pub(crate) struct TagMutationFailure {
    pub(crate) kind: WriteErrorKind,
    pub(crate) error: String,
    pub(crate) file_written: bool,
}

impl TagMutationFailure {
    pub(crate) fn into_parts(self) -> (WriteErrorKind, String, bool) {
        (self.kind, self.error, self.file_written)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedTagMutation {
    pub(crate) id: i64,
    pub(crate) path: PathBuf,
    pub(crate) before: EditableTags,
    pub(crate) patch: TagPatch,
    /// The strict reader could not parse this file's tag container at all (a
    /// damaged ID3v2/APE/ID3v1 the scanner only imported via its relaxed,
    /// tag-free pass). Commit strips every container and writes a fresh ID3v2
    /// instead of editing an existing tag in place.
    pub(crate) strip_and_rewrite: bool,
}

pub(crate) fn effective_tag_patch(current: &EditableTags, patch: &TagPatch) -> TagPatch {
    TagPatch {
        title: patch.title.clone().filter(|value| *value != current.title),
        artist: patch
            .artist
            .clone()
            .filter(|value| *value != current.artist),
        album: patch.album.clone().filter(|value| *value != current.album),
        album_artist: patch
            .album_artist
            .clone()
            .filter(|value| *value != current.album_artist),
        year: patch.year.filter(|value| *value != current.year),
        track_no: patch.track_no.filter(|value| *value != current.track_no),
        genre: patch.genre.clone().filter(|value| *value != current.genre),
    }
}

pub(crate) fn validate_registered_track(
    conn: &Connection,
    id: i64,
    path: &Path,
) -> Result<(), String> {
    let registered_path = conn
        .query_row(
            "SELECT path FROM tracks WHERE id=?1 AND removed_at IS NULL",
            [id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("could not validate track path before edit: {error}"))?;
    match registered_path {
        Some(registered) if registered == path.to_string_lossy() => Ok(()),
        _ => Err("track path changed before edit; refusing stale request".into()),
    }
}

pub(crate) fn prepare_tag_mutation(
    conn: &Connection,
    id: i64,
    path: &Path,
    requested: &TagPatch,
) -> Result<Option<PreparedTagMutation>, TagMutationFailure> {
    validate_registered_track(conn, id, path).map_err(|error| TagMutationFailure {
        kind: WriteErrorKind::Io,
        error,
        file_written: false,
    })?;
    if requested
        .year
        .flatten()
        .is_some_and(|year| year > u32::from(u16::MAX))
    {
        return Err(TagMutationFailure {
            kind: WriteErrorKind::Io,
            error: "year is outside the tag format's supported range".into(),
            file_written: false,
        });
    }
    let (before, strip_and_rewrite) = match read_editable_tags(path) {
        Ok(tags) => (tags, false),
        // A container the strict reader rejects (the scanner only imported it
        // through its relaxed, tag-free pass) is repaired rather than refused:
        // treat the current tags as empty so every requested field is written,
        // and flag the commit to strip the damaged containers first.
        Err(error) if classify_write_error(&error) == WriteErrorKind::UnreadableTags => {
            (EditableTags::default(), true)
        }
        Err(error) => {
            return Err(TagMutationFailure {
                kind: classify_write_error(&error),
                error: error.to_string(),
                file_written: false,
            });
        }
    };
    let patch = effective_tag_patch(&before, requested);
    if patch.is_empty() {
        return Ok(None);
    }
    Ok(Some(PreparedTagMutation {
        id,
        path: path.to_path_buf(),
        before,
        patch,
        strip_and_rewrite,
    }))
}

/// The sole production Lofty tag-save path.
pub(crate) fn apply_tag_patch_to_file(path: &Path, patch: &TagPatch) -> Result<(), TagEditError> {
    if patch.is_empty() {
        return Ok(());
    }

    let mut tagged = lofty::read_from_path(path)?;
    apply_tag_patch_to_tagged(&mut tagged, path, patch)
}

fn apply_tag_patch_to_tagged(
    tagged: &mut TaggedFile,
    path: &Path,
    patch: &TagPatch,
) -> Result<(), TagEditError> {
    if tagged.primary_tag().is_none() && tagged.first_tag().is_none() {
        tagged.insert_tag(Tag::new(tagged.primary_tag_type()));
    }
    let tag = if tagged.primary_tag().is_some() {
        tagged.primary_tag_mut()
    } else {
        tagged.first_tag_mut()
    }
    .ok_or(TagEditError::NoWritableTag)?;

    set_patch_fields(tag, patch);

    save_loaded_tagged(tagged, path)
}

/// Applies a narrowed [`TagPatch`] onto a single tag — sets non-empty values,
/// removes empty ones. Shared by the in-place edit and the strip-and-rewrite
/// repair path so both write identical fields.
fn set_patch_fields(tag: &mut Tag, patch: &TagPatch) {
    if let Some(value) = &patch.title {
        if value.is_empty() {
            tag.remove_title();
        } else {
            tag.set_title(value.clone());
        }
    }
    if let Some(value) = &patch.artist {
        if value.is_empty() {
            tag.remove_artist();
        } else {
            tag.set_artist(value.clone());
        }
    }
    if let Some(value) = &patch.album {
        if value.is_empty() {
            tag.remove_album();
        } else {
            tag.set_album(value.clone());
        }
    }
    if let Some(value) = &patch.album_artist {
        if value.is_empty() {
            tag.remove_key(ItemKey::AlbumArtist);
        } else {
            tag.insert_text(ItemKey::AlbumArtist, value.clone());
        }
    }
    if let Some(value) = patch.year {
        match value {
            Some(year) => tag.set_date(Timestamp {
                year: u16::try_from(year).unwrap_or(u16::MAX),
                ..Timestamp::default()
            }),
            None => tag.remove_date(),
        }
    }
    if let Some(value) = patch.track_no {
        match value {
            Some(track_no) => tag.set_track(track_no),
            None => tag.remove_track(),
        }
    }
    if let Some(value) = &patch.genre {
        if value.is_empty() {
            tag.remove_genre();
        } else {
            tag.set_genre(value.clone());
        }
    }
}

/// Repair path for files the strict reader can't parse: byte-strip every known
/// tag container, then write a fresh ID3v2 carrying only the requested fields.
/// This is the sole way to make a file with a damaged APE/ID3 container
/// editable again — `TagType::remove_from_path` parses the tag before removing
/// it and so can't clear the very container that fails to parse.
pub(super) fn strip_and_rewrite_tag(path: &Path, patch: &TagPatch) -> Result<(), TagEditError> {
    let data = std::fs::read(path).map_err(lofty::error::LoftyError::from)?;
    std::fs::write(path, strip_tag_containers(data)).map_err(lofty::error::LoftyError::from)?;
    // The file is now strictly readable and tag-free; route through the single
    // loaded-container save seam, which inserts a fresh primary (ID3v2) tag.
    let mut tagged = lofty::read_from_path(path)?;
    apply_tag_patch_to_tagged(&mut tagged, path, patch)
}

/// Removes an ID3v2 header (front), and a trailing ID3v1 and APEv2 container
/// by their size headers, without parsing their (possibly damaged) contents.
/// A container whose header is absent or self-inconsistent is left untouched,
/// so an intact audio stream is never truncated.
fn strip_tag_containers(mut data: Vec<u8>) -> Vec<u8> {
    // ID3v2 at the front: "ID3" + version(2) + flags(1) + synchsafe size(4).
    if data.len() >= 10 && &data[0..3] == b"ID3" {
        let size = ((data[6] as usize & 0x7f) << 21)
            | ((data[7] as usize & 0x7f) << 14)
            | ((data[8] as usize & 0x7f) << 7)
            | (data[9] as usize & 0x7f);
        let total = 10 + size;
        if total <= data.len() {
            data.drain(0..total);
        }
    }
    // ID3v1 at the very end: 128 bytes starting with "TAG".
    if data.len() >= 128 && &data[data.len() - 128..data.len() - 125] == b"TAG" {
        data.truncate(data.len() - 128);
    }
    // APEv2 footer at the end: "APETAGEX", 32 bytes from the end.
    if data.len() >= 32 && &data[data.len() - 32..data.len() - 24] == b"APETAGEX" {
        let footer = data.len() - 32;
        let tag_size = u32::from_le_bytes([
            data[footer + 12],
            data[footer + 13],
            data[footer + 14],
            data[footer + 15],
        ]) as usize;
        let flags = u32::from_le_bytes([
            data[footer + 20],
            data[footer + 21],
            data[footer + 22],
            data[footer + 23],
        ]);
        let has_header = flags & (1 << 31) != 0;
        let mut start = (footer + 32).saturating_sub(tag_size);
        if has_header {
            start = start.saturating_sub(32);
        }
        if start <= footer {
            data.truncate(start);
        }
    }
    data
}

pub(super) fn save_loaded_tagged(tagged: &TaggedFile, path: &Path) -> Result<(), TagEditError> {
    tagged
        .primary_tag()
        .or_else(|| tagged.first_tag())
        .ok_or(TagEditError::NoWritableTag)?
        .save_to_path(path, lofty::config::WriteOptions::default())?;
    Ok(())
}

fn editable_tags_from_tagged(tagged: &TaggedFile) -> EditableTags {
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    EditableTags {
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
    }
}

fn affected_fields_still_match(prepared: &PreparedTagMutation, current: &EditableTags) -> bool {
    let patch = &prepared.patch;
    let before = &prepared.before;
    (patch.title.is_none() || current.title == before.title)
        && (patch.artist.is_none() || current.artist == before.artist)
        && (patch.album.is_none() || current.album == before.album)
        && (patch.album_artist.is_none() || current.album_artist == before.album_artist)
        && (patch.year.is_none() || current.year == before.year)
        && (patch.track_no.is_none() || current.track_no == before.track_no)
        && (patch.genre.is_none() || current.genre == before.genre)
}

pub(crate) fn prepare_reconciliation(
    conn: &Connection,
    id: i64,
    path: &Path,
) -> Result<(), String> {
    let changed = conn
        .execute(
            "UPDATE tracks SET file_mtime=-1 \
             WHERE id=?1 AND path=?2 AND removed_at IS NULL",
            rusqlite::params![id, path.to_string_lossy()],
        )
        .map_err(|error| format!("could not prepare tag reconciliation: {error}"))?;
    (changed == 1)
        .then_some(())
        .ok_or_else(|| "track path changed before tag reconciliation; refusing stale write".into())
}

pub(super) fn reconcile_after_write(
    conn: &mut Connection,
    id: i64,
    path: &Path,
) -> Result<(), String> {
    prepare_reconciliation(conn, id, path)?;
    match crate::library::scanner::scan_folder(conn, path) {
        Ok(ScanOutcome::Completed(scan)) if scan.errors == 0 => Ok(()),
        Ok(ScanOutcome::Completed(scan)) => Err(format!(
            "tag reconciliation reported {} error(s)",
            scan.errors
        )),
        Ok(ScanOutcome::RootUnavailable { root }) => Err(format!(
            "tag reconciliation failed: library folder unavailable: {}",
            root.display()
        )),
        Err(error) => Err(format!("tag reconciliation failed: {error}")),
    }
}

pub(crate) fn commit_tag_mutation(
    conn: &mut Connection,
    prepared: &PreparedTagMutation,
    ignore_watcher: bool,
) -> Result<(), TagMutationFailure> {
    validate_registered_track(conn, prepared.id, &prepared.path).map_err(|error| {
        TagMutationFailure {
            kind: WriteErrorKind::Io,
            error,
            file_written: false,
        }
    })?;
    if prepared.strip_and_rewrite {
        if ignore_watcher {
            super::watcher::ignore_path(&prepared.path, IGNORE_DURATION);
        }
        strip_and_rewrite_tag(&prepared.path, &prepared.patch).map_err(|error| {
            TagMutationFailure {
                kind: classify_write_error(&error),
                error: error.to_string(),
                file_written: true,
            }
        })?;
        return reconcile_after_write(conn, prepared.id, &prepared.path).map_err(|error| {
            TagMutationFailure {
                kind: WriteErrorKind::Io,
                error,
                file_written: true,
            }
        });
    }
    let mut tagged = lofty::read_from_path(&prepared.path).map_err(|error| {
        let error = TagEditError::from(error);
        TagMutationFailure {
            kind: classify_write_error(&error),
            error: error.to_string(),
            file_written: false,
        }
    })?;
    if !affected_fields_still_match(prepared, &editable_tags_from_tagged(&tagged)) {
        return Err(TagMutationFailure {
            kind: WriteErrorKind::Io,
            error: "tags changed after the mutation was prepared; refusing stale write".into(),
            file_written: false,
        });
    }
    if ignore_watcher {
        super::watcher::ignore_path(&prepared.path, IGNORE_DURATION);
    }
    apply_tag_patch_to_tagged(&mut tagged, &prepared.path, &prepared.patch).map_err(|error| {
        TagMutationFailure {
            kind: classify_write_error(&error),
            error: error.to_string(),
            file_written: false,
        }
    })?;
    reconcile_after_write(conn, prepared.id, &prepared.path).map_err(|error| TagMutationFailure {
        kind: WriteErrorKind::Io,
        error,
        file_written: true,
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use lofty::prelude::*;
    use rusqlite::Connection;

    use super::*;
    use crate::library::tag_edit::{read_editable_tags, TagPatch};

    fn fixture_copy(dir: &Path, name: &str) -> PathBuf {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
        let destination = dir.join(name);
        std::fs::copy(source, &destination).unwrap();
        destination
    }

    fn seeded_track(dir: &Path, name: &str) -> (Connection, i64, PathBuf) {
        let path = fixture_copy(dir, name);
        let mut tagged = lofty::read_from_path(&path).unwrap();
        tagged
            .primary_tag_mut()
            .unwrap()
            .set_title("Original title".into());
        tagged
            .primary_tag()
            .unwrap()
            .save_to_path(&path, lofty::config::WriteOptions::default())
            .unwrap();

        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        crate::library::scanner::scan_folder(&mut conn, &path).unwrap();
        let path_text = path.to_string_lossy().to_string();
        let id = conn
            .query_row("SELECT id FROM tracks WHERE path=?1", [&path_text], |row| {
                row.get(0)
            })
            .unwrap();
        (conn, id, path)
    }

    #[test]
    fn shared_tag_mutation_skips_exact_noop_without_touching_file() {
        let dir = tempfile::tempdir().unwrap();
        let (conn, id, path) = seeded_track(dir.path(), "noop.flac");
        let before = std::fs::read(&path).unwrap();

        let prepared = prepare_tag_mutation(
            &conn,
            id,
            &path,
            &TagPatch {
                title: Some("Original title".into()),
                ..TagPatch::default()
            },
        )
        .unwrap();

        assert!(prepared.is_none());
        assert_eq!(std::fs::read(path).unwrap(), before);
    }

    #[test]
    fn prepared_mutation_captures_actual_before_and_effective_after() {
        let dir = tempfile::tempdir().unwrap();
        let (conn, id, path) = seeded_track(dir.path(), "capture.flac");

        let prepared = prepare_tag_mutation(
            &conn,
            id,
            &path,
            &TagPatch {
                title: Some("New title".into()),
                artist: Some(String::new()),
                ..TagPatch::default()
            },
        )
        .unwrap()
        .unwrap();

        assert_eq!(prepared.before.title, "Original title");
        assert_eq!(prepared.patch.title.as_deref(), Some("New title"));
        assert!(prepared.patch.artist.is_none());
    }

    #[test]
    fn shared_tag_mutation_rejects_stale_id_path_before_touching_file() {
        let dir = tempfile::tempdir().unwrap();
        let (conn, id, _registered) = seeded_track(dir.path(), "registered.flac");
        let other = fixture_copy(dir.path(), "other.flac");
        let before = std::fs::read(&other).unwrap();

        let error = prepare_tag_mutation(
            &conn,
            id,
            &other,
            &TagPatch {
                title: Some("Must not be written".into()),
                ..TagPatch::default()
            },
        )
        .unwrap_err();

        assert_eq!(error.kind, WriteErrorKind::Io);
        assert_eq!(std::fs::read(other).unwrap(), before);
    }

    #[test]
    fn commit_reconciles_using_id_and_path() {
        let dir = tempfile::tempdir().unwrap();
        let (mut conn, id, path) = seeded_track(dir.path(), "moved.flac");
        let prepared = prepare_tag_mutation(
            &conn,
            id,
            &path,
            &TagPatch {
                title: Some("New title".into()),
                ..TagPatch::default()
            },
        )
        .unwrap()
        .unwrap();
        let replacement = dir.path().join("replacement.flac");
        conn.execute(
            "UPDATE tracks SET path=?1, file_mtime=123 WHERE id=?2",
            rusqlite::params![replacement.to_string_lossy(), id],
        )
        .unwrap();

        let error = commit_tag_mutation(&mut conn, &prepared, false).unwrap_err();

        assert_eq!(error.kind, WriteErrorKind::Io);
        assert_eq!(read_editable_tags(&path).unwrap().title, "Original title");
        let file_mtime: i64 = conn
            .query_row("SELECT file_mtime FROM tracks WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(file_mtime, 123);
        assert!(!error.file_written);
    }

    #[test]
    fn commit_rejects_an_affected_field_changed_after_prepare() {
        let dir = tempfile::tempdir().unwrap();
        let (mut conn, id, path) = seeded_track(dir.path(), "external-change.flac");
        let prepared = prepare_tag_mutation(
            &conn,
            id,
            &path,
            &TagPatch {
                title: Some("Doctor title".into()),
                ..TagPatch::default()
            },
        )
        .unwrap()
        .unwrap();
        let mut tagged = lofty::read_from_path(&path).unwrap();
        tagged
            .primary_tag_mut()
            .unwrap()
            .set_title("External title".into());
        tagged
            .primary_tag()
            .unwrap()
            .save_to_path(&path, lofty::config::WriteOptions::default())
            .unwrap();

        let error = commit_tag_mutation(&mut conn, &prepared, false).unwrap_err();

        assert!(!error.file_written);
        assert_eq!(read_editable_tags(&path).unwrap().title, "External title");
    }

    #[test]
    fn reconciliation_failure_reports_that_the_file_was_written() {
        let dir = tempfile::tempdir().unwrap();
        let (mut conn, id, path) = seeded_track(dir.path(), "reconcile-failure.flac");
        let prepared = prepare_tag_mutation(
            &conn,
            id,
            &path,
            &TagPatch {
                title: Some("Written title".into()),
                ..TagPatch::default()
            },
        )
        .unwrap()
        .unwrap();
        conn.execute_batch(
            "CREATE TRIGGER reject_tag_reconcile
             BEFORE UPDATE OF file_mtime ON tracks
             WHEN NEW.file_mtime = -1
             BEGIN
               SELECT RAISE(FAIL, 'injected reconcile failure');
             END;",
        )
        .unwrap();

        let error = commit_tag_mutation(&mut conn, &prepared, false).unwrap_err();

        assert!(error.file_written);
        assert_eq!(read_editable_tags(&path).unwrap().title, "Written title");
    }
}
