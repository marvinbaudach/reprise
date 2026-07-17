//! `import_errors.rs`'s own unit-level test suite: the pieces that don't
//! need a full `scan_folder` walk to exercise — `clear_error`'s return value
//! and `classify_lofty`'s mapping. The integration-shaped cases (episode
//! dedup across repeated scans, the dismiss-skip fast path, the directory
//! `chmod` case) live in `scanner_import_errors_tests.rs` instead, since
//! those need the real walk loop in `scan_folder_inner` to mean anything.

use rusqlite::Connection;

use super::*;

fn migrated_conn() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn
}

/// `clear_error` must report whether a row actually existed to delete — the
/// bool a future "healed" counter would key off — not just perform the
/// delete silently.
#[test]
fn clear_error_reports_whether_a_row_existed() {
    let mut conn = migrated_conn();
    let tx = conn.transaction().unwrap();

    let existed_before_any_row = clear_error(&tx, "/music/never-failed.flac").unwrap();
    assert!(!existed_before_any_row);

    record_error(
        &tx,
        "/music/broken.flac",
        ImportErrorKind::UnreadableTags,
        "bad data",
        1_000,
    )
    .unwrap();
    let existed = clear_error(&tx, "/music/broken.flac").unwrap();
    assert!(existed);

    let existed_again = clear_error(&tx, "/music/broken.flac").unwrap();
    assert!(
        !existed_again,
        "the row is gone after the first clear; a second clear must report false"
    );
}

/// `ErrorKind::UnknownFormat` — lofty couldn't even guess the file type —
/// must map to `UnsupportedFormat`, exactly as Step 3's mapping rule states.
#[test]
fn classify_lofty_maps_unknown_format_to_unsupported_format() {
    let err = lofty::error::LoftyError::new(lofty::error::ErrorKind::UnknownFormat);
    let (kind, detail) = classify_lofty(&err);
    assert_eq!(kind, ImportErrorKind::UnsupportedFormat);
    assert!(!detail.is_empty(), "detail must carry lofty's own text");
}

/// `ErrorKind::Io(e)` with `e.kind() == PermissionDenied` must map to
/// `PermissionDenied`, not the generic `Io` bucket — this is the case lofty's
/// `Display` text alone could never reliably distinguish (see this module's
/// doc comment), which is the whole reason `classify_lofty` matches on the
/// typed `io::Error` instead.
#[test]
fn classify_lofty_maps_io_permission_denied_to_permission_denied() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
    let err: lofty::error::LoftyError = io_err.into();
    let (kind, _detail) = classify_lofty(&err);
    assert_eq!(kind, ImportErrorKind::PermissionDenied);
}
