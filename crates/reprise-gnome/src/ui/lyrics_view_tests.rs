use gtk4::prelude::*;
use reprise_core::lyrics::{LyricsBody, LyricsError, TimedLine};

use super::lyrics_view::{centered_scroll_value, LyricsView, ACTIVE_LINE_CLASS};

#[test]
fn centered_scroll_clamps_at_start_middle_and_end() {
    assert_eq!(centered_scroll_value(10.0, 40.0, 200.0, 1_000.0), 0.0);
    assert_eq!(centered_scroll_value(450.0, 50.0, 200.0, 1_000.0), 375.0);
    assert_eq!(centered_scroll_value(950.0, 50.0, 200.0, 1_000.0), 800.0);
    assert_eq!(centered_scroll_value(50.0, 20.0, 400.0, 200.0), 0.0);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn lyrics_bodies_are_selectable_and_only_one_timed_line_is_active() {
    gtk4::init().unwrap();
    let view = LyricsView::new();
    view.show_result(&LyricsBody::Synced(vec![
        TimedLine::new(1_000, "first synthetic line"),
        TimedLine::new(2_000, "second synthetic line"),
        TimedLine::new(3_000, "third synthetic line"),
    ]));
    let labels = view.line_labels();
    assert_eq!(labels.len(), 3);
    assert!(labels.iter().all(gtk4::Label::is_selectable));
    assert!(labels.iter().all(gtk4::Label::wraps));

    view.set_active_line(Some(1));
    assert!(!labels[0].has_css_class(ACTIVE_LINE_CLASS));
    assert!(labels[1].has_css_class(ACTIVE_LINE_CLASS));
    assert!(!labels[2].has_css_class(ACTIVE_LINE_CLASS));
    view.set_active_line(Some(2));
    assert!(!labels[1].has_css_class(ACTIVE_LINE_CLASS));
    assert!(labels[2].has_css_class(ACTIVE_LINE_CLASS));
    view.set_active_line(Some(2));
    assert_eq!(
        labels
            .iter()
            .filter(|label| label.has_css_class(ACTIVE_LINE_CLASS))
            .count(),
        1
    );

    view.show_loading("Another synthetic title", "Synthetic artist");
    assert!(view.line_labels().is_empty());

    view.show_result(&LyricsBody::Plain("synthetic plain text".into()));
    let plain = view.line_labels();
    assert_eq!(plain.len(), 1);
    assert!(plain[0].is_selectable());
    assert!(plain[0].wraps());

    view.show_result(&LyricsBody::Instrumental);
    assert_eq!(view.visible_state_name().as_deref(), Some("status"));
    assert_eq!(view.status_text(), "Instrumental");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn temporary_error_exposes_retry_but_not_found_does_not() {
    gtk4::init().unwrap();
    let view = LyricsView::new();
    let _widget = view.widget();
    view.show_loading("Synthetic title", "Synthetic artist");
    assert_eq!(view.visible_state_name().as_deref(), Some("loading"));
    view.set_on_retry(|| {});
    view.show_error(&LyricsError::Temporary);
    assert!(view.retry_is_visible());
    view.show_error(&LyricsError::NotFound);
    assert!(!view.retry_is_visible());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn active_lines_center_and_clamp_in_a_mapped_panel() {
    gtk4::init().unwrap();
    let view = LyricsView::new();
    let lines = (0..40)
        .map(|index| {
            TimedLine::new(
                i64::from(index) * 1_000,
                format!("synthetic line {index} with enough words for stable height"),
            )
        })
        .collect();
    view.show_result(&LyricsBody::Synced(lines));
    let window = gtk4::Window::builder()
        .default_width(340)
        .default_height(240)
        .child(view.widget())
        .build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    view.set_active_line(Some(20));
    while gtk4::glib::MainContext::default().iteration(false) {}
    let (middle, maximum) = view.scroll_values();
    assert!(middle > 0.0 && middle < maximum);

    view.set_active_line(Some(0));
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert_eq!(view.scroll_values().0, 0.0);

    view.set_active_line(Some(39));
    while gtk4::glib::MainContext::default().iteration(false) {}
    let (end, maximum) = view.scroll_values();
    assert!((end - maximum).abs() < 1.0);
    window.close();
}
