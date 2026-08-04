use super::*;

#[test]
fn scan_reports_discovery_then_monotone_audio_file_progress() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    std::fs::copy(&fixture, dir.path().join("first.flac")).unwrap();
    std::fs::copy(&fixture, dir.path().join("second.FLAC")).unwrap();
    std::fs::write(dir.path().join("notes.txt"), b"not music").unwrap();

    let conn = crate::db::Db::open_in_memory().unwrap();
    let mut progress = Vec::new();

    let report = super::tests::completed(
        scan_folder_with_progress(&conn, dir.path(), |event| {
            progress.push(event);
        })
        .unwrap(),
    );

    assert_eq!(progress.first(), Some(&ScanProgress::Discovering));
    let scanning: Vec<_> = progress
        .iter()
        .filter_map(|event| match event {
            ScanProgress::Scanning {
                processed,
                total,
                current_path,
            } => Some((*processed, *total, current_path.clone())),
            ScanProgress::Discovering | ScanProgress::Fetching { .. } => None,
        })
        .collect();
    assert_eq!(scanning.len(), 2);
    assert_eq!(scanning[0].0, 1);
    assert_eq!(scanning[1].0, 2);
    // A first scan has no previous catalog to size itself against, and the
    // source is not walked twice to produce one. Every event therefore reports
    // no total at all, which the UI renders as indeterminate — rather than
    // raising the total to match `processed` and claiming completion for every
    // file. See `ScanProgress::Scanning::total`.
    assert_eq!(scanning[0].1, None);
    assert_eq!(scanning[1].1, None);
    let names: std::collections::HashSet<_> = scanning
        .iter()
        .filter_map(|(_, _, path)| path.file_name().and_then(|name| name.to_str()))
        .collect();
    assert_eq!(
        names,
        std::collections::HashSet::from(["first.flac", "second.FLAC"])
    );

    assert_eq!(report.added, 2);
    assert_eq!(report.updated, 0);
    assert_eq!(report.skipped_unchanged, 0);
    assert_eq!(report.errors, 0);
    assert_eq!(report.moved, 0);
}

#[test]
fn rescan_uses_the_previous_catalog_size_as_its_progress_estimate() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    std::fs::copy(&fixture, dir.path().join("first.flac")).unwrap();
    std::fs::copy(&fixture, dir.path().join("second.flac")).unwrap();

    let db = crate::db::Db::open_in_memory().unwrap();
    super::tests::completed(scan_folder(&db, dir.path()).unwrap());
    std::fs::copy(&fixture, dir.path().join("third.flac")).unwrap();

    let mut progress = Vec::new();
    super::tests::completed(
        scan_folder_with_progress(&db, dir.path(), |event| progress.push(event)).unwrap(),
    );

    let totals: Vec<_> = progress
        .iter()
        .filter_map(|event| match event {
            ScanProgress::Scanning { total, .. } => Some(*total),
            ScanProgress::Discovering | ScanProgress::Fetching { .. } => None,
        })
        .collect();
    // The estimate holds at the previous catalog's two rows and grows only
    // when the walk turns up a third file — an estimate that is too small
    // corrects itself, it never shrinks.
    assert_eq!(totals, vec![Some(2), Some(2), Some(3)]);
}

/// The rename of `total` to an `Option` exists for exactly this: the first
/// scan of a fresh library, the longest one a user ever waits through, must
/// not report a full bar from its first file onward. Guarding it here means a
/// future estimate that "helpfully" falls back to `processed` fails loudly.
#[test]
fn a_first_scan_reports_no_total_rather_than_a_full_bar() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    for name in ["a.flac", "b.flac", "c.flac"] {
        std::fs::copy(&fixture, dir.path().join(name)).unwrap();
    }

    let db = crate::db::Db::open_in_memory().unwrap();
    let mut progress = Vec::new();
    super::tests::completed(
        scan_folder_with_progress(&db, dir.path(), |event| progress.push(event)).unwrap(),
    );

    let scanning: Vec<_> = progress
        .iter()
        .filter_map(|event| match event {
            ScanProgress::Scanning {
                processed, total, ..
            } => Some((*processed, *total)),
            ScanProgress::Discovering | ScanProgress::Fetching { .. } => None,
        })
        .collect();

    assert_eq!(scanning.len(), 3);
    assert!(
        scanning.iter().all(|(_, total)| total.is_none()),
        "a first scan must report no denominator, got {scanning:?}"
    );
}
