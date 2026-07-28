//! Display tests for the My Stats composer.

use super::*;
use reprise_core::library::stats_snapshot::ComparisonPresentation;
use std::sync::{Arc, Mutex};

fn view_and_conn() -> (StatsView, Rc<RefCell<Connection>>) {
    let conn = Rc::new(RefCell::new(reprise_core::db::open(None).unwrap()));
    reprise_core::db::migrate(&conn.borrow()).unwrap();
    let loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    (StatsView::new(loader), conn)
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_10_page_orders_header_hero_chart_row_genres() {
    gtk4::init().unwrap();
    let (view, _) = view_and_conn();

    assert_eq!(view.section_order(), SECTION_ORDER);
    assert!(!view.page_stack.is_vhomogeneous());
    assert!(!view.render.trend_stack.is_vhomogeneous());
}

#[test]
fn section_spacing_stays_in_the_compact_design_range() {
    assert!((16..=24).contains(&SECTION_SPACING));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_10_no_clock_highlights_or_customize_widgets() {
    gtk4::init().unwrap();
    let (view, _) = view_and_conn();
    let page = view.page.upgrade().unwrap();
    let copy = descendant_copy(page.upcast_ref());

    assert!(!copy.iter().any(|text| text.contains("LISTENING CLOCK")));
    assert!(!copy.iter().any(|text| text.contains("HIGHLIGHTS")));
    assert!(!copy.iter().any(|text| text.contains("Customize")));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_6c_fresh_library_without_counters_keeps_plain_empty_state() {
    gtk4::init().unwrap();
    let (view, conn) = view_and_conn();
    view.wire_year_selector(&conn);

    assert_eq!(
        view.page_stack.visible_child_name().as_deref(),
        Some("empty")
    );
    let empty = view
        .page_stack
        .child_by_name("empty")
        .unwrap()
        .downcast::<adw::StatusPage>()
        .unwrap();
    assert_eq!(empty.title(), "Start listening to see your stats");
    // The ribbon is not hidden, it is simply on the page the stack is not
    // showing — which is what actually keeps it off screen.
    let sections = view.page_stack.child_by_name("sections").unwrap();
    assert!(view.render.ribbon.widget().is_ancestor(&sections));
    assert_ne!(view.page_stack.visible_child(), Some(sections));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_6c_imported_counters_do_not_change_the_empty_period_state() {
    gtk4::init().unwrap();
    let (view, conn) = view_and_conn();
    seed_imported_plays(&conn.borrow(), 194);

    view.wire_year_selector(&conn);

    assert_eq!(
        view.page_stack.visible_child_name().as_deref(),
        Some("empty")
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_6c_empty_period_keeps_the_period_selector_available() {
    gtk4::init().unwrap();
    let (view, conn) = view_and_conn();
    seed_play_at(&conn.borrow(), now_unix() - 60 * 24 * 60 * 60);
    view.wire_year_selector(&conn);

    let last_30_days = view
        .periods
        .borrow()
        .iter()
        .position(|period| *period == StatsPeriod::Last30Days)
        .unwrap() as u32;
    let window = adw::Window::builder()
        .default_width(1_000)
        .default_height(700)
        .content(view.widget())
        .build();
    window.present();
    wait_for_layout();

    view.period_dropdown.set_selected(last_30_days);
    while glib::MainContext::default().iteration(false) {}

    assert_eq!(
        view.page_stack.visible_child_name().as_deref(),
        Some("empty")
    );
    assert!(
        view.period_dropdown.is_mapped(),
        "the selector must remain operable while the selected period is empty"
    );

    view.period_dropdown.set_selected(0);
    while glib::MainContext::default().iteration(false) {}
    assert_eq!(
        view.page_stack.visible_child_name().as_deref(),
        Some("sections")
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_6c_empty_state_disappears_when_real_events_exist() {
    gtk4::init().unwrap();
    let (view, conn) = view_and_conn();
    seed_imported_plays(&conn.borrow(), 194);
    view.wire_year_selector(&conn);
    assert_eq!(
        view.page_stack.visible_child_name().as_deref(),
        Some("empty")
    );

    for seconds_ago in 0..5 {
        conn.borrow()
            .execute(
                "INSERT INTO listen_events (track_id, played_at, ms_played) \
                 VALUES (1, ?1, 60000)",
                rusqlite::params![now_unix() - seconds_ago],
            )
            .unwrap();
    }
    view.refresh(&conn);

    assert_eq!(
        view.page_stack.visible_child_name().as_deref(),
        Some("sections")
    );
    assert!(view
        .render
        .hero
        .subline
        .label()
        .starts_with("5 plays \u{00b7}"));
    let snapshot = view.current_snapshot.borrow();
    let snapshot = snapshot.as_ref().unwrap();
    assert_eq!(snapshot.hero.plays, 5);
    assert_eq!(snapshot.top_tracks[0].play_count, 5);
    assert_eq!(
        conn.borrow()
            .query_row("SELECT play_count FROM tracks WHERE id = 1", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        194
    );
}

/// A broken query is not an empty history: the user must not be told to
/// start listening when the numbers exist but could not be read.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_6a_unreadable_history_shows_the_failure_page() {
    gtk4::init().unwrap();
    let (view, conn) = view_and_conn();
    seed_one_play(&conn.borrow());
    view.wire_year_selector(&conn);
    assert_eq!(
        view.page_stack.visible_child_name().as_deref(),
        Some("sections")
    );

    conn.borrow()
        .execute("DROP TABLE listen_events", [])
        .unwrap();
    view.refresh(&conn);

    assert_eq!(
        view.page_stack.visible_child_name().as_deref(),
        Some("failed")
    );
    assert!(!view.render.hero.root.is_visible());
    assert!(!view.render.header.new_badge.is_visible());
    assert!(view.render.hero.time.label().is_empty());
    assert!(view.render.hero.subline.label().is_empty());
}

/// Responsive wrapping only settles after a real allocation, so the test has
/// to let a layout cycle run instead of merely draining pending sources.
fn wait_for_layout() {
    wait_for(150);
}

fn wait_for(milliseconds: u64) {
    crate::ui::test_settle::settle_for(std::time::Duration::from_millis(milliseconds));
}

/// One track with one play in the current period, so the page stack shows
/// the sections instead of the empty state — a hidden stack page is never
/// allocated, and an unallocated row cannot prove responsive wrapping.
fn seed_one_play(conn: &Connection) {
    seed_play_at(conn, now_unix());
}

fn seed_plays(conn: &Connection, count: i64) {
    seed_play_at(conn, now_unix());
    for offset in 1..count {
        conn.execute(
            "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (1, ?1, 200000)",
            rusqlite::params![now_unix() - offset],
        )
        .unwrap();
    }
}

fn seed_previous_year_play(conn: &Connection) {
    conn.execute(
        "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (1, ?1, 100000)",
        rusqlite::params![now_unix() - 365 * 24 * 60 * 60],
    )
    .unwrap();
}

fn seed_play_at(conn: &Connection, played_at: i64) {
    conn.execute(
        "INSERT INTO tracks \
         (id, path, title, artist, album, album_artist, genre, duration_ms, \
          play_count, added_at) \
         VALUES (1, '/music/1.flac', 'Track', 'Artist', 'Album', '', 'Rock', 300000, 1, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (1, ?1, 200000)",
        rusqlite::params![played_at],
    )
    .unwrap();
}

fn seed_imported_plays(conn: &Connection, play_count: i64) {
    conn.execute(
        "INSERT INTO tracks \
         (id, path, title, artist, album, album_artist, genre, duration_ms, \
          play_count, added_at) \
         VALUES (1, '/music/imported.flac', 'Imported', 'Artist', 'Album', '', \
                 'Rock', 300000, ?1, 0)",
        rusqlite::params![play_count],
    )
    .unwrap();
}

fn descendant_copy(root: &gtk4::Widget) -> Vec<String> {
    let mut copy = Vec::new();
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
            copy.push(label.label().to_string());
        }
        if let Some(button) = widget.downcast_ref::<gtk4::Button>() {
            if let Some(label) = button.label() {
                copy.push(label.to_string());
            }
        }
        copy.extend(descendant_copy(&widget));
        child = widget.next_sibling();
    }
    copy
}

fn presented(width: i32) -> (StatsView, adw::Window) {
    let (view, conn) = view_and_conn();
    seed_one_play(&conn.borrow());
    view.wire_year_selector(&conn);
    assert_eq!(
        view.page_stack.visible_child_name().as_deref(),
        Some("sections")
    );
    let window = adw::Window::builder()
        .default_width(width)
        .default_height(700)
        .content(view.widget())
        .build();
    // A bare Xvfb screen can be smaller than the requested default size,
    // which would silently turn the wide case into a narrow one.
    window.set_size_request(width, -1);
    window.present();
    wait_for_layout();
    (view, window)
}

/// Opening My Stats must expose the headline at the start of the viewport.
/// The adjustment assertion distinguishes an accidental scroll restore from
/// a responsive container that collapsed while the viewport stayed at zero.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_view_opens_at_top_with_the_hero_allocated() {
    gtk4::init().unwrap();
    let (view, window) = presented(1_000);
    let adjustment = view.root.vadjustment();
    let hero_row = view.hero_row.upgrade().unwrap();
    let hero_time_row = view.hero_time_row.upgrade().unwrap();
    let hero_owner = hero_row.parent().unwrap();

    assert_eq!(adjustment.value(), adjustment.lower());
    assert!(hero_time_row.is_mapped());
    assert!(
        hero_row.height() > 1,
        "hero responsive row collapsed to {} px",
        hero_row.height()
    );
    assert!(
        hero_time_row.height() <= hero_owner.height(),
        "hero child is {} px high but its responsive owner is only {} px high",
        hero_time_row.height(),
        hero_owner.height()
    );

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_view_present_refresh_and_teardown_emit_no_criticals() {
    gtk4::init().unwrap();
    let criticals = Arc::new(Mutex::new(Vec::new()));
    let handlers = ["Gtk", "GLib-GObject"].map(|domain| {
        let criticals = criticals.clone();
        glib::log_set_handler(
            Some(domain),
            glib::LogLevels::LEVEL_CRITICAL,
            false,
            false,
            move |domain, _, message| {
                criticals
                    .lock()
                    .unwrap()
                    .push(format!("{}: {message}", domain.unwrap_or("unknown")));
            },
        )
    });

    {
        let (view, conn) = view_and_conn();
        seed_one_play(&conn.borrow());
        view.wire_year_selector(&conn);
        let callback_copy = view.clone();
        let window = adw::Window::builder()
            .default_width(1_000)
            .default_height(700)
            .content(view.widget())
            .build();
        window.present();
        wait_for_layout();
        callback_copy.refresh(&conn);
        while glib::MainContext::default().iteration(false) {}
        window.close();
        drop(window);
        drop(callback_copy);
        drop(view);
        while glib::MainContext::default().iteration(false) {}
    }

    for (domain, handler) in ["Gtk", "GLib-GObject"].into_iter().zip(handlers) {
        glib::log_remove_handler(Some(domain), handler);
    }
    let criticals = criticals.lock().unwrap();
    assert!(criticals.is_empty(), "GTK criticals: {criticals:?}");
}

/// STATS-11: the trend tooltip names the seasonally congruent compared span.
#[test]
fn stats_11_trend_tooltip_names_the_seasonally_congruent_compared_span() {
    let tooltip = |period| {
        strings::comparison_copy(ComparisonPresentation::Percentage(12), period)
            .unwrap()
            .tooltip
    };

    assert_eq!(
        tooltip(StatsPeriod::YearToDate(2026)),
        "\u{25b2} 12% vs same period 2025"
    );
    assert_eq!(tooltip(StatsPeriod::Year(2025)), "\u{25b2} 12% vs 2024");
    assert_eq!(
        tooltip(StatsPeriod::Last30Days),
        "\u{25b2} 12% vs previous 30 days"
    );
    // All time is compared against nothing, so it names nothing.
    assert_eq!(
        strings::comparison_copy(ComparisonPresentation::Percentage(12), StatsPeriod::AllTime),
        None
    );
}

/// The natural wrap point must never under-allocate either story card.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn story_row_minimums_fit_the_natural_line_length() {
    gtk4::init().unwrap();
    crate::ui::style::install_css_string_for_test(&super::super::stats_css::css());
    let (view, _) = view_and_conn();
    let (band_minimum, _, _, _) = view
        .render
        .band_section
        .measure(gtk4::Orientation::Horizontal, -1);
    let (songs_minimum, _, _, _) = view
        .render
        .songs_section
        .measure(gtk4::Orientation::Horizontal, -1);
    let minimum = band_minimum + songs_minimum + STORY_SPACING;

    assert!(
        minimum <= STORY_NATURAL_LINE_LENGTH,
        "measured side-by-side minimum {minimum} exceeds the natural line length \
         {STORY_NATURAL_LINE_LENGTH} (band {band_minimum}, songs {songs_minimum})"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_10_narrow_window_stacks_band_before_songs() {
    gtk4::init().unwrap();
    let (view, window) = presented(600);
    let story_row = view.story_row.upgrade().unwrap();

    assert!(story_row.width() > 0);
    assert_ne!(
        view.render
            .band_section
            .compute_bounds(&story_row)
            .unwrap()
            .y(),
        view.render
            .songs_section
            .compute_bounds(&story_row)
            .unwrap()
            .y(),
        "narrow story cards must wrap onto separate lines"
    );
    assert!(
        view.render
            .band_section
            .compute_bounds(&story_row)
            .unwrap()
            .y()
            < view
                .render
                .songs_section
                .compute_bounds(&story_row)
                .unwrap()
                .y(),
        "the band card must remain before the songs card"
    );
    window.close();
}

/// STATS-11: a realistic Stats-pane allocation must reflow the KPI row before
/// the listening-time anchor or trend reference ellipsizes.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_11_realistic_width_keeps_the_hero_copy_unellipsized() {
    gtk4::init().unwrap();
    crate::ui::style::install();
    let (view, conn) = view_and_conn();
    seed_one_play(&conn.borrow());
    seed_previous_year_play(&conn.borrow());
    view.wire_year_selector(&conn);

    let window = adw::Window::builder()
        .default_width(600)
        .default_height(700)
        .content(view.widget())
        .build();
    window.set_size_request(600, -1);
    window.present();
    wait_for_layout();

    let hero_row = view.hero_row.upgrade().unwrap();
    let first = hero_row.first_child().unwrap();
    let last = hero_row.last_child().unwrap();
    assert_ne!(
        first.compute_bounds(&hero_row).unwrap().y(),
        last.compute_bounds(&hero_row).unwrap().y(),
        "hero controls must wrap below the copy at a realistic width"
    );
    assert!(view.render.hero.time.is_mapped());
    assert!(view.render.hero.kpis.trend.root.is_mapped());
    assert!(
        !view.render.hero.time.layout().is_ellipsized(),
        "hours anchor was ellipsized at {} px",
        view.render.hero.time.width()
    );
    assert!(
        !view.render.hero.kpis.trend.value.layout().is_ellipsized(),
        "comparison reference was ellipsized at {} px",
        view.render.hero.kpis.trend.value.width()
    );

    window.close();
}

/// STATS-11a: a zero baseline moves the new-history message into the header
/// badge and never leaves a meaningless delta in the KPI row.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_11a_new_badge_is_not_ellipsized_at_a_realistic_width() {
    gtk4::init().unwrap();
    crate::ui::style::install();
    let (view, conn) = view_and_conn();
    seed_one_play(&conn.borrow());
    view.wire_year_selector(&conn);

    let window = adw::Window::builder()
        .default_width(600)
        .default_height(700)
        .content(view.widget())
        .build();
    window.set_size_request(600, -1);
    window.present();
    wait_for_layout();

    assert_eq!(view.render.header.new_badge.label(), "New this year");
    assert_eq!(
        view.render.header.new_badge.ellipsize(),
        gtk4::pango::EllipsizeMode::None
    );
    assert!(
        !view.render.header.new_badge.layout().is_ellipsized(),
        "new badge was ellipsized at {} px",
        view.render.header.new_badge.width()
    );
    assert!(!view.render.hero.kpis.trend.root.is_visible());

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_11_hero_renders_kpi_pairs_without_placeholders() {
    gtk4::init().unwrap();
    let (view, conn) = view_and_conn();
    seed_one_play(&conn.borrow());
    seed_previous_year_play(&conn.borrow());
    view.wire_year_selector(&conn);

    assert!(view.render.hero.kpis.per_day.root.is_visible());
    assert!(view.render.hero.kpis.trend.root.is_visible());
    assert!(view.render.hero.kpis.pace.root.is_visible());
    assert!(view.render.hero.kpis.best_week.root.is_visible());
    assert!(view.render.hero.subline.label().contains("plays"));
    assert!(view.render.hero.subline.label().contains("artists"));

    let all_time = view
        .periods
        .borrow()
        .iter()
        .position(|period| *period == StatsPeriod::AllTime)
        .unwrap() as u32;
    view.period_dropdown.set_selected(all_time);
    while glib::MainContext::default().iteration(false) {}

    assert!(!view.render.hero.kpis.trend.root.is_visible());
    assert!(!view.render.hero.kpis.pace.root.is_visible());
    assert!(view.render.hero.kpis.per_day.root.is_visible());
    assert!(view.render.hero.kpis.best_week.root.is_visible());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_11a_zero_baseline_shows_new_badge_not_a_delta() {
    gtk4::init().unwrap();
    let (view, conn) = view_and_conn();
    seed_one_play(&conn.borrow());
    view.wire_year_selector(&conn);

    assert!(view.render.header.new_badge.is_visible());
    assert_eq!(view.render.header.new_badge.label(), "New this year");
    assert!(!view.render.hero.kpis.trend.root.is_visible());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_16_thin_history_swaps_chart_for_hint() {
    gtk4::init().unwrap();
    let (view, conn) = view_and_conn();
    seed_plays(&conn.borrow(), 9);
    view.wire_year_selector(&conn);

    assert_eq!(
        view.render.trend_stack.visible_child_name().as_deref(),
        Some("hint")
    );
    assert!(view.render.hero.subline.label().starts_with("9 plays"));

    conn.borrow()
        .execute(
            "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (1, ?1, 200000)",
            rusqlite::params![now_unix() - 10],
        )
        .unwrap();
    view.refresh(&conn);

    assert_eq!(
        view.render.trend_stack.visible_child_name().as_deref(),
        Some("chart")
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_10_wide_window_keeps_band_and_songs_side_by_side() {
    gtk4::init().unwrap();
    let (view, window) = presented(1_000);
    let story_row = view.story_row.upgrade().unwrap();

    assert_eq!(
        view.render
            .band_section
            .compute_bounds(&story_row)
            .unwrap()
            .y(),
        view.render
            .songs_section
            .compute_bounds(&story_row)
            .unwrap()
            .y(),
        "wide story cards must remain side by side"
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_10_section_gaps_stay_equal_when_top_tracks_expand_and_collapse() {
    gtk4::init().unwrap();
    crate::ui::style::install_css_string_for_test(&super::super::stats_css::css());
    let (view, window) = presented(1_000);
    let sections = view
        .page_stack
        .child_by_name("sections")
        .unwrap()
        .downcast::<gtk4::Box>()
        .unwrap();

    assert!(!view.render.top_tracks_section.is_visible());
    assert_eq!(occupied_section_gaps(&sections), vec![20, 20]);

    let reveal = descendant_button(
        view.render.songs_card.widget().upcast_ref(),
        "Show all top tracks",
    );
    reveal.emit_clicked();
    wait_for(400);

    assert!(view.render.top_tracks_section.is_visible());
    assert!(view.render.top_tracks_section.is_child_revealed());
    assert_eq!(occupied_section_gaps(&sections), vec![20, 20, 20]);

    reveal.emit_clicked();
    wait_for(400);

    assert!(!view.render.top_tracks_section.is_visible());
    assert_eq!(occupied_section_gaps(&sections), vec![20, 20]);
    window.close();
}

fn occupied_section_gaps(sections: &gtk4::Box) -> Vec<i32> {
    let mut bounds = Vec::new();
    let mut child = sections.first_child();
    while let Some(widget) = child {
        let widget_bounds = widget.compute_bounds(sections).unwrap();
        if widget.is_visible() && widget_bounds.height() > 0.0 {
            bounds.push(widget_bounds);
        }
        child = widget.next_sibling();
    }
    bounds
        .windows(2)
        .map(|pair| (pair[1].y() - pair[0].y() - pair[0].height()).round() as i32)
        .collect()
}

fn descendant_button(root: &gtk4::Widget, label: &str) -> gtk4::Button {
    descendant_button_or_none(root, label)
        .unwrap_or_else(|| panic!("missing button labeled {label}"))
}

fn descendant_button_or_none(root: &gtk4::Widget, label: &str) -> Option<gtk4::Button> {
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(button) = widget.downcast_ref::<gtk4::Button>() {
            if button.label().as_deref() == Some(label) {
                return Some(button.clone());
            }
        }
        if let Some(button) = descendant_button_or_none(&widget, label) {
            return Some(button);
        }
        child = widget.next_sibling();
    }
    None
}
