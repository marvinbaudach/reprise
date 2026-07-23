use std::path::{Path, PathBuf};

use super::*;

/// Copies the bundled FLAC fixture into `dir` under `name`.
fn flac_fixture(dir: &Path, name: &str) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let destination = dir.join(name);
    std::fs::copy(source, &destination).unwrap();
    destination
}

fn migrated() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn
}

fn seed_track(conn: &Connection, id: i64, path: &str) {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at, file_mtime, file_size) \
         VALUES (?1, ?2, 'T', 'A', 1, 1, 1)",
        params![id, path],
    )
    .unwrap();
}

#[test]
fn ai_tags_round_trip_through_a_flac() {
    let dir = tempfile::tempdir().unwrap();
    let path = flac_fixture(dir.path(), "render.flac");
    let tags = AiTagSet {
        kind: KIND_VOCALS_REMOVED.to_string(),
        model: "htdemucs@4".to_string(),
        source_text: Some("Radiohead — Creep".to_string()),
        source_mbid: Some("abcd-1234".to_string()),
    };

    write_ai_tags(&path, &tags).unwrap();

    assert_eq!(read_ai_tags(&path), Some(tags));
}

#[test]
fn ai_tags_without_a_source_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = flac_fixture(dir.path(), "generated.flac");
    let tags = AiTagSet {
        kind: KIND_VOCALS_REMOVED.to_string(),
        model: "htdemucs@4".to_string(),
        source_text: None,
        source_mbid: None,
    };

    write_ai_tags(&path, &tags).unwrap();

    let read = read_ai_tags(&path).unwrap();
    assert_eq!(read.source_text, None);
    assert_eq!(read.source_mbid, None);
    assert_eq!(read.kind, KIND_VOCALS_REMOVED);
}

#[test]
fn write_ai_tags_preserves_existing_standard_tags() {
    use lofty::prelude::*;
    let dir = tempfile::tempdir().unwrap();
    let path = flac_fixture(dir.path(), "titled.flac");
    // Set a standard title through the ordinary lofty path first.
    crate::library::tag_edit::apply_patch_to_file(
        &path,
        &crate::library::tag_edit::TagPatch {
            title: Some("Creep (Instrumental)".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    write_ai_tags(
        &path,
        &AiTagSet {
            kind: KIND_VOCALS_REMOVED.to_string(),
            model: "m@1".to_string(),
            source_text: Some("Radiohead — Creep".to_string()),
            source_mbid: None,
        },
    )
    .unwrap();

    // The standard title survives alongside the new provenance tags.
    let tagged = lofty::read_from_path(&path).unwrap();
    let title = tagged
        .primary_tag()
        .and_then(lofty::tag::Accessor::title)
        .map(std::borrow::Cow::into_owned);
    assert_eq!(title.as_deref(), Some("Creep (Instrumental)"));
    assert!(read_ai_tags(&path).is_some());
}

#[test]
fn read_ai_tags_returns_none_for_a_plain_flac() {
    let dir = tempfile::tempdir().unwrap();
    let path = flac_fixture(dir.path(), "plain.flac");
    assert_eq!(read_ai_tags(&path), None);
}

#[test]
fn read_ai_tags_returns_none_for_a_non_flac_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, b"not audio").unwrap();
    assert_eq!(read_ai_tags(&path), None);
}

#[test]
fn human_comment_names_the_manipulation() {
    assert_eq!(
        human_comment(KIND_VOCALS_REMOVED),
        "AI-manipulated: vocals removed (Reprise)"
    );
    assert_eq!(
        human_comment("something-else"),
        "AI-manipulated: something-else (Reprise)"
    );
}

#[test]
fn provenance_round_trips_through_the_db() {
    let conn = migrated();
    seed_track(&conn, 1, "/src.flac");
    seed_track(&conn, 2, "/inst.flac");
    let input = ProvenanceInput {
        kind: KIND_VOCALS_REMOVED.to_string(),
        ai: true,
        source_track_id: Some(1),
        source_text: Some("A — T".to_string()),
        source_mbid: Some("mbid".to_string()),
        model: Some("m@1".to_string()),
    };

    insert_provenance(&conn, 2, &input, 500).unwrap();

    let read = get_provenance(&conn, 2).unwrap().unwrap();
    assert_eq!(read.track_id, 2);
    assert!(read.ai);
    assert_eq!(read.source_track_id, Some(1));
    assert_eq!(read.source_text.as_deref(), Some("A — T"));
    assert_eq!(read.model.as_deref(), Some("m@1"));
    assert_eq!(read.created_at, 500);
    assert!(is_ai_track(&conn, 2).unwrap());
    assert!(!is_ai_track(&conn, 1).unwrap());
}

#[test]
fn insert_provenance_is_idempotent() {
    let conn = migrated();
    seed_track(&conn, 2, "/inst.flac");
    let mut input = ProvenanceInput {
        kind: KIND_VOCALS_REMOVED.to_string(),
        ai: true,
        source_track_id: None,
        source_text: Some("first".to_string()),
        source_mbid: None,
        model: None,
    };
    insert_provenance(&conn, 2, &input, 0).unwrap();
    input.source_text = Some("second".to_string());
    insert_provenance(&conn, 2, &input, 1).unwrap();

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM track_provenance", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        get_provenance(&conn, 2)
            .unwrap()
            .unwrap()
            .source_text
            .as_deref(),
        Some("second")
    );
}

#[test]
fn deleting_the_original_keeps_textual_provenance() {
    let conn = migrated();
    seed_track(&conn, 1, "/src.flac");
    seed_track(&conn, 2, "/inst.flac");
    insert_provenance(
        &conn,
        2,
        &ProvenanceInput {
            kind: KIND_VOCALS_REMOVED.to_string(),
            ai: true,
            source_track_id: Some(1),
            source_text: Some("A — T".to_string()),
            source_mbid: None,
            model: None,
        },
        0,
    )
    .unwrap();

    conn.execute("DELETE FROM tracks WHERE id = 1", []).unwrap();

    let read = get_provenance(&conn, 2).unwrap().unwrap();
    assert_eq!(read.source_track_id, None, "FK nulls the source link");
    assert_eq!(read.source_text.as_deref(), Some("A — T"), "text survives");
    assert!(is_ai_track(&conn, 2).unwrap());
}

#[test]
fn reconstruct_provenance_rebuilds_from_tags_on_a_fresh_db() {
    let dir = tempfile::tempdir().unwrap();
    let path = flac_fixture(dir.path(), "reconstruct.flac");
    write_ai_tags(
        &path,
        &AiTagSet {
            kind: KIND_VOCALS_REMOVED.to_string(),
            model: "htdemucs@4".to_string(),
            source_text: Some("A — T".to_string()),
            source_mbid: Some("mbid".to_string()),
        },
    )
    .unwrap();
    let conn = migrated();
    seed_track(&conn, 9, path.to_str().unwrap());

    assert!(reconstruct_provenance(&conn, 9, &path, 700).unwrap());

    let read = get_provenance(&conn, 9).unwrap().unwrap();
    assert!(read.ai);
    // Beschluss 13: the source is textual only — never an app-internal id.
    assert_eq!(read.source_track_id, None);
    assert_eq!(read.source_text.as_deref(), Some("A — T"));
    assert_eq!(read.source_mbid.as_deref(), Some("mbid"));
    assert_eq!(read.model.as_deref(), Some("htdemucs@4"));
}

#[test]
fn reconstruct_skips_known_and_non_ai_tracks() {
    let dir = tempfile::tempdir().unwrap();
    let plain = flac_fixture(dir.path(), "plain.flac");
    let conn = migrated();
    seed_track(&conn, 1, plain.to_str().unwrap());
    // A plain FLAC has no AI tags -> nothing to reconstruct.
    assert!(!reconstruct_provenance(&conn, 1, &plain, 0).unwrap());
    assert!(get_provenance(&conn, 1).unwrap().is_none());

    // A track that already has provenance is left untouched.
    seed_track(&conn, 2, "/inst.flac");
    insert_provenance(
        &conn,
        2,
        &ProvenanceInput {
            kind: KIND_VOCALS_REMOVED.to_string(),
            ai: true,
            source_track_id: None,
            source_text: Some("keep".to_string()),
            source_mbid: None,
            model: None,
        },
        0,
    )
    .unwrap();
    assert!(!reconstruct_provenance(&conn, 2, Path::new("/inst.flac"), 1).unwrap());
    assert_eq!(
        get_provenance(&conn, 2)
            .unwrap()
            .unwrap()
            .source_text
            .as_deref(),
        Some("keep")
    );
}

#[test]
fn reconstruct_all_missing_sweeps_the_library() {
    let dir = tempfile::tempdir().unwrap();
    let ai_path = flac_fixture(dir.path(), "ai.flac");
    write_ai_tags(
        &ai_path,
        &AiTagSet {
            kind: KIND_VOCALS_REMOVED.to_string(),
            model: "m@1".to_string(),
            source_text: Some("A — T".to_string()),
            source_mbid: None,
        },
    )
    .unwrap();
    let plain_path = flac_fixture(dir.path(), "plain.flac");
    let conn = migrated();
    seed_track(&conn, 1, ai_path.to_str().unwrap());
    seed_track(&conn, 2, plain_path.to_str().unwrap());

    let reconstructed = reconstruct_all_missing(&conn, 0).unwrap();

    assert_eq!(reconstructed, 1, "only the AI track gets a row");
    assert!(is_ai_track(&conn, 1).unwrap());
    assert!(get_provenance(&conn, 2).unwrap().is_none());
    // A second sweep is a no-op (idempotent).
    assert_eq!(reconstruct_all_missing(&conn, 0).unwrap(), 0);
}
