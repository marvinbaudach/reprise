//! Rule-named display tests for the My Stats entrance choreography.

use super::*;

fn view_and_conn() -> (StatsView, Rc<RefCell<Connection>>) {
    crate::ui::style::install_css_string_for_test(&crate::ui::stats::stats_css::css());
    let conn = Rc::new(RefCell::new(reprise_core::db::open(None).unwrap()));
    reprise_core::db::migrate(&conn.borrow()).unwrap();
    let loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    (StatsView::new(loader), conn)
}

fn presented(width: i32) -> (StatsView, adw::Window) {
    let (view, conn) = view_and_conn();
    seed_plays(&conn.borrow(), 10);
    view.wire_year_selector(&conn);
    let window = adw::Window::builder()
        .default_width(width)
        .default_height(700)
        .content(view.widget())
        .build();
    window.set_size_request(width, -1);
    window.present();
    wait_for(150);
    (view, window)
}

fn presented_entrance(width: i32) -> (StatsView, Rc<RefCell<Connection>>, adw::Window) {
    presented_entrance_at_size(width, 700)
}

fn presented_entrance_at_size(
    width: i32,
    height: i32,
) -> (StatsView, Rc<RefCell<Connection>>, adw::Window) {
    let (view, conn) = view_and_conn();
    seed_plays(&conn.borrow(), 10);
    view.wire_year_selector(&conn);
    view.prepare_entrance();
    view.refresh(&conn);

    let window = adw::Window::builder()
        .default_width(width)
        .default_height(height)
        .content(view.widget())
        .build();
    window.set_size_request(width, height);
    window.present();
    wait_for(80);
    (view, conn, window)
}

fn seed_plays(conn: &Connection, count: i64) {
    conn.execute(
        "INSERT INTO tracks \
         (id, path, title, artist, album, album_artist, genre, duration_ms, \
          play_count, added_at) \
         VALUES (1, '/music/1.flac', 'Track', 'Artist', 'Album', '', 'Rock', \
                 300000, 1, 0)",
        [],
    )
    .unwrap();
    for offset in 0..count {
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

fn wait_for(milliseconds: u64) {
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(milliseconds), move || {
        quit.quit();
    });
    main_loop.run();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_17_entrance_runs_once_per_open() {
    gtk4::init().unwrap();
    let (view, conn, window) = presented_entrance(1_000);
    let final_copy = strings::stats_duration(
        view.current_snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .hero
            .total_ms,
    );

    assert_eq!(view.render.entrance.entrance_runs(), 1);
    assert_ne!(
        view.render.hero.time.label(),
        final_copy,
        "the mapped hero must still be counting after its first frame"
    );

    wait_for(1_050);
    view.refresh(&conn);
    assert_eq!(view.render.entrance.entrance_runs(), 1);
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_17_entrance_is_a_sequence_not_a_burst() {
    gtk4::init().unwrap();
    let (view, _conn, window) = presented_entrance_at_size(1_000, 1_000);
    wait_for(320);

    assert_eq!(
        view.render.hero.time_slide.opacity(),
        1.0,
        "the hero must have completed before the later cards begin"
    );
    assert_eq!(
        view.render.genres_slide.opacity(),
        0.0,
        "the genre card must still be waiting while the hero is legible"
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_17_cards_below_the_fold_skip_the_entrance() {
    gtk4::init().unwrap();
    let (view, _conn, window) = presented_entrance_at_size(1_000, 360);
    let content = view.root.child().unwrap();
    let bounds = view
        .render
        .genres_slide
        .compute_bounds(&content)
        .expect("the genre card and clamp share one widget tree");
    let adjustment = view.root.vadjustment();

    assert!(
        f64::from(bounds.y()) >= adjustment.value() + adjustment.page_size(),
        "the fixture must keep the genre card below the initial viewport"
    );
    assert_eq!(view.render.genres_slide.opacity(), 1.0);
    assert_eq!(view.render.genres_slide.offset_y(), 0.0);
    assert!(view
        .render
        .genres_section_data
        .segment_slides()
        .iter()
        .all(|segment| segment.reveal_fraction() == 1.0));
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_17_period_switch_only_tweens_values() {
    gtk4::init().unwrap();
    let (view, conn, window) = presented_entrance(1_000);
    wait_for(1_050);
    seed_previous_year_play(&conn.borrow());
    view.refresh(&conn);
    let entrances = view.render.entrance.entrance_runs();
    let tweens = view.render.entrance.tween_runs();

    let all_time = view
        .periods
        .borrow()
        .iter()
        .position(|period| *period == StatsPeriod::AllTime)
        .unwrap() as u32;
    view.period_dropdown.set_selected(all_time);
    wait_for(40);
    let final_copy = strings::stats_duration(
        view.current_snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .hero
            .total_ms,
    );

    assert_eq!(view.render.entrance.entrance_runs(), entrances);
    assert_eq!(view.render.entrance.tween_runs(), tweens + 1);
    assert_ne!(
        view.render.hero.time.label(),
        final_copy,
        "a mapped period switch must be observable mid-tween"
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_17_reduced_motion_lands_in_end_state() {
    gtk4::init().unwrap();
    let settings = gtk4::Settings::default().unwrap();
    let previous = settings.is_gtk_enable_animations();
    let (view, window) = presented(1_000);
    let conn = view.connection.borrow().clone().unwrap();

    settings.set_gtk_enable_animations(false);
    view.prepare_entrance();
    view.refresh(&conn);
    let reduced_state = (
        view.render.ribbon.reveal_fraction(),
        view.render.ribbon.marker_opacity(),
        view.render.band_slide.opacity(),
        view.render.songs_slide.opacity(),
        view.render.genres_slide.opacity(),
        view.render.header_slide.offset_y(),
        view.render.hero.time_slide.offset_y(),
        view.render.hero.kpi_slide.offset_y(),
        view.render.band_slide.offset_y(),
        view.render.songs_slide.offset_y(),
        view.render.genres_slide.offset_y(),
        view.render
            .songs_card
            .summary_bars()
            .iter()
            .all(|bar| bar.value() > 0.0),
    );

    settings.set_gtk_enable_animations(true);
    view.prepare_entrance();
    view.refresh(&conn);
    wait_for(40);
    let animated_state = (
        view.render.ribbon.reveal_fraction(),
        view.render.hero.time.label().to_string(),
    );
    let final_copy = strings::stats_duration(
        view.current_snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .hero
            .total_ms,
    );
    settings.set_gtk_enable_animations(previous);

    assert_eq!(
        [
            reduced_state.0,
            reduced_state.1,
            reduced_state.2,
            reduced_state.3,
            reduced_state.4,
        ],
        [1.0; 5]
    );
    assert_eq!(
        [
            reduced_state.5,
            reduced_state.6,
            reduced_state.7,
            reduced_state.8,
            reduced_state.9,
            reduced_state.10,
        ],
        [0.0; 6]
    );
    assert!(reduced_state.11);
    assert!(
        animated_state.0 < 1.0 || animated_state.1 != final_copy,
        "the enabled-motion control run must still be in flight"
    );
    window.close();
}
