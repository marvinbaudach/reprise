//! Display tests for the My Stats composer.

use super::*;

fn view_and_conn() -> (StatsView, Rc<RefCell<Connection>>) {
    let conn = Rc::new(RefCell::new(reprise_core::db::open(None).unwrap()));
    reprise_core::db::migrate(&conn.borrow()).unwrap();
    let loader = CoverLoader::new(crate::ui::cover_download_worker::setup());
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
fn stats_view_empty_history_shows_the_status_page_instead_of_the_sections() {
    gtk4::init().unwrap();
    let (view, conn) = view_and_conn();
    view.wire_year_selector(&conn);

    assert_eq!(
        view.page_stack.visible_child_name().as_deref(),
        Some("empty")
    );
    // The ribbon is not hidden, it is simply on the page the stack is not
    // showing — which is what actually keeps it off screen.
    let sections = view.page_stack.child_by_name("sections").unwrap();
    assert!(view.render.ribbon.widget().is_ancestor(&sections));
    assert_ne!(view.page_stack.visible_child(), Some(sections));
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

/// The breakpoint only applies once the bin has a real allocation, so the
/// test has to let a layout cycle run instead of draining pending sources.
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
/// allocated, and an unallocated bin can never hit a breakpoint.
fn seed_one_play(conn: &Connection) {
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
        rusqlite::params![now_unix()],
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

/// STATS-1: the pill names the compared span, and that span is seasonally
/// congruent. "2026 so far" is measured against Jan–Jul 2025, so the pill has
/// to say so instead of naming an equally long stretch ("previous 200 days")
/// that reaches back into the previous winter.
#[test]
fn stats_1_pill_names_the_seasonally_congruent_compared_span() {
    let name = |period| compared_period_name(period).unwrap();

    assert_eq!(name(StatsPeriod::YearToDate(2026)), "same period 2025");
    assert_eq!(name(StatsPeriod::Year(2025)), "2024");
    assert_eq!(name(StatsPeriod::Last30Days), "previous 30 days");
    // All time is compared against nothing, so it names nothing.
    assert_eq!(compared_period_name(StatsPeriod::AllTime), None);
}

/// Between the two numbers lies a band of window widths where the row is
/// still side by side but already narrower than the two sections need —
/// GTK then under-allocates them. The band has to stay empty.
#[test]
fn asymmetric_row_minimums_fit_under_the_breakpoint() {
    let minimum = CLOCK_MIN_WIDTH + HIGHLIGHTS_MIN_WIDTH + ASYMMETRIC_SPACING;
    assert!(
        f64::from(minimum) <= ASYMMETRIC_BREAKPOINT,
        "side-by-side minimum {minimum} exceeds the breakpoint {ASYMMETRIC_BREAKPOINT}"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_view_narrow_width_stacks_the_asymmetric_row() {
    gtk4::init().unwrap();
    let (view, window) = presented(600);

    let width = view.asymmetric_bin.width();
    assert!(width > 0, "the bin must be allocated, got {width}");
    assert!(
        width <= ASYMMETRIC_BREAKPOINT as i32,
        "the row's minimum width must fit under the breakpoint, got {width}"
    );
    assert_eq!(
        view.asymmetric_row.orientation(),
        gtk4::Orientation::Vertical
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_view_wide_width_keeps_the_asymmetric_row_side_by_side() {
    gtk4::init().unwrap();
    let (view, window) = presented(1_000);

    let width = view.asymmetric_bin.width();
    assert!(
        width > ASYMMETRIC_BREAKPOINT as i32,
        "a wide window must allocate the bin above the breakpoint, got {width}"
    );
    assert_eq!(
        view.asymmetric_row.orientation(),
        gtk4::Orientation::Horizontal
    );
    window.close();
}
