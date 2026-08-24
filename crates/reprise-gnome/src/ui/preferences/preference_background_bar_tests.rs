use super::*;
use crate::ui::cover_download_batch::{BatchProgress as CoverProgress, BatchState as CoverState};
use crate::ui::lyrics_batch::{LyricsBatchProgress, LyricsBatchState};

fn artwork(fraction: f64) -> JobRowState {
    JobRowState {
        owner: JobOwner::Artwork,
        detail: "Album covers · 1942 of 2132".to_owned(),
        fraction,
    }
}

fn lyrics(fraction: f64) -> JobRowState {
    JobRowState {
        owner: JobOwner::OnlineLyrics,
        detail: "Missing lyrics · 261 of 2132".to_owned(),
        fraction,
    }
}

#[test]
fn two_running_jobs_stand_side_by_side_instead_of_sharing_one_slot() {
    let state = bar_state(&[Some(artwork(0.91)), Some(lyrics(0.12))], true);

    assert_eq!(
        state.rows.iter().map(|row| row.owner).collect::<Vec<_>>(),
        vec![JobOwner::Artwork, JobOwner::OnlineLyrics],
        "the lyrics check must be visible while Artwork is still running"
    );
    assert_eq!(state.count_badge.as_deref(), Some("2"));
    assert_eq!(state.empty_notice, None);
}

#[test]
fn a_job_keeps_its_own_row_when_the_other_one_stops() {
    let state = bar_state(&[None, Some(lyrics(0.12))], true);

    assert_eq!(state.rows, vec![lyrics(0.12)]);
    assert_eq!(state.count_badge.as_deref(), Some("1"));
}

#[test]
fn nothing_running_shows_no_badge_and_no_notice() {
    let state = bar_state(&[None, None], true);

    assert!(state.rows.is_empty());
    assert_eq!(state.count_badge, None);
    assert_eq!(state.empty_notice, None);
}

#[test]
fn the_gate_being_off_replaces_every_row_with_one_reason() {
    let state = bar_state(&[Some(artwork(0.91)), Some(lyrics(0.12))], false);

    assert!(state.rows.is_empty());
    assert_eq!(state.count_badge, None);
    assert_eq!(
        state.empty_notice.as_deref(),
        Some("No online jobs — Online content is off")
    );
}

#[test]
fn every_row_is_named_after_the_plugin_that_started_it() {
    assert_eq!(JobOwner::Artwork.title(), "Artwork");
    assert_eq!(JobOwner::OnlineLyrics.title(), "Online Lyrics");
}

#[test]
fn only_a_running_batch_is_background_activity() {
    let running = CoverProgress {
        state: CoverState::Running,
        checked: 1942,
        total: 2132,
        downloaded: 0,
        unavailable: 0,
    };

    let job = artwork_job(running).expect("a running cover batch is a job");
    assert_eq!(job.owner, JobOwner::Artwork);
    assert_eq!(job.detail, "Album covers · 1942 of 2132");
    assert_eq!(job.percent(), 91);

    for state in [CoverState::Idle, CoverState::Complete, CoverState::Failed] {
        assert_eq!(
            artwork_job(CoverProgress { state, ..running }),
            None,
            "{state:?} is not activity"
        );
    }
}

#[test]
fn the_lyrics_batch_reports_its_own_counts_under_its_own_name() {
    let running = LyricsBatchProgress {
        state: LyricsBatchState::Running,
        checked: 261,
        total: 2132,
        downloaded: 0,
        unavailable: 0,
        failed: 0,
    };

    let job = lyrics_job(running).expect("a running lyrics batch is a job");
    assert_eq!(job.owner, JobOwner::OnlineLyrics);
    assert_eq!(job.detail, "Missing lyrics · 261 of 2132");
    assert_eq!(job.percent(), 12);
    assert_eq!(
        lyrics_job(LyricsBatchProgress {
            state: LyricsBatchState::Idle,
            ..running
        }),
        None
    );
}

#[test]
fn the_percent_column_rounds_and_clamps_so_the_row_cannot_jump() {
    assert_eq!(artwork(0.386).percent(), 39);
    assert_eq!(artwork(2.0).percent(), 100);
    assert_eq!(artwork(-1.0).percent(), 0);
}

#[test]
fn the_footer_paints_from_named_colours_and_a_flat_bar() {
    let css = css();

    assert!(css.contains(&format!(".{BAR_CLASS} {{")));
    assert!(css.contains("background-color: @sidebar_bg_color"));
    // No animation, no stripes, no pulsing: one flat accent fill.
    assert!(css.contains("background-image: none"));
    assert!(css.contains("background-color: @accent_color"));
    assert!(!css.contains("animation"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn a_cancel_button_only_cancels_its_own_job() {
    use std::cell::RefCell;

    gtk4::init().unwrap();
    let bar = BackgroundBar::new();
    let cancelled: Rc<RefCell<Vec<JobOwner>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let cancelled = cancelled.clone();
        bar.set_on_cancel(move |owner| cancelled.borrow_mut().push(owner));
    }
    bar.publish(JobOwner::Artwork, Some(artwork(0.91)));
    bar.publish(JobOwner::OnlineLyrics, Some(lyrics(0.12)));

    let buttons = cancel_buttons(bar.widget());
    assert_eq!(buttons.len(), 2, "every job row carries its own cancel");
    buttons[1].emit_clicked();

    assert_eq!(*cancelled.borrow(), vec![JobOwner::OnlineLyrics]);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_footer_css_parses_without_gtk_errors() {
    gtk4::init().unwrap();
    let errors = crate::ui::style::css_parse_errors(&css());
    assert!(
        errors.is_empty(),
        "GTK reported CSS parsing errors: {errors:?}"
    );
}

#[cfg(test)]
fn cancel_buttons(root: &gtk4::Widget) -> Vec<gtk4::Button> {
    let mut found = Vec::new();
    if let Ok(button) = root.clone().downcast::<gtk4::Button>() {
        found.push(button);
        return found;
    }
    let mut child = root.first_child();
    while let Some(current) = child {
        found.extend(cancel_buttons(&current));
        child = current.next_sibling();
    }
    found
}
