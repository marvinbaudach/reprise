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
