use std::path::Path;

use crate::db::Db;
use crate::library::source_test_support::NamedUnixSource;

use super::{relink_track_with_source, RelinkTarget};

#[test]
fn relink_uses_the_source_name_when_the_item_has_no_title_tag() {
    let directory = tempfile::tempdir().unwrap();
    let new_path = directory.path().join("opaque-source-id.flac");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    std::fs::copy(fixture, &new_path).unwrap();
    let old_path = directory.path().join("missing.flac");
    let db = Db::open_in_memory().unwrap();
    db.conn()
        .execute(
            "INSERT INTO tracks \
             (path, title, added_at, missing_since, missing_reason) \
             VALUES (?1, 'Previous title', 1, 10, 'deleted')",
            [old_path.to_string_lossy().as_ref()],
        )
        .unwrap();
    let track_id = db.conn().last_insert_rowid();
    let target = RelinkTarget { track_id, old_path };

    relink_track_with_source(&NamedUnixSource("Provider Track"), &db, &target, &new_path).unwrap();

    let title: String = db
        .conn()
        .query_row(
            "SELECT title FROM tracks WHERE id = ?1",
            [track_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(title, "Provider Track");
}
