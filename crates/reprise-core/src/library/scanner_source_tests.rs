//! `scanner.rs`'s source-neutrality suite: the tests that drive a real scan
//! through a `LibrarySource` the scanner has never seen. Its own file for the
//! project's 800-line reason — `scanner.rs` declares it via `#[cfg(test)]
//! #[path = "scanner_source_tests.rs"] mod source_tests;`.
//!
//! `scanner_tests.rs` proves what the scanner does; this file proves it does
//! it without knowing where the tree came from.

use super::tests::{completed, fixture_copy, tag_file};
use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

/// A library source that produces its tree from a hand-built list instead of
/// walking a filesystem — no `walkdir`, no `read_dir`, no directory order to
/// depend on. Residence classification still delegates to the Unix source,
/// because package 2 abstracted traversal and nothing else; the paths it hands
/// out are real files in a temp dir, so the scanner's own reads still work.
///
/// This is what makes the traversal seam more than a rename: the two tests
/// below drive `scan_folder_inner`'s entire body — the audio filter, the
/// per-entry upsert, the import-error path — through a source the scanner has
/// never seen, rather than through `UnixLibrarySource` in disguise.
struct ScriptedSource {
    items: Vec<super::source::LibraryWalkItem>,
    contents: HashMap<std::path::PathBuf, Vec<u8>>,
    open_counts: Mutex<HashMap<std::path::PathBuf, usize>>,
    probe_counts: Mutex<HashMap<std::path::PathBuf, usize>>,
}

impl ScriptedSource {
    fn new(items: Vec<super::source::LibraryWalkItem>) -> Self {
        Self {
            items,
            contents: HashMap::new(),
            open_counts: Mutex::new(HashMap::new()),
            probe_counts: Mutex::new(HashMap::new()),
        }
    }

    fn with_content(mut self, path: std::path::PathBuf, bytes: Vec<u8>) -> Self {
        self.contents.insert(path, bytes);
        self
    }

    fn open_count(&self, path: &std::path::Path) -> usize {
        self.open_counts
            .lock()
            .unwrap()
            .get(path)
            .copied()
            .unwrap_or_default()
    }

    fn probe_count(&self, path: &std::path::Path) -> usize {
        self.probe_counts
            .lock()
            .unwrap()
            .get(path)
            .copied()
            .unwrap_or_default()
    }
}

impl super::source::LibrarySource for ScriptedSource {
    fn residence_token(&self, at: &std::path::Path) -> Option<i64> {
        super::source::UnixLibrarySource.residence_token(at)
    }

    fn open_read(&self, at: &std::path::Path) -> std::io::Result<super::source::LibraryReadHandle> {
        *self
            .open_counts
            .lock()
            .unwrap()
            .entry(at.to_path_buf())
            .or_default() += 1;
        match self.contents.get(at) {
            Some(bytes) => Ok(super::source::LibraryReadHandle::new(std::io::Cursor::new(
                bytes.clone(),
            ))),
            None => super::source::UnixLibrarySource.open_read(at),
        }
    }

    /// The scanner never lists a directory, so this double is never asked.
    /// Spelled out rather than inherited: the trait has no defaults, so a
    /// source cannot answer "nothing here" to a question nobody taught it.
    fn read_directory(
        &self,
        _directory: &std::path::Path,
    ) -> Option<Vec<super::source::LibraryDirectoryEntry>> {
        None
    }

    fn probe(
        &self,
        at: &std::path::Path,
        links: super::source::LibraryLinkMode,
    ) -> super::source::LibraryPathPresence {
        *self
            .probe_counts
            .lock()
            .unwrap()
            .entry(at.to_path_buf())
            .or_default() += 1;
        super::source::UnixLibrarySource.probe(at, links)
    }

    fn walk(
        &self,
        _root: &std::path::Path,
        _order: super::source::LibraryWalkOrder,
        visitor: &mut dyn super::source::LibraryWalkVisitor,
    ) {
        for item in &self.items {
            if visitor.visit(item.clone()) == super::source::LibraryWalkControl::Stop {
                break;
            }
        }
    }
}

fn scripted_file(path: &std::path::Path) -> super::source::LibraryWalkItem {
    super::source::LibraryWalkItem::Entry(super::source::LibraryEntry {
        path: path.to_path_buf(),
        is_file: true,
        metadata: None,
    })
}

fn scripted_file_with_metadata(path: &std::path::Path) -> super::source::LibraryWalkItem {
    let super::source::LibraryPathPresence::Present(metadata) =
        super::source::UnixLibrarySource.probe(path, super::source::LibraryLinkMode::Follow)
    else {
        panic!("fixture file must be reachable");
    };
    super::source::LibraryWalkItem::Entry(super::source::LibraryEntry {
        path: path.to_path_buf(),
        is_file: true,
        metadata: Some(metadata),
    })
}

fn scripted_virtual_file(path: &std::path::Path, size: u64) -> super::source::LibraryWalkItem {
    super::source::LibraryWalkItem::Entry(super::source::LibraryEntry {
        path: path.to_path_buf(),
        is_file: true,
        metadata: Some(super::source::LibraryPathMetadata {
            is_file: true,
            is_directory: false,
            size: Some(size),
            modified: Some(std::time::UNIX_EPOCH),
            identity: None,
        }),
    })
}

/// The scanner consumes `LibrarySource::walk`, not `walkdir`: a source that
/// never touches a directory still produces a complete, correct scan.
#[test]
fn a_scan_runs_to_completion_through_a_non_filesystem_traversal() {
    let tmp = tempfile::tempdir().unwrap();
    let first = fixture_copy(tmp.path(), "first.flac");
    let second = fixture_copy(tmp.path(), "second.flac");
    // A non-audio entry the source offers anyway — the audio filter lives in
    // the scanner, so it must reject this one without the source's help.
    std::fs::write(tmp.path().join("notes.txt"), b"not music").unwrap();

    let db = crate::db::Db::open_in_memory().unwrap();
    let source = ScriptedSource::new(vec![
        scripted_file(&first),
        scripted_file(&tmp.path().join("notes.txt")),
        scripted_file(&second),
    ]);

    let report = completed(scan_folder_with_source(&source, &db, tmp.path()).unwrap());

    assert_eq!(report.added, 2, "both audio files must be catalogued");
    assert_eq!(report.errors, 0);
    let rows: i64 = db
        .conn()
        .query_row("SELECT count(*) FROM tracks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 2, "the non-audio entry must not become a track");
}

/// The import-error plumbing is reached through the trait too. The existing
/// permission-based test proves this on Unix but skips itself wherever
/// directory permissions are not enforced (as root, or on some CI images);
/// a scripted source proves the same path with no permissions involved.
#[test]
fn a_traversal_error_from_a_foreign_source_is_recorded_and_the_walk_continues() {
    let tmp = tempfile::tempdir().unwrap();
    let readable = fixture_copy(tmp.path(), "readable.flac");

    let db = crate::db::Db::open_in_memory().unwrap();
    let source = ScriptedSource::new(vec![
        super::source::LibraryWalkItem::Error(super::source::LibraryWalkError {
            path: Some(tmp.path().join("unreachable")),
            kind: super::source::LibraryWalkErrorKind::PermissionDenied,
            detail: "the provider refused this subtree".to_string(),
        }),
        scripted_file(&readable),
    ]);

    let report = completed(scan_folder_with_source(&source, &db, tmp.path()).unwrap());

    assert_eq!(report.errors, 1, "the failure must be counted");
    assert_eq!(
        report.added, 1,
        "a failure in one subtree must not stop the rest of the walk"
    );
    let recorded: (String, String) = db
        .conn()
        .query_row("SELECT path, reason_kind FROM import_errors", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(recorded.0, tmp.path().join("unreachable").to_string_lossy());
    assert_eq!(
        recorded.1,
        crate::models::ImportErrorKind::PermissionDenied.as_str(),
        "the source's error kind must survive the crossing, not collapse to unknown"
    );
}

#[test]
fn scanner_queries_each_audio_file_once_and_skips_non_audio_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    let first = fixture_copy(tmp.path(), "first.flac");
    let second = fixture_copy(tmp.path(), "second.flac");
    let notes = tmp.path().join("notes.txt");
    std::fs::write(&notes, b"not music").unwrap();
    let source = ScriptedSource::new(vec![
        scripted_file(&first),
        scripted_file(&notes),
        scripted_file(&second),
    ]);
    let db = crate::db::Db::open_in_memory().unwrap();

    completed(scan_folder_with_source(&source, &db, tmp.path()).unwrap());

    assert_eq!(source.probe_count(&first), 1);
    assert_eq!(source.probe_count(&second), 1);
    assert_eq!(source.probe_count(&notes), 0);
    assert_eq!(source.open_count(&first), 1);
    assert_eq!(source.open_count(&second), 1);
    assert_eq!(source.open_count(&notes), 0);
}

#[test]
fn scanner_reuses_metadata_carried_by_the_walk_without_an_extra_query() {
    let tmp = tempfile::tempdir().unwrap();
    let track = fixture_copy(tmp.path(), "carried.flac");
    let source = ScriptedSource::new(vec![scripted_file_with_metadata(&track)]);
    let db = crate::db::Db::open_in_memory().unwrap();

    let report = completed(scan_folder_with_source(&source, &db, tmp.path()).unwrap());

    assert_eq!(report.added, 1);
    assert_eq!(source.probe_count(&track), 0);
    assert_eq!(source.open_count(&track), 1);
}

#[test]
fn complete_scan_persists_tags_read_only_from_the_library_source() {
    let tmp = tempfile::tempdir().unwrap();
    let staging = fixture_copy(tmp.path(), "staging.flac");
    tag_file(
        &staging,
        "Source-only title",
        "Source-only artist",
        "Source-only album",
    );
    let bytes = std::fs::read(&staging).unwrap();
    std::fs::remove_file(&staging).unwrap();

    let logical_path = tmp.path().join("provider-track.flac");
    assert!(
        !logical_path.exists(),
        "the scanner path must not be a file"
    );
    let source = ScriptedSource::new(vec![scripted_virtual_file(
        &logical_path,
        bytes.len() as u64,
    )])
    .with_content(logical_path.clone(), bytes);
    let db = crate::db::Db::open_in_memory().unwrap();

    let report = completed(scan_folder_with_source(&source, &db, tmp.path()).unwrap());

    assert_eq!(report.added, 1);
    assert_eq!(report.errors, 0);
    let stored: (String, String, String, i64) = db
        .conn()
        .query_row(
            "SELECT title, artist, album, duration_ms FROM tracks WHERE path=?1",
            [logical_path.to_string_lossy().to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(stored.0, "Source-only title");
    assert_eq!(stored.1, "Source-only artist");
    assert_eq!(stored.2, "Source-only album");
    assert!(stored.3 > 0, "the real source duration must be persisted");
    assert_eq!(source.open_count(&logical_path), 1);
}

fn recorded_reason(db: &crate::db::Db, path: &std::path::Path) -> String {
    db.conn()
        .query_row(
            "SELECT reason_kind FROM import_errors WHERE path=?1",
            [path.to_string_lossy().to_string()],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn broken_tag_verdict_is_identical_through_a_source_and_a_path() {
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/broken-tags.mp3");
    let bytes = std::fs::read(&fixture).unwrap();

    let path_root = tempfile::tempdir().unwrap();
    let path_track = path_root.path().join("broken.mp3");
    std::fs::write(&path_track, &bytes).unwrap();
    let path_db = crate::db::Db::open_in_memory().unwrap();
    let path_report = completed(scan_folder(&path_db, path_root.path()).unwrap());
    assert_eq!(path_report.added, 1);

    let source_root = tempfile::tempdir().unwrap();
    let source_track = source_root.path().join("broken.mp3");
    let source = ScriptedSource::new(vec![scripted_virtual_file(
        &source_track,
        bytes.len() as u64,
    )])
    .with_content(source_track.clone(), bytes);
    let source_db = crate::db::Db::open_in_memory().unwrap();
    let source_report =
        completed(scan_folder_with_source(&source, &source_db, source_root.path()).unwrap());
    assert_eq!(source_report.added, 1);

    let path_reason = recorded_reason(&path_db, &path_track);
    let source_reason = recorded_reason(&source_db, &source_track);
    assert_eq!(source_reason, path_reason);
    assert_eq!(
        source_reason,
        crate::models::ImportErrorKind::UnreadableTags.as_str()
    );
}

#[test]
fn damaged_front_tag_verdict_matches_the_strict_path_reader() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/broken-front-id3v2-damaged-ape.mp3");
    let bytes = std::fs::read(&fixture).unwrap();
    let path_root = tempfile::tempdir().unwrap();
    let path_track = path_root.path().join("broken-front.mp3");
    std::fs::write(&path_track, &bytes).unwrap();
    let path_kind = match track_meta::read_meta(&path_track).unwrap_err() {
        ScanError::Import { kind, .. } => kind,
        other => panic!("strict path read returned a non-import error: {other}"),
    };

    let source_root = tempfile::tempdir().unwrap();
    let source_track = source_root.path().join("broken-front.mp3");
    let source = ScriptedSource::new(vec![scripted_virtual_file(
        &source_track,
        bytes.len() as u64,
    )])
    .with_content(source_track.clone(), bytes);
    let source_db = crate::db::Db::open_in_memory().unwrap();
    let report =
        completed(scan_folder_with_source(&source, &source_db, source_root.path()).unwrap());

    assert_eq!(report.added + report.errors, 1);
    assert_eq!(
        recorded_reason(&source_db, &source_track),
        path_kind.as_str()
    );
    assert_eq!(path_kind, crate::models::ImportErrorKind::UnreadableTags);
}

#[test]
fn unknown_extension_keeps_the_path_read_unknown_format_verdict() {
    let tmp = tempfile::tempdir().unwrap();
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac"),
    )
    .unwrap();
    let path_track = tmp.path().join("path-track.unknown");
    std::fs::write(&path_track, &bytes).unwrap();
    let path_error = crate::library::tag_edit::read_editable_tags(&path_track).unwrap_err();

    let source_track = tmp.path().join("source-track.unknown");
    let source = ScriptedSource::new(Vec::new()).with_content(source_track.clone(), bytes);
    let source_error =
        crate::library::tag_edit::read_editable_tags_with_source(&source, &source_track)
            .unwrap_err();

    let classify = |error| match error {
        crate::library::tag_edit::TagEditError::Lofty(error) => {
            crate::library::import_errors::classify_lofty(&error).0
        }
        crate::library::tag_edit::TagEditError::NoWritableTag => {
            panic!("a read cannot fail for lack of a writable tag")
        }
    };
    let source_kind = classify(source_error);
    let path_kind = classify(path_error);
    assert_eq!(source_kind, path_kind);
    assert_eq!(
        source_kind,
        crate::models::ImportErrorKind::UnsupportedFormat
    );
}
