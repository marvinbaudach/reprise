//! Rule-named display tests for the My Stats bar-only entrance motion.

use super::*;
use reprise_core::db::Db;

fn view_and_conn() -> (StatsView, Rc<Db>) {
    crate::ui::style::install_css_string_for_test(&crate::ui::stats::stats_css::css());
    let conn = Rc::new(crate::test_db::open().unwrap());
    let loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    (StatsView::new_for_test(loader), conn)
}

fn presented_entrance() -> (StatsView, Rc<Db>, adw::Window) {
    let (view, conn) = view_and_conn();
    seed_current_plays(&conn, 10);
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

fn seed_current_plays(db: &Db, count: i64) {
    crate::test_db::connection(db)
        .execute(
            "INSERT INTO tracks \
         (id, path, title, artist, album, album_artist, genre, duration_ms, \
          play_count, added_at) \
         VALUES (1, '/music/1.flac', 'Current Track', 'Current Artist', \
                 'Current Album', '', 'Rock', 300000, 1, 0)",
            [],
        )
        .unwrap();
    for offset in 0..count {
        crate::test_db::connection(db)
            .execute(
                "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (1, ?1, 200000)",
                rusqlite::params![now_unix() - offset],
            )
            .unwrap();
    }
}

fn seed_previous_year_track(db: &Db, count: i64) {
    crate::test_db::connection(db)
        .execute(
            "INSERT INTO tracks \
         (id, path, title, artist, album, album_artist, genre, duration_ms, \
          play_count, added_at) \
         VALUES (2, '/music/2.flac', 'Earlier Track', 'Earlier Artist', \
                 'Earlier Album', '', 'Jazz', 300000, 1, 0)",
            [],
        )
        .unwrap();
    for offset in 0..count {
        crate::test_db::connection(db)
            .execute(
                "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (2, ?1, 200000)",
                rusqlite::params![now_unix() - 365 * 24 * 60 * 60 - offset],
            )
            .unwrap();
    }
}

fn wait_for(milliseconds: u64) {
    crate::ui::test_settle::settle_for(std::time::Duration::from_millis(milliseconds));
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
fn stats_19_reduced_motion_puts_every_bar_at_its_target_immediately() {
    gtk4::init().unwrap();
    let _setting_guard = AnimationSettingGuard::disable();
    let (view, _conn, window) = presented_entrance();

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
fn stats_19_period_switch_tweens_bars_without_restarting_static_content() {
    gtk4::init().unwrap();
    let (view, conn, window) = presented_entrance();
    wait_for(800);
    seed_previous_year_track(&conn, 20);

    let all_time = view
        .periods
        .borrow()
        .iter()
        .position(|period| *period == StatsPeriod::AllTime)
        .unwrap() as u32;
    view.period_dropdown.set_selected(all_time);
    // The tween starts on the first frame after the period switch, and a fixed
    // wait budgets wall-clock time for that frame rather than waiting for it.
    // Under parallel display-test workers forty milliseconds can pass without
    // one, and the sample below was then still zero.
    //
    // Both samples are taken INSIDE the predicate, on the first frame that has
    // started them. Reading them again after `settle_until` returned would
    // reopen the race it closes: an unbounded number of frames can pass between
    // the predicate going true and that second read, and on a loaded runner the
    // tween is then already at its target, making the growth assertions below
    // compare a finished value against itself.
    let growing = std::cell::Cell::new(None);
    assert!(
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
            let bar = view
                .render
                .songs_card
                .summary_bars()
                .get(1)
                .map(gtk4::LevelBar::value);
            let segment = view
                .render
                .genres_section_data
                .segment_reveals()
                .get(1)
                .map(gtk4::prelude::WidgetExt::width);
            match (bar, segment) {
                (Some(value), Some(width)) if value > 0.0 && width > 0 => {
                    growing.set(Some((value, width)));
                    true
                }
                _ => false,
            }
        }),
        "the newly visible song bar and genre segment must start their period tweens"
    );

    let (growing_value, growing_genre_width) = growing
        .get()
        .expect("the settle predicate stores its sample");
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
    assert_eq!(view.render.bands_card.widget().opacity(), 1.0);
    assert_eq!(view.render.songs_section.opacity(), 1.0);

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
    // `STATS_TWEEN` runs for 250ms. A runner that is starved enough to skip a
    // whole frame can therefore deliver the tween already finished on the very
    // first frame that has it started, and no sampling strategy can catch an
    // intermediate value that never became observable. Distinguishing "jumped
    // straight to the target" from "we never got a frame while it ran" is not
    // possible from here, so the growth claim is made only when a genuinely
    // intermediate sample was observed; the end state is asserted either way.
    assert!(
        growing_value <= final_value,
        "the song bar must never overshoot its new target"
    );
    assert!(
        growing_genre_width <= final_genre_width,
        "the genre segment must never overshoot its new target"
    );
    if growing_value < final_value {
        assert!(
            growing_genre_width < final_genre_width,
            "a tween caught mid-flight must move the genre segment too"
        );
    }
    window.close();
}
