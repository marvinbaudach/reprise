//! Display tests for the My Stats composer.

use super::*;
use reprise_core::library::stats_snapshot::{ComparisonDirection, ComparisonFactor};
use std::sync::{Arc, Mutex};

fn view_and_conn() -> (StatsView, Rc<RefCell<Connection>>) {
    let conn = Rc::new(RefCell::new(reprise_core::db::open(None).unwrap()));
    reprise_core::db::migrate(&conn.borrow()).unwrap();
    let loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    (StatsView::new(loader), conn)
}

/// Presses the real CheckButtons of the Customize menu — a test that calls
/// the handler behind them proves nothing about the menu (STYLE-1) — and
/// reads the section order off the live widget tree.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_7_customize_toggles_sections() {
    gtk4::init().unwrap();
    let (view, conn) = view_and_conn();
    view.wire_year_selector(&conn);
    let [clock_check, genres_check, highlights_check] = view.render.customize.checks();
    assert_eq!(view.render.customize.check_count(), 3);
    assert!(clock_check.is_active());
    assert!(view.render.clock_section.is_visible());
    assert!(view.render.genres_section.is_visible());
    assert!(view.render.highlights_section.is_visible());
    assert_eq!(view.section_order(), SECTION_ORDER);

    clock_check.activate();

    assert!(!clock_check.is_active());
    assert!(!view.render.clock_section.is_visible());
    assert!(view.render.genres_section.is_visible());
    assert!(view.render.highlights_section.is_visible());
    // Hiding a section never reorders the page.
    assert_eq!(view.section_order(), SECTION_ORDER);
    assert_eq!(
        settings::get_stats_layout(&conn.borrow()),
        StatsLayout {
            clock: false,
            genres: true,
            highlights: true,
        }
    );

    genres_check.activate();
    highlights_check.activate();

    assert!(!view.render.genres_section.is_visible());
    assert!(!view.render.highlights_section.is_visible());
    assert_eq!(view.section_order(), SECTION_ORDER);
    assert_eq!(
        settings::get_stats_layout(&conn.borrow()),
        StatsLayout {
            clock: false,
            genres: false,
            highlights: false,
        }
    );

    clock_check.activate();

    assert!(view.render.clock_section.is_visible());
    assert_eq!(view.section_order(), SECTION_ORDER);
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
        .hero_subline
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
}

/// Responsive wrapping only settles after a real allocation, so the test has
/// to let a layout cycle run instead of merely draining pending sources.
fn wait_for_layout() {
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
        quit.quit();
    });
    main_loop.run();
}

/// One track with one play in the current period, so the page stack shows
/// the sections instead of the empty state — a hidden stack page is never
/// allocated, and an unallocated row cannot prove responsive wrapping.
fn seed_one_play(conn: &Connection) {
    seed_play_at(conn, now_unix());
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

/// STATS-1: the pill names the compared span, and that span is seasonally
/// congruent. "2026 so far" is measured against Jan–Jul 2025, so the pill has
/// to say so instead of naming an equally long stretch ("previous 200 days")
/// that reaches back into the previous winter.
#[test]
fn stats_1_pill_names_the_seasonally_congruent_compared_span() {
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

/// Between the two numbers lies a band of window widths where the row is
/// still side by side but already narrower than the two sections need —
/// GTK then under-allocates them. The band has to stay empty.
#[test]
fn asymmetric_row_minimums_fit_the_natural_line_length() {
    let minimum = CLOCK_MIN_WIDTH + HIGHLIGHTS_MIN_WIDTH + ASYMMETRIC_SPACING;
    assert!(
        minimum <= ASYMMETRIC_NATURAL_LINE_LENGTH,
        "side-by-side minimum {minimum} exceeds the natural line length \
         {ASYMMETRIC_NATURAL_LINE_LENGTH}"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_view_narrow_width_stacks_the_asymmetric_row() {
    gtk4::init().unwrap();
    let (view, window) = presented(600);
    let asymmetric_row = view.asymmetric_row.upgrade().unwrap();

    assert!(asymmetric_row.width() > 0);
    assert_ne!(
        view.render
            .clock_section
            .compute_bounds(&asymmetric_row)
            .unwrap()
            .y(),
        view.render
            .highlights_section
            .compute_bounds(&asymmetric_row)
            .unwrap()
            .y(),
        "narrow sections must wrap onto separate lines"
    );
    window.close();
}

/// STATS-1: a realistic Stats-pane allocation (the center column of a
/// 1200-pixel app window with both side columns present) must reflow controls
/// before either the hours anchor or the named comparison period ellipsizes.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_1_realistic_width_keeps_the_hero_copy_unellipsized() {
    gtk4::init().unwrap();
    crate::ui::style::install();
    let (view, conn) = view_and_conn();
    seed_one_play(&conn.borrow());
    view.wire_year_selector(&conn);
    view.render.hero_time.set_label("34 hours");
    view.render
        .comparison_pill
        .set_label("\u{25b2} 955% vs same period 2025");
    view.render.comparison_pill.set_visible(true);

    let window = adw::Window::builder()
        .default_width(600)
        .default_height(700)
        .content(view.widget())
        .build();
    window.set_size_request(600, -1);
    window.present();
    wait_for_layout();

    let hero_row = view.hero_row.upgrade().unwrap();
    let hero_time_row = view.hero_time_row.upgrade().unwrap();
    let first = hero_row.first_child().unwrap();
    let last = hero_row.last_child().unwrap();
    assert_ne!(
        first.compute_bounds(&hero_row).unwrap().y(),
        last.compute_bounds(&hero_row).unwrap().y(),
        "hero controls must wrap below the copy at a realistic width"
    );
    assert_eq!(
        view.render
            .hero_time
            .compute_bounds(&hero_time_row)
            .unwrap()
            .y(),
        view.render
            .comparison_pill
            .compute_bounds(&hero_time_row)
            .unwrap()
            .y(),
        "the hours and compact pill still fit on one line"
    );
    assert!(view.render.hero_time.is_mapped());
    assert!(view.render.comparison_pill.is_mapped());
    assert!(
        !view.render.hero_time.layout().is_ellipsized(),
        "hours anchor was ellipsized at {} px",
        view.render.hero_time.width()
    );
    assert!(
        !view.render.comparison_pill.layout().is_ellipsized(),
        "comparison reference was ellipsized at {} px",
        view.render.comparison_pill.width()
    );

    window.close();
}

/// STATS-1a: the pill owns compact copy and the tooltip owns the fully named
/// seasonal reference. At a realistic center-pane width the compact label is
/// allocated in full; ellipsization is never an accepted fallback.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_1a_comparison_pill_is_not_ellipsized_at_a_realistic_width() {
    gtk4::init().unwrap();
    crate::ui::style::install();
    let (view, conn) = view_and_conn();
    seed_one_play(&conn.borrow());
    view.wire_year_selector(&conn);
    view.render.hero_time.set_label("34 hours");
    render_comparison(
        &view.render,
        Some(ComparisonPresentation::Factor {
            direction: ComparisonDirection::Up,
            value: ComparisonFactor::Whole(11),
        }),
        StatsPeriod::YearToDate(2026),
    );

    let window = adw::Window::builder()
        .default_width(600)
        .default_height(700)
        .content(view.widget())
        .build();
    window.set_size_request(600, -1);
    window.present();
    wait_for_layout();

    assert_eq!(
        view.render.comparison_pill.label(),
        "\u{25b2} \u{00d7}11 vs 2025"
    );
    assert_eq!(
        view.render.comparison_pill.tooltip_text().as_deref(),
        Some("\u{25b2} \u{00d7}11 vs same period 2025")
    );
    assert_eq!(
        view.render.comparison_pill.ellipsize(),
        gtk4::pango::EllipsizeMode::None
    );
    assert!(
        !view.render.comparison_pill.layout().is_ellipsized(),
        "comparison pill was ellipsized at {} px",
        view.render.comparison_pill.width()
    );

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_view_wide_width_keeps_the_asymmetric_row_side_by_side() {
    gtk4::init().unwrap();
    let (view, window) = presented(1_000);
    let asymmetric_row = view.asymmetric_row.upgrade().unwrap();

    assert_eq!(
        view.render
            .clock_section
            .compute_bounds(&asymmetric_row)
            .unwrap()
            .y(),
        view.render
            .highlights_section
            .compute_bounds(&asymmetric_row)
            .unwrap()
            .y(),
        "wide sections must remain side by side"
    );
    window.close();
}
