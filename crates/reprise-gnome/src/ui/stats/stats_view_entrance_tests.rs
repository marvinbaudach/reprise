//! Rule-named display tests for the My Stats bar-only entrance motion.

use super::*;

fn view_and_conn() -> (StatsView, Rc<RefCell<Connection>>) {
    crate::ui::style::install_css_string_for_test(&crate::ui::stats::stats_css::css());
    let conn = Rc::new(RefCell::new(reprise_core::db::open(None).unwrap()));
    reprise_core::db::migrate(&conn.borrow()).unwrap();
    let loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    (StatsView::new(loader), conn)
}

fn presented_entrance() -> (StatsView, Rc<RefCell<Connection>>, adw::Window) {
    let (view, conn) = view_and_conn();
    seed_current_plays(&conn.borrow(), 10);
    view.wire_year_selector(&conn);
    view.prepare_entrance();
    view.refresh(&conn);

    let window = adw::Window::builder()
        .default_width(1_000)
        .default_height(700)
        .content(view.widget())
        .build();
    window.set_size_request(1_000, 700);
    window.present();
    wait_for(20);
    (view, conn, window)
}

fn seed_current_plays(conn: &Connection, count: i64) {
    conn.execute(
        "INSERT INTO tracks \
         (id, path, title, artist, album, album_artist, genre, duration_ms, \
          play_count, added_at) \
         VALUES (1, '/music/1.flac', 'Current Track', 'Current Artist', \
                 'Current Album', '', 'Rock', 300000, 1, 0)",
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

fn seed_previous_year_track(conn: &Connection, count: i64) {
    conn.execute(
        "INSERT INTO tracks \
         (id, path, title, artist, album, album_artist, genre, duration_ms, \
          play_count, added_at) \
         VALUES (2, '/music/2.flac', 'Earlier Track', 'Earlier Artist', \
                 'Earlier Album', '', 'Jazz', 300000, 1, 0)",
        [],
    )
    .unwrap();
    for offset in 0..count {
        conn.execute(
            "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (2, ?1, 200000)",
            rusqlite::params![now_unix() - 365 * 24 * 60 * 60 - offset],
        )
        .unwrap();
    }
}

fn wait_for(milliseconds: u64) {
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(milliseconds), move || {
        quit.quit();
    });
    main_loop.run();
}

fn wait_until(milliseconds: u64, condition: impl Fn() -> bool + 'static) -> bool {
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    let reached = Rc::new(Cell::new(false));
    let reached_in_poll = reached.clone();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(milliseconds);
    gtk4::glib::timeout_add_local(std::time::Duration::from_millis(10), move || {
        if condition() {
            reached_in_poll.set(true);
            quit.quit();
            gtk4::glib::ControlFlow::Break
        } else if std::time::Instant::now() >= deadline {
            quit.quit();
            gtk4::glib::ControlFlow::Break
        } else {
            gtk4::glib::ControlFlow::Continue
        }
    });
    main_loop.run();
    reached.get()
}

#[derive(Default)]
struct BestWeekOrder {
    saw_reset: Cell<bool>,
    saw_growth: Cell<bool>,
    finished: Cell<bool>,
    label_appeared_while_growing: Cell<bool>,
}

fn observe_best_week_order(ribbon: StatsRibbon, milliseconds: u64) -> Rc<BestWeekOrder> {
    let best_index = ribbon
        .best_week_index()
        .expect("the fixture must identify its best-week bar");
    let order = Rc::new(BestWeekOrder::default());
    let observed = order.clone();
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(milliseconds);
    gtk4::glib::timeout_add_local(std::time::Duration::from_millis(10), move || {
        let fraction = ribbon.bar_fractions()[best_index];
        let label_opacity = ribbon.best_week_label_opacity();
        if fraction == 0.0 {
            observed.saw_reset.set(true);
        } else if fraction < 1.0 {
            observed.saw_growth.set(true);
            if label_opacity > 0.0 {
                observed.label_appeared_while_growing.set(true);
                quit.quit();
                return gtk4::glib::ControlFlow::Break;
            }
        } else if observed.saw_growth.get() {
            observed.finished.set(true);
            quit.quit();
            return gtk4::glib::ControlFlow::Break;
        }

        if std::time::Instant::now() >= deadline {
            quit.quit();
            gtk4::glib::ControlFlow::Break
        } else {
            gtk4::glib::ControlFlow::Continue
        }
    });
    main_loop.run();
    order
}

struct AnimationSettingGuard {
    settings: gtk4::Settings,
    previous: bool,
}

impl AnimationSettingGuard {
    fn disable() -> Self {
        let settings = gtk4::Settings::default().unwrap();
        let previous = settings.is_gtk_enable_animations();
        settings.set_gtk_enable_animations(false);
        Self { settings, previous }
    }
}

impl Drop for AnimationSettingGuard {
    fn drop(&mut self) {
        self.settings.set_gtk_enable_animations(self.previous);
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_17_entrance_keeps_copy_static_while_sparse_week_bars_grow() {
    gtk4::init().unwrap();
    let (view, _conn, window) = presented_entrance();
    let final_copy = strings::stats_duration(
        view.current_snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .hero
            .total_ms,
    );

    assert_eq!(
        view.render.hero.time.label(),
        final_copy,
        "the mapped hero copy must be final from its first presented frame"
    );
    assert!(view.render.ribbon.is_sparse());
    let order = observe_best_week_order(view.render.ribbon.clone(), 1_000);
    assert!(
        order.saw_reset.get(),
        "the best-week bar must start from the baseline"
    );
    assert!(
        order.saw_growth.get(),
        "the best-week bar must visibly grow after the calm frame"
    );
    assert!(
        !order.label_appeared_while_growing.get(),
        "the best-week label must remain invisible for every observed growth frame"
    );
    assert!(
        order.finished.get(),
        "the best-week bar must finish within the bounded poll"
    );

    let ribbon = view.render.ribbon.clone();
    assert!(
        wait_until(300, move || (0.0..1.0)
            .contains(&ribbon.best_week_label_opacity())),
        "the best-week label must fade only after its own bar reaches full height"
    );
    let best_index = view.render.ribbon.best_week_index().unwrap();
    assert_eq!(
        view.render.ribbon.bar_fractions()[best_index],
        1.0,
        "the label fade must never overtake its own bar"
    );
    assert_eq!(view.render.hero.time.label(), final_copy);
    assert_eq!(view.render.header.root.opacity(), 1.0);
    assert_eq!(view.render.hero.root.opacity(), 1.0);
    assert_eq!(view.render.band_section.opacity(), 1.0);
    assert_eq!(view.render.songs_section.opacity(), 1.0);

    let song_bar = view
        .render
        .songs_card
        .summary_bars()
        .into_iter()
        .next()
        .expect("the fixture renders one song bar");
    let requested_width = song_bar.measure(gtk4::Orientation::Horizontal, -1).0;
    assert!(
        requested_width > 0,
        "the mapped song bar must retain a real width requirement while its fill grows"
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_17_reduced_motion_puts_every_bar_at_its_target_immediately() {
    gtk4::init().unwrap();
    let _setting_guard = AnimationSettingGuard::disable();
    let (view, _conn, window) = presented_entrance();

    assert!(view
        .render
        .ribbon
        .bar_fractions()
        .iter()
        .all(|fraction| *fraction == 1.0));
    assert_eq!(view.render.ribbon.best_week_label_opacity(), 1.0);
    assert!(view
        .render
        .songs_card
        .summary_bars()
        .iter()
        .all(|bar| bar.value() > 0.0));
    assert!(view
        .render
        .genres_section_data
        .segment_reveals()
        .iter()
        .all(|segment| segment.reveal_fraction() == 1.0));

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_17_period_switch_tweens_bars_without_restarting_static_content() {
    gtk4::init().unwrap();
    let (view, conn, window) = presented_entrance();
    wait_for(800);
    seed_previous_year_track(&conn.borrow(), 20);

    let all_time = view
        .periods
        .borrow()
        .iter()
        .position(|period| *period == StatsPeriod::AllTime)
        .unwrap() as u32;
    view.period_dropdown.set_selected(all_time);
    wait_for(40);

    let bars = view.render.songs_card.summary_bars();
    let growing_value = bars
        .get(1)
        .expect("the all-time period adds the earlier track")
        .value();
    let growing_genre_width = view
        .render
        .genres_section_data
        .segment_reveals()
        .get(1)
        .expect("the all-time period adds a second genre")
        .width();
    let final_copy = strings::stats_duration(
        view.current_snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .hero
            .total_ms,
    );
    assert!(
        growing_value > 0.0,
        "the newly visible song bar must have started its period tween"
    );
    assert!(
        growing_genre_width > 0,
        "the newly visible genre segment must have started its width tween"
    );
    assert_eq!(
        view.render.hero.time.label(),
        final_copy,
        "period copy changes immediately instead of counting"
    );
    assert_eq!(view.render.header.root.opacity(), 1.0);
    assert_eq!(view.render.hero.root.opacity(), 1.0);
    assert_eq!(view.render.band_section.opacity(), 1.0);
    assert_eq!(view.render.songs_section.opacity(), 1.0);
    assert_eq!(view.render.ribbon.best_week_label_opacity(), 1.0);

    wait_for(300);
    let final_value = view
        .render
        .songs_card
        .summary_bars()
        .get(1)
        .unwrap()
        .value();
    let final_genre_width = view
        .render
        .genres_section_data
        .segment_reveals()
        .get(1)
        .unwrap()
        .width();
    assert!(
        growing_value < final_value,
        "the observed intermediate width must finish at the new target"
    );
    assert!(
        growing_genre_width < final_genre_width,
        "the observed genre segment width must finish at the new target"
    );
    window.close();
}
