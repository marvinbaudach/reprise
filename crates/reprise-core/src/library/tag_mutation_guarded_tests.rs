use std::path::Path;

use lofty::prelude::*;

use super::tag_edit::read_editable_tags;
use super::tag_mutation::{commit_guarded_tag_changes, GuardedTagChange, GuardedTagField};

const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

fn seeded_track() -> (tempfile::TempDir, crate::db::Db, i64, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let path = dir.path().join("typed-track-number.flac");
    std::fs::copy(source, &path).unwrap();
    let mut tagged = lofty::read_from_path(&path).unwrap();
    tagged.primary_tag_mut().unwrap().set_track(3);
    tagged
        .primary_tag()
        .unwrap()
        .save_to_path(&path, lofty::config::WriteOptions::default())
        .unwrap();
    let conn = crate::db::Db::open_in_memory().unwrap();
    super::scanner::scan_folder(&conn, &path).unwrap();
    let id = conn
        .conn()
        .query_row(
            "SELECT id FROM tracks WHERE path=?1",
            [path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )
        .unwrap();
    (dir, conn, id, path)
}

#[test]
fn guarded_fields_are_typed_and_track_number_never_silently_noops() {
    let (_dir, conn, id, path) = seeded_track();

    let result = commit_guarded_tag_changes(
        conn.conn(),
        id,
        &path,
        &[GuardedTagChange {
            field: GuardedTagField::TrackNo,
            expected: Some("3".into()),
            after: Some("9".into()),
        }],
        false,
    )
    .unwrap();

    assert_eq!(result.applied, vec![GuardedTagField::TrackNo]);
    assert_eq!(read_editable_tags(&path).unwrap().track_no, Some(9));
}

#[test]
fn guarded_numeric_values_are_validated_before_save() {
    let (_dir, conn, id, path) = seeded_track();

    let failure = commit_guarded_tag_changes(
        conn.conn(),
        id,
        &path,
        &[GuardedTagChange {
            field: GuardedTagField::TrackNo,
            expected: Some("3".into()),
            after: Some("not-a-number".into()),
        }],
        false,
    )
    .unwrap_err();

    assert!(!failure.file_written);
    assert_eq!(read_editable_tags(&path).unwrap().track_no, Some(3));
}

#[test]
fn guarded_save_preserves_embedded_cover_art() {
    use lofty::picture::{MimeType, Picture, PictureType};

    let (_dir, conn, id, path) = seeded_track();
    let mut tagged = lofty::read_from_path(&path).unwrap();
    tagged.primary_tag_mut().unwrap().push_picture(
        Picture::unchecked(TINY_PNG.to_vec())
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::Png)
            .build(),
    );
    tagged
        .primary_tag()
        .unwrap()
        .save_to_path(&path, lofty::config::WriteOptions::default())
        .unwrap();

    commit_guarded_tag_changes(
        conn.conn(),
        id,
        &path,
        &[GuardedTagChange {
            field: GuardedTagField::TrackNo,
            expected: Some("3".into()),
            after: Some("9".into()),
        }],
        false,
    )
    .unwrap();

    let tagged = lofty::read_from_path(&path).unwrap();
    let pictures = tagged.primary_tag().unwrap().pictures();
    assert_eq!(pictures.len(), 1);
    assert_eq!(pictures[0].data(), TINY_PNG);
}

#[test]
fn production_tag_mutations_have_one_loaded_container_save_seam() {
    let mutation = include_str!("tag_mutation.rs");
    let guarded = include_str!("tag_mutation_guarded.rs");
    let production_mutation = mutation.split("#[cfg(test)]").next().unwrap();

    assert_eq!(
        production_mutation.matches(".save_to_path(").count()
            + guarded.matches(".save_to_path(").count(),
        1
    );
}
