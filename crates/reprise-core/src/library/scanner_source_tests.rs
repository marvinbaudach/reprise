//! `scanner.rs`'s source-neutrality suite: the tests that drive a real scan
//! through a `LibrarySource` the scanner has never seen. Its own file for the
//! project's 800-line reason — `scanner.rs` declares it via `#[cfg(test)]
//! #[path = "scanner_source_tests.rs"] mod source_tests;`.
//!
//! `scanner_tests.rs` proves what the scanner does; this file proves it does
//! it without knowing where the tree came from.

use super::tests::{completed, fixture_copy};
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
    probe_counts: Mutex<HashMap<std::path::PathBuf, usize>>,
}

impl ScriptedSource {
    fn new(items: Vec<super::source::LibraryWalkItem>) -> Self {
        Self {
            items,
            probe_counts: Mutex::new(HashMap::new()),
        }
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
    ) -> Option<super::source::LibraryPathMetadata> {
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
    let metadata = super::source::UnixLibrarySource
        .probe(path, super::source::LibraryLinkMode::Follow)
        .expect("fixture file must be reachable");
    super::source::LibraryWalkItem::Entry(super::source::LibraryEntry {
        path: path.to_path_buf(),
        is_file: true,
        metadata: Some(metadata),
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
}
