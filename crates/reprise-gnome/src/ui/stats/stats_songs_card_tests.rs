use chrono::Datelike;
use reprise_core::library::stats_period::StatsPeriod;
use reprise_core::library::stats_snapshot;

use super::*;

#[test]
fn compact_card_fills_six_song_rows() {
    assert_eq!(SONG_ROW_LIMIT, 6);
}

#[test]
fn compact_sort_toggle_names_map_to_the_shared_ranking_order() {
    assert_eq!(sort_for_toggle_name(Some("plays")), SortBy::Plays);
    assert_eq!(sort_for_toggle_name(Some("time")), SortBy::Time);
}

#[test]
fn summary_cover_generation_survives_rendering_the_full_ranking() {
    let generations = CoverGenerations::default();
    let summary_token = generations.next_summary();

    generations.next_full();

    assert_eq!(generations.summary.get(), summary_token);
}

fn card_and_snapshot(metadata: MetadataCallback) -> (StatsSongsCard, StatsSnapshot) {
    let conn = crate::test_db::open().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    for id in 1..=6_i64 {
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO tracks \
                 (id, path, title, artist, album, album_artist, genre, duration_ms, added_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, '', 'Rock', 300000, 0)",
                rusqlite::params![
                    id,
                    format!("/music/{id}.flac"),
                    format!("Track {id}"),
                    format!("Artist {id}"),
                    format!("Album {id}")
                ],
            )
            .unwrap();
        for play in 0..=(6 - id) {
            crate::test_db::connection(&conn)
                .execute(
                    "INSERT INTO listen_events (track_id, played_at, ms_played) \
                     VALUES (?1, ?2, ?3)",
                    rusqlite::params![id, now - play, id * 60_000],
                )
                .unwrap();
        }
    }
    let snapshot = stats_snapshot::compute(
        &conn,
        StatsPeriod::YearToDate(chrono::Local::now().year()),
        now,
        &chrono::Local,
    )
    .unwrap();
    let loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    (StatsSongsCard::new(loader, metadata), snapshot)
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_14_song_row_focuses_track_in_artist_scope() {
    gtk4::init().unwrap();
    let target = Rc::new(RefCell::new(None));
    let metadata: MetadataCallback = Rc::new(RefCell::new(Some({
        let target = target.clone();
        Rc::new(move |value| *target.borrow_mut() = Some(value))
    })));
    let (card, snapshot) = card_and_snapshot(metadata);
    card.set_data(&snapshot);

    card.summary.row_clicks.borrow()[0]
        .emit_by_name::<()>("released", &[&1_i32, &0.0_f64, &0.0_f64]);

    assert!(matches!(
        target.borrow().as_ref(),
        Some(StatsMetadataTarget::Artist {
            track_id: 1,
            artist
        }) if artist == "Artist 1"
    ));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_14_hover_play_targets_exactly_one_track() {
    gtk4::init().unwrap();
    crate::ui::style::install_css_string_for_test(&crate::ui::stats::stats_css::css());
    let metadata: MetadataCallback = Rc::new(RefCell::new(None));
    let (card, snapshot) = card_and_snapshot(metadata);
    let played = Rc::new(RefCell::new(Vec::new()));
    card.set_on_play_track({
        let played = played.clone();
        move |id| played.borrow_mut().push(id)
    });
    card.set_data(&snapshot);

    card.summary.play_buttons.borrow()[2].emit_clicked();

    assert_eq!(*played.borrow(), vec![3]);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_14_compact_toggle_sorts_six_rows_and_show_all_reveals_the_full_list() {
    gtk4::init().unwrap();
    crate::ui::style::install_css_string_for_test(&crate::ui::stats::stats_css::css());
    let metadata: MetadataCallback = Rc::new(RefCell::new(None));
    let (card, snapshot) = card_and_snapshot(metadata);
    card.set_data(&snapshot);

    assert!(!card.revealer.reveals_child());
    assert!(
        card.revealer.parent().is_none(),
        "the expanded ranking must not live inside the songs card"
    );
    let stage = gtk4::Box::new(gtk4::Orientation::Vertical, 20);
    stage.append(card.widget());
    stage.append(card.expanded_widget());
    let window = gtk4::Window::builder()
        .default_width(960)
        .child(&stage)
        .build();
    window.present();
    card.revealer
        .set_transition_type(gtk4::RevealerTransitionType::None);
    run_main_loop_for_layout();

    assert_eq!(card.sort_toggle.n_toggles(), 2);
    assert_eq!(
        card.sort_toggle.toggle(0).unwrap().label().as_deref(),
        Some("by plays")
    );
    assert_eq!(
        card.sort_toggle.toggle(1).unwrap().label().as_deref(),
        Some("by time")
    );
    assert_eq!(card.sort_toggle.active_name().as_deref(), Some("plays"));
    assert_eq!(
        card.reveal_button.label().as_deref(),
        Some("Show all top tracks")
    );
    assert_eq!(card.summary.rows.observe_children().n_items(), 6);
    card.sort_toggle.set_active_name(Some("time"));
    assert_eq!(card.sort_by.get(), SortBy::Time);
    assert_eq!(
        descendant_labels(&card.summary.rows.first_child().unwrap())[0],
        "Track 3"
    );

    card.reveal_button.emit_clicked();
    assert!(card.revealer.reveals_child());
    assert_eq!(card.full_rows.observe_children().n_items(), 6);
    card.sort_toggle.set_active_name(Some("plays"));
    assert_eq!(card.sort_by.get(), SortBy::Plays);
    run_main_loop_for_layout();

    let row = card.full_rows.first_child().unwrap();
    let rank = row.first_child().unwrap();
    let cover = rank.next_sibling().unwrap();
    let text = cover.next_sibling().unwrap();
    let bar = text.next_sibling().unwrap();
    assert_eq!(row.height_request(), 56);
    assert!(
        row.height() <= 64,
        "expanded row was {} px tall",
        row.height()
    );
    assert_eq!(rank.width(), 24);
    assert_eq!((cover.width(), cover.height()), (42, 42));
    assert_eq!(bar.height(), 8);
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_14_context_actions_target_the_same_track() {
    gtk4::init().unwrap();
    let opened = Rc::new(RefCell::new(None));
    let metadata: MetadataCallback = Rc::new(RefCell::new(Some({
        let opened = opened.clone();
        Rc::new(move |value| *opened.borrow_mut() = Some(value))
    })));
    let (card, snapshot) = card_and_snapshot(metadata);
    let next = Rc::new(RefCell::new(Vec::new()));
    let queued = Rc::new(RefCell::new(Vec::new()));
    card.set_on_play_next({
        let next = next.clone();
        move |id| next.borrow_mut().push(id)
    });
    card.set_on_add_to_queue({
        let queued = queued.clone();
        move |id| queued.borrow_mut().push(id)
    });
    card.set_data(&snapshot);

    let actions = &card.summary.context_actions.borrow()[1];
    actions.lookup_action("play-next").unwrap().activate(None);
    actions
        .lookup_action("add-to-queue")
        .unwrap()
        .activate(None);
    actions.lookup_action("open-album").unwrap().activate(None);

    assert_eq!(*next.borrow(), vec![2]);
    assert_eq!(*queued.borrow(), vec![2]);
    assert!(matches!(
        opened.borrow().as_ref(),
        Some(StatsMetadataTarget::Album { track_id: 2, .. })
    ));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn discarded_song_rows_release_their_context_widgets() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let metadata: MetadataCallback = Rc::new(RefCell::new(None));
    let (card, snapshot) = card_and_snapshot(metadata);
    card.set_data(&snapshot);
    let old_row = card.summary.rows.first_child().unwrap();
    let old_row = old_row.downcast::<gtk4::Box>().unwrap();
    let weak_row = old_row.downgrade();
    drop(old_row);

    card.set_data(&snapshot);
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }

    assert!(
        weak_row.upgrade().is_none(),
        "a discarded row must not be retained by its input controllers"
    );
}

fn descendant_labels(root: &gtk4::Widget) -> Vec<String> {
    let mut labels = Vec::new();
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
            labels.push(label.label().to_string());
        }
        labels.extend(descendant_labels(&widget));
        child = widget.next_sibling();
    }
    labels
}

fn run_main_loop_for_layout() {
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
        quit.quit();
    });
    main_loop.run();
}
