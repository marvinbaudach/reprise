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

/// Seeds two DIFFERENT source tracks that share Artist + Title (a cover, a
/// live version, or a duplicate import), each with its own finished-but-unsaved
/// staged render. Both sanitise to the same base destination, so promoting both
/// exercises the collision path. Returns `([job_a, job_b], staging_store)`.
fn seed_identical_pair(conn: &Connection, staging_dir: &Path) -> (Vec<i64>, StagingStore) {
    let staging = StagingStore::new(staging_dir);
    staging.ensure_dir().unwrap();
    let mut jobs = Vec::new();
    for id in [1_i64, 2] {
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, album, album_artist, added_at, file_mtime, file_size) \
             VALUES (?1, ?2, 'Creep', 'Radiohead', 'Pablo Honey', 'Radiohead', 1, 1, 1)",
            rusqlite::params![id, format!("/music/creep-{id}.flac")],
        )
        .unwrap();
        let job_id = ai_jobs::enqueue_instrumental(conn, &staging, id, "htdemucs@4", 0)
            .unwrap()
            .job_id();
        conn.execute("UPDATE ai_jobs SET status = 'done' WHERE id = ?1", [job_id])
            .unwrap();
        std::fs::copy(
            flac_fixture(staging_dir, &format!("seed-{id}.flac")),
            staging.path_for_job(job_id),
        )
        .unwrap();
        jobs.push(job_id);
    }
    (jobs, staging)
}

#[test]
fn two_sources_with_the_same_name_get_distinct_files_and_provenance() {
    let library = tempfile::tempdir().unwrap();
    let staging_dir = tempfile::tempdir().unwrap();
    let mut conn = migrated();
    let (jobs, staging) = seed_identical_pair(&conn, staging_dir.path());
    let config = PromotionConfig::new(library.path());

    let out_a = promote(&mut conn, &staging, &config, jobs[0], 100).unwrap();
    let out_b = promote(&mut conn, &staging, &config, jobs[1], 200).unwrap();

    // Distinct files on disk — the second promotion must not clobber the first.
    assert_ne!(out_a.path, out_b.path);
    assert!(out_a.path.is_file() && out_b.path.is_file());
    assert_eq!(
        out_b.path.file_name().unwrap().to_string_lossy(),
        "Creep (Instrumental) (2).flac",
        "the colliding second file is deterministically suffixed"
    );

    // Distinct result tracks, each with its own provenance pointing at its own
    // source — B's INSERT OR REPLACE must not have flipped A's row.
    assert_ne!(out_a.result_track_id, out_b.result_track_id);
    let prov_a = crate::provenance::get_provenance(&conn, out_a.result_track_id)
        .unwrap()
        .unwrap();
    let prov_b = crate::provenance::get_provenance(&conn, out_b.result_track_id)
        .unwrap()
        .unwrap();
    assert_eq!(prov_a.source_track_id, Some(1));
    assert_eq!(prov_b.source_track_id, Some(2));

    // Job A's binding is intact after B's promotion.
    let job_a = ai_jobs::get_job(&conn, jobs[0]).unwrap().unwrap();
    assert_eq!(job_a.result_track_id, Some(out_a.result_track_id));
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
fn retry_after_a_failed_copy_does_not_double_the_instrumental_suffix() {
    use lofty::prelude::*;
    let library = tempfile::tempdir().unwrap();
    let staging_dir = tempfile::tempdir().unwrap();
    let mut conn = migrated();
    let (job_id, staging) = staged_job(&conn, staging_dir.path());
    let config = PromotionConfig::new(library.path());

    // Give the render its own embedded title, so the post-delete fallback has a
    // real value to read (the DB row seeds "Creep", but the fallback reads the
    // file, not the DB).
    crate::library::tag_edit::apply_patch_to_file(
        &staging.path_for_job(job_id),
        &crate::library::tag_edit::TagPatch {
            title: Some("Creep".to_string()),
            artist: Some("Radiohead".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // Force the copy to fail: occupy the destination with a directory so
    // `fs::copy` cannot write the file there.
    let destination = library
        .path()
        .join("Reprise Instrumentals")
        .join("Radiohead")
        .join("Creep (Instrumental).flac");
    std::fs::create_dir_all(&destination).unwrap();
    assert!(promote(&mut conn, &staging, &config, job_id, 100).is_err());

    // The original is deleted before the retry, so the retry must fall back to
    // the render's own embedded tags. Those must still read "Creep" — the first
    // attempt must not have mutated the staging render in place.
    conn.execute("DELETE FROM tracks WHERE id = 1", []).unwrap();
    std::fs::remove_dir_all(&destination).unwrap();

    let outcome = promote(&mut conn, &staging, &config, job_id, 200).unwrap();
    let tagged = lofty::read_from_path(&outcome.path).unwrap();
    assert_eq!(
        tagged.primary_tag().unwrap().title().as_deref(),
        Some("Creep (Instrumental)"),
        "exactly one instrumental suffix, never doubled"
    );
}

#[test]
fn instrumental_suffix_is_idempotent() {
    assert_eq!(with_instrumental_suffix("Creep"), "Creep (Instrumental)");
    // An already-suffixed title (e.g. re-read from a prior attempt) is left as-is.
    assert_eq!(
        with_instrumental_suffix("Creep (Instrumental)"),
        "Creep (Instrumental)"
    );
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

fn creep_source() -> SourceMeta {
    SourceMeta {
        title: "Creep".to_string(),
        artist: "Radiohead".to_string(),
        album: "Pablo Honey".to_string(),
        album_artist: "Radiohead".to_string(),
        year: Some(1993),
        track_no: Some(2),
        genre: "Alt Rock".to_string(),
    }
}

#[test]
fn resolve_destination_builds_the_documented_layout() {
    let conn = migrated();
    let config = PromotionConfig::new("/library");
    // No jobs exist, so nothing reserves the base name.
    assert_eq!(
        resolve_destination(&conn, &config, 1, &creep_source()).unwrap(),
        PathBuf::from("/library/Reprise Instrumentals/Radiohead/Creep (Instrumental).flac")
    );
}

#[test]
fn resolve_destination_reuses_the_owning_job_and_bumps_a_foreign_one() {
    let library = tempfile::tempdir().unwrap();
    let staging_dir = tempfile::tempdir().unwrap();
    let mut conn = migrated();
    let (jobs, staging) = seed_identical_pair(&conn, staging_dir.path());
    let config = PromotionConfig::new(library.path());

    // Promote A: its saved result now sits at the base path.
    let out_a = promote(&mut conn, &staging, &config, jobs[0], 100).unwrap();

    // The owning job re-resolves to its own file — a retry reuses it, no suffix.
    assert_eq!(
        resolve_destination(&conn, &config, jobs[0], &creep_source()).unwrap(),
        out_a.path
    );
    // A different job must not land on A's result; it is bumped deterministically.
    let for_b = resolve_destination(&conn, &config, jobs[1], &creep_source()).unwrap();
    assert_ne!(for_b, out_a.path);
    assert_eq!(
        for_b.file_name().unwrap().to_string_lossy(),
        "Creep (Instrumental) (2).flac"
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
