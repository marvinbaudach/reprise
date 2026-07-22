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

/// Enqueues one instrumental job (batch API) with the given save-intent, claims
/// it as worker 5, and produces a real FLAC render in staging via the fake
/// backend — leaving the job `running` and ready for `complete_render`.
fn running_job_with_render(
    conn: &Connection,
    staging_dir: &Path,
    staging: &StagingStore,
    auto_promote: bool,
) -> i64 {
    staging.ensure_dir().unwrap();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, album_artist, added_at, file_mtime, file_size) \
         VALUES (1, '/music/creep.flac', 'Creep', 'Radiohead', 'Pablo Honey', 'Radiohead', 1, 1, 1)",
        [],
    )
    .unwrap();
    let job_id = ai_jobs::enqueue_instrumental_batch(
        conn,
        staging,
        &[1],
        crate::stem_separation::CURRENT_MODEL_ID,
        auto_promote,
        0,
    )
    .unwrap()
    .jobs[0]
        .job_id();
    let claimed = ai_jobs::claim_next(conn, 5, 0, 60).unwrap().unwrap();
    assert_eq!(claimed.id, job_id);
    // The fake backend copies a real FLAC into the staging render path.
    use crate::stem_separation::StemSeparationBackend;
    let source = flac_fixture(staging_dir, "backend-src.flac");
    crate::stem_separation::FakeStemBackend::new()
        .separate_instrumental(&source, &staging.path_for_job(job_id), &mut |_| {}, &|| {
            false
        })
        .unwrap();
    job_id
}

#[test]
fn complete_render_auto_promotes_when_the_intent_is_set() {
    let library = tempfile::tempdir().unwrap();
    let staging_dir = tempfile::tempdir().unwrap();
    let mut conn = migrated();
    let staging = StagingStore::new(staging_dir.path());
    let config = PromotionConfig::new(library.path());
    let job_id = running_job_with_render(&conn, staging_dir.path(), &staging, true);

    let outcome = complete_render(&mut conn, &staging, &config, job_id, 5, 100).unwrap();

    let promoted = match outcome {
        CompletionOutcome::Promoted(promoted) => promoted,
        other => panic!("expected Promoted, got {other:?}"),
    };
    assert!(promoted.path.is_file());
    let provenance = crate::provenance::get_provenance(&conn, promoted.result_track_id)
        .unwrap()
        .unwrap();
    assert!(provenance.ai);
    assert_eq!(
        provenance.model.as_deref(),
        Some(crate::stem_separation::CURRENT_MODEL_ID)
    );
    let job = ai_jobs::get_job(&conn, job_id).unwrap().unwrap();
    assert_eq!(job.state, ai_jobs::JobState::Done);
    assert_eq!(job.result_track_id, Some(promoted.result_track_id));
    assert!(
        !staging.exists(job_id),
        "the promoted render leaves staging"
    );
}

#[test]
fn complete_render_leaves_a_no_intent_job_staged() {
    let library = tempfile::tempdir().unwrap();
    let staging_dir = tempfile::tempdir().unwrap();
    let mut conn = migrated();
    let staging = StagingStore::new(staging_dir.path());
    let config = PromotionConfig::new(library.path());
    let job_id = running_job_with_render(&conn, staging_dir.path(), &staging, false);

    let outcome = complete_render(&mut conn, &staging, &config, job_id, 5, 100).unwrap();

    assert_eq!(outcome, CompletionOutcome::Staged);
    let job = ai_jobs::get_job(&conn, job_id).unwrap().unwrap();
    assert_eq!(job.state, ai_jobs::JobState::Done);
    assert!(job.result_track_id.is_none(), "no intent => stays staged");
    assert!(
        staging.exists(job_id),
        "the render waits in staging for a manual save"
    );
}

#[test]
fn complete_render_keeps_a_failed_auto_promotion_retryable() {
    let library = tempfile::tempdir().unwrap();
    let staging_dir = tempfile::tempdir().unwrap();
    let mut conn = migrated();
    let staging = StagingStore::new(staging_dir.path());
    let config = PromotionConfig::new(library.path());
    let job_id = running_job_with_render(&conn, staging_dir.path(), &staging, true);

    // Force the auto-promotion's copy to fail with a directory at the target.
    let destination = config
        .instrumentals_root()
        .join("Radiohead")
        .join("Creep (Instrumental).flac");
    std::fs::create_dir_all(&destination).unwrap();

    let outcome = complete_render(&mut conn, &staging, &config, job_id, 5, 100).unwrap();
    assert!(matches!(
        outcome,
        CompletionOutcome::PromotionDeferred { .. }
    ));

    let job = ai_jobs::get_job(&conn, job_id).unwrap().unwrap();
    assert_eq!(job.state, ai_jobs::JobState::Done);
    assert!(
        job.result_track_id.is_none(),
        "a failed promotion leaves the job unsaved"
    );
    assert!(job.error_kind.is_some(), "the promotion error is noted");
    assert!(
        staging.exists(job_id),
        "the render stays in staging for a retry"
    );

    // Retryable: clear the blocker and promote directly.
    std::fs::remove_dir_all(&destination).unwrap();
    let retried = promote(&mut conn, &staging, &config, job_id, 200).unwrap();
    assert!(retried.path.is_file());
    let job = ai_jobs::get_job(&conn, job_id).unwrap().unwrap();
    assert_eq!(job.result_track_id, Some(retried.result_track_id));
}

// --- complete_render_with_publish (owner-guarded temp -> canonical publish) ---

/// Seeds source track 1, enqueues one job with `auto_promote`, and claims it as
/// `worker` at `now` (lease 60) — leaving it `running`, but with NO render yet.
fn seed_and_claim(
    conn: &Connection,
    staging: &StagingStore,
    worker: i64,
    auto_promote: bool,
    now: i64,
) -> i64 {
    staging.ensure_dir().unwrap();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, album_artist, added_at, file_mtime, file_size) \
         VALUES (1, '/music/creep.flac', 'Creep', 'Radiohead', 'Pablo Honey', 'Radiohead', 1, 1, 1)",
        [],
    )
    .unwrap();
    let job_id = ai_jobs::enqueue_instrumental_batch(
        conn,
        staging,
        &[1],
        crate::stem_separation::CURRENT_MODEL_ID,
        auto_promote,
        0,
    )
    .unwrap()
    .jobs[0]
        .job_id();
    let claimed = ai_jobs::claim_next(conn, worker, now, 60).unwrap().unwrap();
    assert_eq!(claimed.id, job_id);
    job_id
}

/// Renders a real FLAC into the claim-scoped **temp** path for `worker` — the
/// way a worker renders before publishing. Returns the temp path.
fn render_temp(staging_dir: &Path, staging: &StagingStore, job_id: i64, worker: i64) -> PathBuf {
    use crate::stem_separation::StemSeparationBackend;
    let source = flac_fixture(staging_dir, &format!("src-{worker}.flac"));
    let temp = staging.temp_path_for_job(job_id, worker);
    crate::stem_separation::FakeStemBackend::new()
        .separate_instrumental(&source, &temp, &mut |_| {}, &|| false)
        .unwrap();
    temp
}

#[test]
fn complete_render_with_publish_publishes_the_temp_render_when_owned() {
    let staging_dir = tempfile::tempdir().unwrap();
    let mut conn = migrated();
    let staging = StagingStore::new(staging_dir.path());
    let job_id = seed_and_claim(&conn, &staging, 5, false, 0);
    let temp = render_temp(staging_dir.path(), &staging, job_id, 5);
    assert!(
        temp.is_file() && !staging.exists(job_id),
        "the render sits in the temp path, not yet at its canonical path"
    );

    let outcome =
        complete_render_with_publish(&mut conn, &staging, None, job_id, 5, &temp, 100).unwrap();

    assert_eq!(outcome, CompletionOutcome::Staged);
    assert!(
        staging.exists(job_id),
        "the temp render is published to its canonical staging path"
    );
    assert!(!temp.exists(), "the temp file is consumed by the publish");
    let job = ai_jobs::get_job(&conn, job_id).unwrap().unwrap();
    assert_eq!(job.state, ai_jobs::JobState::Done);
    assert!(job.result_track_id.is_none(), "no root => stays staged");
}

#[test]
fn complete_render_with_publish_promotes_the_published_render_when_intent_set() {
    let library = tempfile::tempdir().unwrap();
    let staging_dir = tempfile::tempdir().unwrap();
    let mut conn = migrated();
    let staging = StagingStore::new(staging_dir.path());
    let config = PromotionConfig::new(library.path());
    let job_id = seed_and_claim(&conn, &staging, 5, true, 0);
    let temp = render_temp(staging_dir.path(), &staging, job_id, 5);

    let outcome =
        complete_render_with_publish(&mut conn, &staging, Some(&config), job_id, 5, &temp, 100)
            .unwrap();

    let promoted = match outcome {
        CompletionOutcome::Promoted(promoted) => promoted,
        other => panic!("expected Promoted, got {other:?}"),
    };
    assert!(promoted.path.is_file());
    assert!(!temp.exists(), "the temp file is consumed by the publish");
    assert!(
        !staging.exists(job_id),
        "the promoted render is filed and leaves staging"
    );
    let job = ai_jobs::get_job(&conn, job_id).unwrap().unwrap();
    assert_eq!(job.result_track_id, Some(promoted.result_track_id));
}

#[test]
fn a_straggler_after_discard_never_resurrects_the_staging_render() {
    // The reviewer's trace, made deterministic via the injected clock/lease:
    // worker A's lease is reclaimed by B mid-render; B publishes the render and
    // the user discards it; A then finishes and tries to complete. A MUST fail
    // the ownership guard and delete its own temp, never touching the canonical
    // staging path — so no permanent, unlisted, un-GC'd orphan is resurrected.
    let staging_dir = tempfile::tempdir().unwrap();
    let mut conn = migrated();
    let staging = StagingStore::new(staging_dir.path());

    // A (5) claims at t=0 with a 60 s lease; B (6) reclaims at t=100 once it
    // expires (claim_next reclaims a running job whose lease_expires_at < now).
    let job_id = seed_and_claim(&conn, &staging, 5, false, 0);
    let reclaimed = ai_jobs::claim_next(&conn, 6, 100, 60).unwrap().unwrap();
    assert_eq!(reclaimed.id, job_id, "B reclaims A's expired lease");

    // B renders and completes: the render is published to its canonical path.
    let temp_b = render_temp(staging_dir.path(), &staging, job_id, 6);
    let done =
        complete_render_with_publish(&mut conn, &staging, None, job_id, 6, &temp_b, 100).unwrap();
    assert_eq!(done, CompletionOutcome::Staged);
    assert!(staging.exists(job_id), "B's render is committed");

    // The user discards it: the canonical render is gone, the job is terminal.
    ai_jobs::discard_staged(&conn, &staging, job_id, 150).unwrap();
    assert!(!staging.exists(job_id), "the discarded render is removed");

    // The straggler A finally finishes and tries to complete with its own temp.
    let temp_a = render_temp(staging_dir.path(), &staging, job_id, 5);
    let outcome =
        complete_render_with_publish(&mut conn, &staging, None, job_id, 5, &temp_a, 300).unwrap();

    assert_eq!(
        outcome,
        CompletionOutcome::NotOwned,
        "A no longer owns the reclaimed-then-terminal job"
    );
    assert!(
        !staging.exists(job_id),
        "the discarded render is NOT resurrected by the straggler"
    );
    assert!(
        !temp_a.exists(),
        "the straggler deletes its own worthless temp"
    );
}
