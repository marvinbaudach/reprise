use std::path::{Path, PathBuf};

use super::*;

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

/// Seeds a source track and a finished-but-unsaved job for it, plus a staging
/// render on disk. Returns `(job_id, staging_store)`.
fn staged_job(conn: &Connection, staging_dir: &Path) -> (i64, StagingStore) {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, album_artist, year, track_no, genre, added_at, file_mtime, file_size) \
         VALUES (1, '/music/creep.flac', 'Creep', 'Radiohead', 'Pablo Honey', 'Radiohead', 1993, 2, 'Alt Rock', 1, 1, 1)",
        [],
    )
    .unwrap();
    let staging = StagingStore::new(staging_dir);
    let job_id = ai_jobs::enqueue_instrumental(conn, &staging, 1, "htdemucs@4", 0)
        .unwrap()
        .job_id();
    conn.execute("UPDATE ai_jobs SET status = 'done' WHERE id = ?1", [job_id])
        .unwrap();
    // Put a real FLAC render in place for this job.
    staging.ensure_dir().unwrap();
    let render = staging.path_for_job(job_id);
    std::fs::copy(flac_fixture(staging_dir, "seed.flac"), &render).unwrap();
    (job_id, staging)
}

#[test]
fn promote_files_tags_registers_and_records_provenance() {
    use lofty::prelude::*;
    let library = tempfile::tempdir().unwrap();
    let staging_dir = tempfile::tempdir().unwrap();
    let mut conn = migrated();
    let (job_id, staging) = staged_job(&conn, staging_dir.path());
    let config = PromotionConfig::new(library.path());

    let outcome = promote(&mut conn, &staging, &config, job_id, 900).unwrap();

    // 1. Filed at <root>/Reprise Instrumentals/<Artist>/<Title> (Instrumental).flac
    let expected = library
        .path()
        .join("Reprise Instrumentals")
        .join("Radiohead")
        .join("Creep (Instrumental).flac");
    assert_eq!(outcome.path, expected);
    assert!(expected.is_file(), "the promoted file exists");

    // 2. Final tags: title suffixed, album unchanged, provenance present.
    let tagged = lofty::read_from_path(&expected).unwrap();
    let tag = tagged.primary_tag().unwrap();
    assert_eq!(tag.title().as_deref(), Some("Creep (Instrumental)"));
    assert_eq!(tag.album().as_deref(), Some("Pablo Honey"));
    let ai = crate::provenance::read_ai_tags(&expected).unwrap();
    assert_eq!(ai.kind, crate::provenance::KIND_VOCALS_REMOVED);
    assert_eq!(ai.model, "htdemucs@4");
    assert_eq!(ai.source_text.as_deref(), Some("Radiohead — Creep"));

    // 3. Registered track + provenance row keyed on the AI flag.
    let provenance = crate::provenance::get_provenance(&conn, outcome.result_track_id)
        .unwrap()
        .unwrap();
    assert!(provenance.ai);
    assert_eq!(provenance.source_track_id, Some(1));
    assert_eq!(provenance.source_text.as_deref(), Some("Radiohead — Creep"));
    assert_eq!(provenance.model.as_deref(), Some("htdemucs@4"));

    // 4. Job moved staged -> saved; staging render discarded.
    let job = ai_jobs::get_job(&conn, job_id).unwrap().unwrap();
    assert_eq!(job.state, ai_jobs::JobState::Done);
    assert_eq!(job.result_track_id, Some(outcome.result_track_id));
    assert!(
        !staging.exists(job_id),
        "staging render is discarded after save"
    );

    // 5. A `save` lifecycle event was recorded.
    let saved = crate::events::read_since(&conn, 0, None)
        .unwrap()
        .into_iter()
        .any(|change| change.entity == "ai_job" && change.operation == "save");
    assert!(saved);
}

#[test]
fn promote_rejects_a_job_that_is_not_a_finished_unsaved_render() {
    let library = tempfile::tempdir().unwrap();
    let staging_dir = tempfile::tempdir().unwrap();
    let mut conn = migrated();
    let (job_id, staging) = staged_job(&conn, staging_dir.path());
    let config = PromotionConfig::new(library.path());
    // Flip it back to queued: no longer promotable.
    conn.execute(
        "UPDATE ai_jobs SET status = 'queued' WHERE id = ?1",
        [job_id],
    )
    .unwrap();

    let error = promote(&mut conn, &staging, &config, job_id, 0).unwrap_err();
    assert!(matches!(error, PromotionError::NotPromotable(_)));
}

#[test]
fn promote_reports_missing_staging_render() {
    let library = tempfile::tempdir().unwrap();
    let staging_dir = tempfile::tempdir().unwrap();
    let mut conn = migrated();
    let (job_id, staging) = staged_job(&conn, staging_dir.path());
    let config = PromotionConfig::new(library.path());
    staging.discard(job_id).unwrap(); // remove the render

    let error = promote(&mut conn, &staging, &config, job_id, 0).unwrap_err();
    assert!(matches!(error, PromotionError::StagingMissing(_)));
}

#[test]
fn promote_falls_back_to_render_tags_when_the_original_is_gone() {
    use lofty::prelude::*;
    let library = tempfile::tempdir().unwrap();
    let staging_dir = tempfile::tempdir().unwrap();
    let mut conn = migrated();
    let (job_id, staging) = staged_job(&conn, staging_dir.path());
    let config = PromotionConfig::new(library.path());
    // Give the render its own tags, then delete the original (nulls the job's
    // source_track_id via the FK).
    let render = staging.path_for_job(job_id);
    crate::library::tag_edit::apply_patch_to_file(
        &render,
        &crate::library::tag_edit::TagPatch {
            title: Some("Orphan".to_string()),
            artist: Some("Ghost".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    conn.execute("DELETE FROM tracks WHERE id = 1", []).unwrap();

    let outcome = promote(&mut conn, &staging, &config, job_id, 0).unwrap();

    let tagged = lofty::read_from_path(&outcome.path).unwrap();
    assert_eq!(
        tagged.primary_tag().unwrap().title().as_deref(),
        Some("Orphan (Instrumental)")
    );
    let provenance = crate::provenance::get_provenance(&conn, outcome.result_track_id)
        .unwrap()
        .unwrap();
    assert_eq!(provenance.source_track_id, None, "the original is gone");
    assert_eq!(provenance.source_text.as_deref(), Some("Ghost — Orphan"));
}

#[test]
fn path_guard_contains_the_target_subtree() {
    let root = Path::new("/library/Reprise Instrumentals");
    assert!(is_within(root, &root.join("Artist/Song.flac")));
    // An escape via `..` is rejected.
    assert!(!is_within(root, &root.join("../../etc/passwd")));
    // A sibling with a shared prefix is not "within".
    assert!(!is_within(
        root,
        Path::new("/library/Reprise Instrumentals Evil/x.flac")
    ));
    // The root itself is not a valid destination.
    assert!(!is_within(root, root));
}

#[test]
fn sanitize_component_neutralises_separators_and_dot_segments() {
    assert_eq!(sanitize_component("Radiohead"), "Radiohead");
    assert_eq!(sanitize_component("AC/DC"), "AC_DC");
    assert_eq!(sanitize_component(".."), "Unknown");
    assert_eq!(sanitize_component("   "), "Unknown");
    assert_eq!(sanitize_component(""), "Unknown");
    // A traversal attempt collapses to a single, contained component.
    let sanitized = sanitize_component("../../etc");
    assert!(!sanitized.contains('/'));
    assert_ne!(sanitized, "..");
}

#[test]
fn resolve_destination_builds_the_documented_layout() {
    let config = PromotionConfig::new("/library");
    let source = SourceMeta {
        title: "Creep".to_string(),
        artist: "Radiohead".to_string(),
        album: "Pablo Honey".to_string(),
        album_artist: "Radiohead".to_string(),
        year: Some(1993),
        track_no: Some(2),
        genre: "Alt Rock".to_string(),
    };
    assert_eq!(
        resolve_destination(&config, &source).unwrap(),
        PathBuf::from("/library/Reprise Instrumentals/Radiohead/Creep (Instrumental).flac")
    );
}

#[test]
fn a_custom_subfolder_is_honored() {
    let mut config = PromotionConfig::new("/library");
    config.subfolder = "AI Renders".to_string();
    assert_eq!(
        config.instrumentals_root(),
        PathBuf::from("/library/AI Renders")
    );
}
