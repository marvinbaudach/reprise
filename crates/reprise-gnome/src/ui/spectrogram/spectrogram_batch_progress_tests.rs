use super::*;

fn running(analyzed: usize, total: usize) -> SpectrogramBatchProgress {
    SpectrogramBatchProgress {
        state: SpectrogramBatchState::Running,
        analyzed,
        total,
        failed: 0,
    }
}

#[test]
fn an_idle_batch_shows_no_card_at_all() {
    let presentation = presentation(SpectrogramBatchProgress::idle());

    assert!(!presentation.visible);
    assert!(!presentation.auto_hide);
}

/// The analysis starts on every launch, so the overwhelmingly common case is a
/// library that is already done. That run must leave no trace on screen.
#[test]
fn nav_7b_an_autostarted_run_with_nothing_to_do_shows_no_card() {
    assert!(!presentation(running(0, 0)).visible);
    assert!(
        !presentation(SpectrogramBatchProgress {
            state: SpectrogramBatchState::Complete,
            analyzed: 0,
            total: 0,
            failed: 0,
        })
        .visible
    );
}

#[test]
fn a_running_batch_shows_where_it_is_and_stays_on_screen() {
    let presentation = presentation(running(412, 1846));

    assert!(presentation.visible);
    assert!(!presentation.auto_hide);
    assert_eq!(presentation.title, "Analyzing library…");
    assert_eq!(presentation.detail, "412 of 1846 analyzed");
    assert!((presentation.fraction - 412.0 / 1846.0).abs() < f64::EPSILON);
}

#[test]
fn a_finished_batch_reports_what_it_achieved_and_hides_itself() {
    let presentation = presentation(SpectrogramBatchProgress {
        state: SpectrogramBatchState::Complete,
        analyzed: 1840,
        total: 1846,
        failed: 6,
    });

    assert!(presentation.visible);
    assert!(presentation.auto_hide);
    assert_eq!(presentation.title, "Library analysis complete");
    assert_eq!(presentation.detail, "1840 analyzed · 6 failed");
}

#[test]
fn nav_7b_a_stopped_run_says_so_rather_than_claiming_completion() {
    let presentation = presentation(SpectrogramBatchProgress {
        state: SpectrogramBatchState::Stopped,
        analyzed: 200,
        total: 1846,
        failed: 0,
    });

    assert_eq!(presentation.title, "Library analysis stopped");
    assert!(presentation.auto_hide);
}

#[test]
fn a_failed_run_is_named_as_a_failure() {
    let presentation = presentation(SpectrogramBatchProgress {
        state: SpectrogramBatchState::Failed,
        analyzed: 0,
        total: 0,
        failed: 0,
    });

    assert_eq!(presentation.title, "Could not analyze the library");
    assert!(presentation.visible);
}

#[test]
fn the_fraction_never_leaves_the_bar() {
    assert!((presentation(running(9, 4)).fraction - 1.0).abs() < f64::EPSILON);
    assert!((presentation(running(0, 0)).fraction - 0.0).abs() < f64::EPSILON);
}
