use super::*;

#[test]
fn scan_reports_discovery_then_monotone_audio_file_progress() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    std::fs::copy(&fixture, dir.path().join("first.flac")).unwrap();
    std::fs::copy(&fixture, dir.path().join("second.FLAC")).unwrap();
    std::fs::write(dir.path().join("notes.txt"), b"not music").unwrap();

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let mut progress = Vec::new();

    let report = scan_folder_with_progress(&mut conn, dir.path(), |event| {
        progress.push(event);
    })
    .unwrap();

    assert_eq!(progress.first(), Some(&ScanProgress::Discovering));
    let scanning: Vec<_> = progress
        .iter()
        .filter_map(|event| match event {
            ScanProgress::Scanning {
                processed,
                total,
                current_path,
            } => Some((*processed, *total, current_path.clone())),
            ScanProgress::Discovering => None,
        })
        .collect();
    assert_eq!(scanning.len(), 2);
    assert_eq!(scanning[0].0, 1);
    assert_eq!(scanning[1].0, 2);
    assert!(scanning.iter().all(|(_, total, _)| *total == 2));
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
