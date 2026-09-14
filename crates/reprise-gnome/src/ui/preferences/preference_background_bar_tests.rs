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
fn fb_9_the_dialog_reports_every_running_task_once() {
    let running = bar_state(&[Some(artwork(0.91)), Some(lyrics(0.12))], true, true);

    assert_eq!(
        running.row_count(),
        3,
        "scan, Artwork and Lyrics each own one row"
    );
    assert_eq!(running.count_badge.as_deref(), Some("3"));
    assert_eq!(running.empty_notice, None);

    let idle = bar_state(&[None, None], true, false);
    assert_eq!(idle.row_count(), 0);
    assert_eq!(idle.count_badge, None);
    assert_eq!(idle.empty_notice.as_deref(), Some("No background activity"));

    let scan_with_online_off = bar_state(&[None, None], false, true);
    assert_eq!(scan_with_online_off.row_count(), 1);
    assert_eq!(scan_with_online_off.count_badge.as_deref(), Some("1"));
    assert_eq!(scan_with_online_off.empty_notice, None);
}

#[test]
fn two_running_jobs_stand_side_by_side_instead_of_sharing_one_slot() {
    let state = bar_state(&[Some(artwork(0.91)), Some(lyrics(0.12))], true, false);

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
    let state = bar_state(&[None, Some(lyrics(0.12))], true, false);

    assert_eq!(state.rows, vec![lyrics(0.12)]);
    assert_eq!(state.count_badge.as_deref(), Some("1"));
}

#[test]
fn nothing_running_keeps_the_resting_notice() {
    let state = bar_state(&[None, None], true, false);

    assert!(state.rows.is_empty());
    assert_eq!(state.count_badge, None);
    assert_eq!(
        state.empty_notice.as_deref(),
        Some("No background activity")
    );

    let disabled = bar_state(&[None, None], false, false);
    assert_eq!(
        disabled.empty_notice.as_deref(),
        Some("No background activity")
    );
}

#[test]
fn the_gate_being_off_replaces_activity_with_one_reason() {
    let state = bar_state(&[Some(artwork(0.91)), Some(lyrics(0.12))], false, false);

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
fn the_idle_footer_widget_keeps_its_resting_notice() {
    gtk4::init().unwrap();
    let bar = BackgroundBar::new();

    assert!(bar.widget().is_visible());
    bar.publish(JobOwner::Artwork, Some(artwork(0.25)));
    assert!(bar.widget().is_visible());
    bar.publish(JobOwner::Artwork, None);
    assert!(bar.widget().is_visible());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_18_adopting_the_scan_chrome_does_not_hold_the_idle_footer_open() {
    gtk4::init().unwrap();
    let bar = BackgroundBar::new();
    // Stand-ins for the scan line and chip: both start hidden and hide
    // themselves again when no scan runs, exactly as `ScanChromeView` builds
    // them — without its fade, which is time-gated and not what is under test.
    let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    line.set_visible(false);
    let chip = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    chip.set_visible(false);
    bar.adopt_scan_chrome(line.upcast_ref(), chip.upcast_ref());

    assert!(
        bar.widget().get_visible(),
        "the footer keeps its resting notice under every page"
    );
    assert!(
        !chip.has_css_class("scan-chip"),
        "the floating chip surface is removed"
    );

    chip.set_visible(true);
    assert!(
        bar.widget().get_visible(),
        "a running scan brings the footer back"
    );

    chip.set_visible(false);
    assert!(
        bar.widget().get_visible(),
        "and the footer returns to its resting notice once scanning stops"
    );

    bar.publish(JobOwner::Artwork, Some(artwork(0.25)));
    assert!(
        bar.widget().get_visible(),
        "a plugin job opens the footer on its own"
    );
    bar.publish(JobOwner::Artwork, None);
    assert!(bar.widget().get_visible());
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
