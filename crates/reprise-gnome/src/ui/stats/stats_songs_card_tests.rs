use chrono::Datelike;
use reprise_core::library::stats_period::StatsPeriod;
use reprise_core::library::stats_snapshot;

use super::*;

#[test]
fn stats_19_the_card_shows_a_full_top_ten_over_two_columns() {
    assert_eq!(SONG_ROW_LIMIT, 10);
    assert_eq!(SUMMARY_COLUMN_ROWS, 5);
    // The expander continues past the ten rows already on screen.
    assert_eq!(FULL_TRACK_EXTRA, 15);
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
    card_and_snapshot_with(metadata, 6)
}

/// `tracks` plays descend with the id, so rank order is the id order.
fn card_and_snapshot_with(
    metadata: MetadataCallback,
    tracks: i64,
) -> (StatsSongsCard, StatsSnapshot) {
    let conn = crate::test_db::open().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    for id in 1..=tracks {
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
        for play in 0..=(tracks - id) {
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
fn stats_19_the_row_plays_and_its_labels_navigate() {
    gtk4::init().unwrap();
    let target = Rc::new(RefCell::new(None));
    let metadata: MetadataCallback = Rc::new(RefCell::new(Some({
        let target = target.clone();
        Rc::new(move |value| *target.borrow_mut() = Some(value))
    })));
    let (card, snapshot) = card_and_snapshot(metadata);
    let played = recorder(&card);
    card.set_data(&snapshot);

    // The row itself is the play affordance.
    card.summary.row_clicks.borrow()[0]
        .emit_by_name::<()>("released", &[&1_i32, &0.0_f64, &0.0_f64]);
    assert_eq!(*played.borrow(), vec![(vec![1, 2, 3, 4, 5, 6], 0)]);
    assert!(
        target.borrow().is_none(),
        "playing a row must not also navigate away from it"
    );
}

/// STATS-21: the row starts *its* track — and hands over the ranking around
/// it, so Previous/Next, Shuffle and the Queue have a context to work with
/// instead of one orphaned track.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_21_a_row_click_starts_its_track_inside_the_visible_ranking() {
    gtk4::init().unwrap();
    crate::ui::style::install_css_string_for_test(&crate::ui::stats::stats_css::css());
    let metadata: MetadataCallback = Rc::new(RefCell::new(None));
    let (card, snapshot) = card_and_snapshot(metadata);
    let played = recorder(&card);
    card.set_data(&snapshot);

    card.summary.row_clicks.borrow()[2]
        .emit_by_name::<()>("released", &[&1_i32, &0.0_f64, &0.0_f64]);

    let calls = played.borrow();
    let (ids, index) = calls.first().expect("the row started playback");
    assert_eq!(*ids, vec![1, 2, 3, 4, 5, 6], "the whole visible ranking");
    assert_eq!(*index, 2, "starting at the row that was clicked");
    assert_eq!(ids[*index], 3, "which is still exactly that row's track");
}

/// The context follows the sort the user is looking at: re-sorting rebuilds
/// the rows, so a play never seeds the queue in the previous order.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_21_the_play_context_follows_the_active_sort() {
    gtk4::init().unwrap();
    crate::ui::style::install_css_string_for_test(&crate::ui::stats::stats_css::css());
    let metadata: MetadataCallback = Rc::new(RefCell::new(None));
    let (card, snapshot) = card_and_snapshot(metadata);
    let played = recorder(&card);
    card.set_data(&snapshot);

    card.sort_toggle.set_active_name(Some("time"));
    card.summary.row_clicks.borrow()[0]
        .emit_by_name::<()>("released", &[&1_i32, &0.0_f64, &0.0_f64]);

    let calls = played.borrow();
    let (ids, index) = calls.first().expect("the row started playback");
    let by_time = snapshot
        .top_tracks_sorted(SortBy::Time)
        .iter()
        .map(|track| track.track_id)
        .collect::<Vec<_>>();
    assert_eq!(*ids, by_time, "the queue follows the ranking on screen");
    assert_eq!(*index, 0);
}

/// One recorded activation: the context handed over and the row inside it.
type PlayCalls = Rc<RefCell<Vec<(Vec<i64>, usize)>>>;

/// Records `(ids, index)` per activation, so a test can assert the whole
/// context and not just the track that starts.
fn recorder(card: &StatsSongsCard) -> PlayCalls {
    let played = Rc::new(RefCell::new(Vec::new()));
    card.set_on_play_track({
        let played = played.clone();
        move |ids: &[i64], index| played.borrow_mut().push((ids.to_vec(), index))
    });
    played
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_19_toggle_sorts_both_columns_and_show_all_reveals_the_full_list() {
    gtk4::init().unwrap();
    crate::ui::style::install_css_string_for_test(&crate::ui::stats::stats_css::css());
    let metadata: MetadataCallback = Rc::new(RefCell::new(None));
    let (card, snapshot) = card_and_snapshot_with(metadata, 13);
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
        Some("Show more top tracks")
    );
    // Two columns of five: thirteen tracks fill the visible top ten.
    assert_eq!(card.summary.rows.observe_children().n_items(), 2);
    assert_eq!(card.summary.columns[0].observe_children().n_items(), 5);
    assert_eq!(card.summary.columns[1].observe_children().n_items(), 5);
    card.sort_toggle.set_active_name(Some("time"));
    assert_eq!(card.sort_by.get(), SortBy::Time);
    assert_eq!(
        descendant_labels(&card.summary.columns[0].first_child().unwrap())[1],
        "Track 5",
        // Listened time is capped at the track duration, so the fixture's
        // time leader is the one with the most *complete* plays, not the one
        // with the largest raw ms_played.
        "the rank slot prints first, then the title"
    );

    card.sort_toggle.set_active_name(Some("plays"));
    assert_eq!(card.sort_by.get(), SortBy::Plays);
    card.reveal_button.emit_clicked();
    assert!(card.revealer.reveals_child());
    // The expander continues the ranking instead of restating it: ten rows
    // are already on screen, so only 11-13 remain.
    assert_eq!(card.full_rows.observe_children().n_items(), 3);
    assert_eq!(
        descendant_labels(&card.full_rows.first_child().unwrap())[0],
        "11",
        "the expanded list picks up where the card stopped"
    );
    assert!(
        card.summary.playbacks.borrow().len() == 10 && card.full_playbacks.borrow().len() == 3,
        "STATS-18: both lists register every row for the shared marker"
    );
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
fn stats_19_the_expander_is_only_offered_when_it_has_something_to_add() {
    gtk4::init().unwrap();
    let metadata: MetadataCallback = Rc::new(RefCell::new(None));

    // Six tracks all fit in the visible top ten — opening the expander would
    // reveal an empty card.
    let (card, snapshot) = card_and_snapshot_with(metadata.clone(), 6);
    card.set_data(&snapshot);
    assert!(!card.reveal_button.is_visible());
    assert_eq!(card.full_rows.observe_children().n_items(), 0);

    // Eleven tracks leave exactly one for the expander.
    let (card, snapshot) = card_and_snapshot_with(metadata, 11);
    card.set_data(&snapshot);
    assert!(card.reveal_button.is_visible());
    assert_eq!(card.full_rows.observe_children().n_items(), 1);
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
    let old_row = card.summary.columns[0].first_child().unwrap();
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
