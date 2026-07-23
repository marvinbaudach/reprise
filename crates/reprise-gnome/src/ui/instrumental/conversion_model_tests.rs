//! Headless tests for the conversion view-model — row states, the play/wait
//! decision (INST-5), and the aggregate figure.

use reprise_core::ai_jobs::{AiJob, JobState};

use super::*;

fn job(id: i64, state: JobState, permille: u16, result: Option<i64>) -> AiJob {
    AiJob {
        id,
        kind: "instrumental".to_string(),
        batch_id: Some("batch".to_string()),
        source_track_id: Some(id),
        params_fingerprint: "model@1".to_string(),
        state,
        progress_permille: permille,
        cancel_requested: false,
        error_kind: None,
        result_track_id: result,
        created_at: 0,
        finished_at: None,
    }
}

// UX INST-5a: the view-model resolves a click on a still-processing row to
// wait-with-progress — never Play (no original fallback), never a skip.
#[test]
fn inst_5a_click_on_a_processing_row_waits_with_progress_never_plays() {
    let processing = RowState::Processing { permille: 300 };
    assert_eq!(click_action(processing), RowClickAction::WaitWithProgress);
    assert_ne!(
        click_action(processing),
        RowClickAction::Play,
        "a processing row must never fall back to playing anything"
    );
    // The contrast: a finished, playable render plays immediately (INST-4).
    assert_eq!(
        click_action(RowState::DoneUnsaved { playable: true }),
        RowClickAction::Play
    );
    assert_eq!(click_action(RowState::Saved), RowClickAction::Play);
    // Queued and failed rows neither play nor wait.
    assert_eq!(click_action(RowState::Queued), RowClickAction::Inert);
    assert_eq!(click_action(RowState::Failed), RowClickAction::Inert);
    // A finished render whose staging file vanished is not playable.
    assert_eq!(
        click_action(RowState::DoneUnsaved { playable: false }),
        RowClickAction::Inert
    );
}

#[test]
fn row_state_maps_job_row_and_staging_presence() {
    assert_eq!(
        row_state(&job(1, JobState::Queued, 0, None), false),
        RowState::Queued
    );
    assert_eq!(
        row_state(&job(2, JobState::Running, 420, None), false),
        RowState::Processing { permille: 420 }
    );
    // Done + no result + render present => playable unsaved; render gone => not.
    assert_eq!(
        row_state(&job(3, JobState::Done, 1000, None), true),
        RowState::DoneUnsaved { playable: true }
    );
    assert_eq!(
        row_state(&job(3, JobState::Done, 1000, None), false),
        RowState::DoneUnsaved { playable: false }
    );
    // Done + a promoted track id => saved (INST-6 row switch).
    assert_eq!(
        row_state(&job(4, JobState::Done, 1000, Some(99)), false),
        RowState::Saved
    );
    assert_eq!(
        row_state(&job(5, JobState::Failed, 250, None), false),
        RowState::Failed
    );
}

#[test]
fn is_playable_covers_only_finished_renders() {
    assert!(is_playable(RowState::Saved));
    assert!(is_playable(RowState::DoneUnsaved { playable: true }));
    assert!(!is_playable(RowState::DoneUnsaved { playable: false }));
    assert!(!is_playable(RowState::Processing { permille: 999 }));
    assert!(!is_playable(RowState::Queued));
    assert!(!is_playable(RowState::Failed));
}

#[test]
fn aggregate_counts_done_and_averages_completion() {
    // Two done (1000 each), one half-done running (500), one queued (0):
    // done=2, total=4, mean permille = (1000+1000+500+0)/4 = 625.
    let jobs = [
        job(1, JobState::Done, 1000, None),
        job(2, JobState::Done, 1000, Some(7)),
        job(3, JobState::Running, 500, None),
        job(4, JobState::Queued, 0, None),
    ];
    let aggregate = aggregate(&jobs);
    assert_eq!(aggregate.done, 2);
    assert_eq!(aggregate.total, 4);
    assert_eq!(aggregate.permille, 625);
    assert_eq!(aggregate.percent(), 63);
    assert!((aggregate.fraction() - 0.625).abs() < 1e-9);
}

#[test]
fn aggregate_of_an_empty_list_is_zero() {
    assert_eq!(aggregate(&[]), Aggregate::default());
    assert_eq!(Aggregate::default().fraction(), 0.0);
}

#[test]
fn has_undecided_detects_a_finished_unsaved_render() {
    assert!(has_undecided(&[job(1, JobState::Done, 1000, None)]));
    assert!(!has_undecided(&[job(1, JobState::Done, 1000, Some(3))]));
    assert!(!has_undecided(&[job(1, JobState::Running, 500, None)]));
    assert!(!has_undecided(&[]));
}

// UX INST-3: every job row maps to exactly one visible state — queued /
// processing / done-unsaved / saved / failed — and the five are mutually
// distinct, so the view shows a distinct state per row.
#[test]
fn inst_3_each_job_row_maps_to_its_distinct_visible_state() {
    assert_eq!(
        row_state(&job(1, JobState::Queued, 0, None), false),
        RowState::Queued
    );
    assert_eq!(
        row_state(&job(2, JobState::Running, 300, None), false),
        RowState::Processing { permille: 300 }
    );
    assert_eq!(
        row_state(&job(3, JobState::Done, 1000, None), true),
        RowState::DoneUnsaved { playable: true }
    );
    assert_eq!(
        row_state(&job(4, JobState::Done, 1000, Some(9)), false),
        RowState::Saved
    );
    assert_eq!(
        row_state(&job(5, JobState::Failed, 250, None), false),
        RowState::Failed
    );
    let states = [
        RowState::Queued,
        RowState::Processing { permille: 300 },
        RowState::DoneUnsaved { playable: true },
        RowState::Saved,
        RowState::Failed,
    ];
    for (i, a) in states.iter().enumerate() {
        for (j, b) in states.iter().enumerate() {
            assert_eq!(
                i == j,
                a == b,
                "the visible states must be mutually distinct"
            );
        }
    }
}

// UX INST-7: "Clear playlist" warns exactly when an undecided (done, unsaved)
// render exists — the predicate the confirmation dialog keys on so hours of
// compute are never discarded unconfirmed.
#[test]
fn inst_7_clear_warns_exactly_when_an_undecided_render_exists() {
    assert!(has_undecided(&[job(1, JobState::Done, 1000, None)]));
    assert!(!has_undecided(&[job(1, JobState::Done, 1000, Some(3))]));
    assert!(!has_undecided(&[job(1, JobState::Running, 500, None)]));
    assert!(!has_undecided(&[job(1, JobState::Failed, 0, None)]));
    assert!(!has_undecided(&[]));
    // One undecided among saved/running still warns.
    assert!(has_undecided(&[
        job(1, JobState::Done, 1000, Some(3)),
        job(2, JobState::Running, 500, None),
        job(3, JobState::Done, 1000, None),
    ]));
}

// UX INST-9: re-adding an already-converted track produces a dedup hint, not a
// second job — the core facade deduplicates and the toast communicates the skip.
#[test]
fn inst_9_already_converted_track_yields_a_dedup_hint_not_a_double_job() {
    use reprise_core::ai_jobs::EnqueueOutcome;
    use reprise_core::ai_staging::StagingStore;

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, added_at) \
         VALUES (1, '/music/1.flac', 'Song', 'Artist', 'Album', 0)",
        [],
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let staging = StagingStore::new(dir.path().join("staging"));

    let first =
        reprise_core::ai_conversion::add_to_conversion(&conn, &staging, 1, "model@1", 0).unwrap();
    assert!(
        matches!(first, EnqueueOutcome::Created { .. }),
        "the first add creates a job"
    );
    let second =
        reprise_core::ai_conversion::add_to_conversion(&conn, &staging, 1, "model@1", 0).unwrap();
    assert!(
        matches!(second, EnqueueOutcome::Deduplicated { .. }),
        "re-adding the same track deduplicates, it does not create a second job"
    );

    // The toast surfaces the skip as a hint (0 created, 1 already existing).
    let hint = crate::ui::strings::create_instrumental_toast(0, 1);
    assert!(
        hint.contains("already exist"),
        "the toast hints at the dedup: {hint:?}"
    );
    assert!(
        !crate::ui::strings::create_instrumental_toast(1, 0).contains("already"),
        "a pure create shows no false dedup hint"
    );
}
