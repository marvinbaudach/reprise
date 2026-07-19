use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::lyrics::{LyricsBody, LyricsError, TimedLine};

use super::lyrics_scroll::{content_margins, ManualScrollTimer, ScrollMode};
use super::lyrics_view::{
    active_line_alpha, centered_scroll_value, css, line_alpha, lyrics_footer, LyricsView,
    ACTIVE_LINE_CLASS, INLINE_RETRY_CLASS,
};

/// Pumps the main loop until `predicate` holds or the deadline passes.
/// A single non-blocking iteration does not let GTK allocate a freshly
/// presented window, so geometry assertions raced the layout.
fn settle_until(milliseconds: u64, mut predicate: impl FnMut() -> bool) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(milliseconds);
    loop {
        if predicate() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        gtk4::glib::MainContext::default().iteration(false);
    }
}

#[test]
fn npp_5_line_hierarchy_uses_the_decided_alpha_steps() {
    assert_eq!(line_alpha(Some(3), 3), 100);
    assert_eq!(line_alpha(Some(3), 2), 45);
    assert_eq!(line_alpha(Some(3), 4), 45);
    assert_eq!(line_alpha(Some(3), 1), 32);
    assert_eq!(line_alpha(Some(3), 5), 32);
    assert_eq!(line_alpha(Some(3), 0), 28);
    assert_eq!(line_alpha(None, 3), 28);

    let css = css();
    for declaration in [
        "font-size: 13px",
        "font-size: 15px",
        "font-weight: 700",
        "min-width: 26px",
        "min-height: 2.5px",
        "background-color: @reprise_player_accent",
    ] {
        assert!(css.contains(declaration), "missing {declaration}");
    }
}

#[test]
fn npp_6_line_changes_use_the_micro_fade_token() {
    let css = css();
    assert!(css.contains(&format!(
        "transition: opacity {}ms {}",
        crate::ui::motion::MICRO_MS,
        crate::ui::motion::MICRO_CSS_EASING
    )));
    // NPP-8 scopes the hover to lines where clicking actually seeks, so the
    // rule must exclude both unsynced and active lines. Asserting the
    // exclusions rather than the whole literal keeps this honest: applying it
    // to every line dimmed the active line and let the accent glow bleed
    // through unsynced text (which read as brown on a warm cover).
    assert!(css.contains(":not(.lyrics-unsynced)"));
    assert!(css.contains(":not(.lyrics-line-active)"));
    assert!(css.contains("opacity: 0.65;"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn lyr_3_disabled_state_offers_activation() {
    gtk4::init().unwrap();
    let view = LyricsView::new();
    let activated = Rc::new(Cell::new(false));
    let activated_for_callback = activated.clone();
    view.set_on_settings(move || activated_for_callback.set(true));

    view.show_disabled();

    assert_eq!(view.visible_state_name().as_deref(), Some("disabled"));
    assert_eq!(
        view.disabled_snapshot(),
        (
            Some("network-offline-symbolic".into()),
            "Online lyrics are disabled".into(),
            "Enable them to load missing lyrics automatically".into(),
            "Enable in Settings".into(),
        )
    );
    view.activate_settings_for_test();
    assert!(activated.get());
}

#[test]
fn npp_9_fallbacks_keep_source_and_instrumental_gap_semantics() {
    let synced = LyricsBody::Synced(vec![TimedLine::new(1_000, "synthetic line")]);
    let plain = LyricsBody::Plain("synthetic plain text".into());

    assert_eq!(lyrics_footer(&synced), "synced · LRCLIB");
    assert_eq!(lyrics_footer(&plain), "lyrics · tags");
    assert_eq!(lyrics_footer(&LyricsBody::Instrumental), "");
    assert_eq!(active_line_alpha(1_000, 11_000), 100);
    assert_eq!(active_line_alpha(1_000, 11_001), 60);
    assert!(css().contains("color: alpha(#ffffff, 0.65)"));
}

#[test]
fn centered_scroll_clamps_at_start_middle_and_end() {
    assert_eq!(centered_scroll_value(10.0, 40.0, 200.0, 1_000.0), 0.0);
    assert_eq!(centered_scroll_value(450.0, 50.0, 200.0, 1_000.0), 375.0);
    assert_eq!(centered_scroll_value(950.0, 50.0, 200.0, 1_000.0), 800.0);
    assert_eq!(centered_scroll_value(50.0, 20.0, 400.0, 200.0), 0.0);
}

#[test]
fn lyrics_padding_only_synthesizes_trailing_context() {
    assert_eq!(content_margins(240, 40), (18, 100));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn lyrics_bodies_are_not_selectable_and_only_one_timed_line_is_active() {
    gtk4::init().unwrap();
    let view = LyricsView::new();
    view.show_result(&LyricsBody::Synced(vec![
        TimedLine::new(1_000, "first synthetic line"),
        TimedLine::new(2_000, "second synthetic line"),
        TimedLine::new(3_000, "third synthetic line"),
    ]));
    let labels = view.line_labels();
    assert_eq!(labels.len(), 3);
    assert!(labels.iter().all(|label| !label.is_selectable()));
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
    assert!(!plain[0].is_selectable());
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
fn npp_9_errors_offer_only_inline_retry() {
    gtk4::init().unwrap();
    let view = LyricsView::new();

    view.show_error(&LyricsError::Temporary);
    assert!(view.retry_is_visible());
    assert!(view.retry_has_css_class(INLINE_RETRY_CLASS));
    view.show_error(&LyricsError::NotFound);
    assert!(!view.retry_is_visible());
    assert_eq!(view.status_text(), "No lyrics found");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_7_user_scroll_pauses_autoscroll() {
    gtk4::init().unwrap();
    let timer = ManualScrollTimer::new();
    let view = LyricsView::new_with_timer(timer.clone());
    view.show_result(&LyricsBody::Synced(vec![TimedLine::new(
        1_000,
        "synthetic line",
    )]));
    view.set_active_line(Some(0));

    view.simulate_user_scroll();
    assert_eq!(view.scroll_mode(), ScrollMode::UserPause);
    assert_eq!(timer.active_timer_count(), 1);
    timer.advance(2_000);
    view.simulate_user_scroll();
    assert_eq!(
        timer.active_timer_count(),
        1,
        "a new event resets the timer"
    );
    view.simulate_programmatic_scroll();
    assert_eq!(timer.active_timer_count(), 1);
    timer.advance(3_999);
    assert_eq!(view.scroll_mode(), ScrollMode::UserPause);
    timer.advance(1);
    assert_eq!(view.scroll_mode(), ScrollMode::Returning);

    view.simulate_user_scroll();
    assert_eq!(view.scroll_mode(), ScrollMode::UserPause);
    assert!(!view.has_scroll_animation());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_8_line_click_seeks() {
    gtk4::init().unwrap();
    let timer = ManualScrollTimer::new();
    let view = LyricsView::new_with_timer(timer.clone());
    view.show_result(&LyricsBody::Synced(vec![
        TimedLine::new(1_000, "first synthetic line"),
        TimedLine::new(2_500, "second synthetic line"),
    ]));
    let sought = Rc::new(Cell::new(None));
    let sought_for_callback = sought.clone();
    view.set_on_seek(move |position_ms| sought_for_callback.set(Some(position_ms)));

    view.simulate_user_scroll();
    view.simulate_line_click(1);
    assert_eq!(sought.get(), Some(2_500));
    assert_eq!(view.scroll_mode(), ScrollMode::Auto);
    assert_eq!(timer.active_timer_count(), 0);

    view.simulate_user_scroll();
    view.simulate_external_seek();
    assert_eq!(view.scroll_mode(), ScrollMode::Auto);
    assert_eq!(timer.active_timer_count(), 0);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn synced_lyrics_are_roving_rows_and_plain_lyrics_are_not_actions() {
    gtk4::init().unwrap();
    let view = LyricsView::new();
    view.show_result(&LyricsBody::Synced(vec![TimedLine::new(1_000, "synced")]));
    let synced = view.line_rows_for_test();
    assert_eq!(synced.len(), 1);
    assert!(synced[0].is_activatable());

    view.show_result(&LyricsBody::Plain("plain".into()));
    let plain = view.line_rows_for_test();
    assert_eq!(plain.len(), 1);
    assert!(!plain[0].is_activatable());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn active_lines_center_and_clamp_in_a_mapped_panel() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
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
    wait_for_scroll_value(&view, |value, maximum| value > 0.0 && value < maximum);
    let (middle, maximum) = view.scroll_values();
    assert!(middle > 0.0 && middle < maximum);

    view.set_active_line(Some(0));
    wait_for_scroll_value(&view, |value, _| value == 0.0);
    assert_eq!(view.scroll_values().0, 0.0);

    view.set_active_line(Some(39));
    wait_for_scroll_value(&view, |value, maximum| (value - maximum).abs() < 1.0);
    let (end, maximum) = view.scroll_values();
    assert!((end - maximum).abs() < 1.0);
    window.close();
}

fn wait_for_scroll_value(view: &LyricsView, predicate: impl Fn(f64, f64) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let (value, maximum) = view.scroll_values();
        if predicate(value, maximum) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "lyrics scroll did not settle: value={value}, maximum={maximum}"
        );
        gtk4::glib::MainContext::default().iteration(true);
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn lyr_4_start_of_song_is_not_centered() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let view = LyricsView::new();
    let lines = (0..20)
        .map(|index| TimedLine::new(i64::from(index) * 1_000, format!("line {index}")))
        .collect();
    view.show_result(&LyricsBody::Synced(lines));
    let window = gtk4::Window::builder()
        .default_width(300)
        .default_height(240)
        .child(view.widget())
        .build();
    window.present();
    settle_until(1000, || view.widget().height() > 0);

    assert!(
        (view.line_viewport_top_offset(0) - 18.0).abs() < 2.0,
        "top offset was {} (expected ~18), center offset {}, allocated height {}",
        view.line_viewport_top_offset(0),
        view.line_center_offset(0),
        view.widget().height()
    );
    assert_eq!(view.content_margin_top(), 18);
    assert_eq!(view.scroll_values().0, 0.0);
    assert!(view.line_center_offset(0) < -20.0);
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_10_new_lyrics_begin_at_line_zero() {
    gtk4::init().unwrap();
    let view = LyricsView::new();
    let lines = (0..20)
        .map(|index| TimedLine::new(i64::from(index) * 1_000, format!("line {index}")))
        .collect();
    view.show_result(&LyricsBody::Synced(lines));
    let window = gtk4::Window::builder()
        .default_width(300)
        .default_height(240)
        .child(view.widget())
        .build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert_eq!(view.scroll_values().0, 0.0);
    window.close();
}
