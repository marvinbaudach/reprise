use super::super::podcasts_footer::{REFRESH_LABEL_PAGE, REFRESH_SPINNER_PAGE};
use super::*;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_refresh_button_carries_a_spinner_while_a_fetch_runs_and_recovers_after_one() {
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    let runtime = PodcastsRuntime::setup(&conn);
    let view = PodcastsView::install(
        Rc::new(conn),
        runtime,
        PodcastsCallbacks::default(),
        PodcastKind::Rss,
    );

    view.begin_refresh_feedback();
    assert!(!view.refresh_button.is_sensitive());
    assert_eq!(
        view.refresh_stack.visible_child_name().as_deref(),
        Some(REFRESH_SPINNER_PAGE)
    );
    assert!(view.refresh_spinner.is_spinning());

    view.begin_refresh_feedback();
    view.end_refresh_feedback();
    assert!(!view.refresh_button.is_sensitive());
    assert_eq!(
        view.refresh_stack.visible_child_name().as_deref(),
        Some(REFRESH_SPINNER_PAGE)
    );
    assert!(view.refresh_spinner.is_spinning());

    view.end_refresh_feedback();
    assert!(view.refresh_button.is_sensitive());
    assert_eq!(
        view.refresh_stack.visible_child_name().as_deref(),
        Some(REFRESH_LABEL_PAGE)
    );
    assert!(!view.refresh_spinner.is_spinning());
}
