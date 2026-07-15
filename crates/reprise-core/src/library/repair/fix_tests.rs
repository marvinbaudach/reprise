use std::path::PathBuf;

use super::*;

fn fixture_copy(dir: &std::path::Path, name: &str) -> PathBuf {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let dst = dir.join(name);
    std::fs::copy(&src, &dst).unwrap();
    dst
}

/// A no-op fixer that always returns `Skipped`.
struct NoOpFixer;

impl ExternalFixer for NoOpFixer {
    fn fix_vbr_header(&self, _path: &std::path::Path) -> FixOutcome {
        FixOutcome::Skipped {
            reason: "no-op fixer".into(),
        }
    }
}

#[test]
fn create_backup_copies_file() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fixture_copy(tmp.path(), "song.flac");
    let bak = create_backup(&path).unwrap();
    assert_eq!(bak, path.with_extension("flac.bak"));
    assert!(bak.exists());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        std::fs::read(&bak).unwrap(),
    );
}

#[test]
fn repair_with_no_issues_returns_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fixture_copy(tmp.path(), "song.flac");
    let reports = repair(&path, &[], &NoOpFixer, true);
    assert!(reports.is_empty());
}

#[test]
fn repair_creates_backup_before_tag_resave() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fixture_copy(tmp.path(), "song.flac");
    let original_bytes = std::fs::read(&path).unwrap();

    let reports = repair(
        &path,
        &[Issue::CorruptId3Frames],
        &NoOpFixer,
        true,
    );

    assert_eq!(reports.len(), 1);
    // Backup must exist and match the original
    let bak = path.with_extension("flac.bak");
    assert!(bak.exists());
    assert_eq!(std::fs::read(&bak).unwrap(), original_bytes);
}

#[test]
fn repair_skips_backup_when_disabled() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fixture_copy(tmp.path(), "song.flac");

    let _reports = repair(
        &path,
        &[Issue::CorruptId3Frames],
        &NoOpFixer,
        false,
    );

    let bak = path.with_extension("flac.bak");
    assert!(!bak.exists());
}

#[test]
fn repair_vbr_delegates_to_external_fixer() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fixture_copy(tmp.path(), "song.flac");

    let reports = repair(
        &path,
        &[Issue::MissingVbrHeader],
        &NoOpFixer,
        false,
    );

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].issue, Issue::MissingVbrHeader);
    assert!(matches!(reports[0].outcome, FixOutcome::Skipped { .. }));
}

#[test]
fn repair_backup_only_created_once_for_multiple_issues() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fixture_copy(tmp.path(), "song.flac");

    let _reports = repair(
        &path,
        &[Issue::DuplicateIlst, Issue::CorruptId3Frames],
        &NoOpFixer,
        true,
    );

    // Only one .bak file, not two
    let bak_count = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "bak"))
        .count();
    assert_eq!(bak_count, 1);
}
