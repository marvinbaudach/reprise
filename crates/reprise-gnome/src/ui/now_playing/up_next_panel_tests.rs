use super::*;

fn track(id: i64) -> reprise_core::up_next::QueueItem {
    reprise_core::up_next::QueueItem::Track(id)
}

fn tracks(ids: &[i64]) -> Vec<reprise_core::up_next::QueueItem> {
    ids.iter().copied().map(track).collect()
}

fn context_window(ids: &[i64]) -> Rc<dyn crate::ui::track_list::queue_sections::ContextWindow> {
    Rc::new(ids.to_vec())
}

fn collect_buttons_with_class(widget: &gtk4::Widget, class: &str, buttons: &mut Vec<gtk4::Button>) {
    if let Ok(button) = widget.clone().downcast::<gtk4::Button>() {
        if button.has_css_class(class) {
            buttons.push(button);
        }
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        collect_buttons_with_class(&widget, class, buttons);
        child = widget.next_sibling();
    }
}

fn collect_label_texts(widget: &gtk4::Widget, labels: &mut Vec<String>) {
    if let Ok(label) = widget.clone().downcast::<gtk4::Label>() {
        labels.push(label.text().to_string());
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        collect_label_texts(&widget, labels);
        child = widget.next_sibling();
    }
}

#[test]
fn upcoming_tracks_are_manual_entries_then_the_snapshot_after_current() {
    let model = crate::ui::track_list::queue_sections::compose(
        Some(track(10)),
        &tracks(&[90, 91]),
        &[30, 40],
        Some("Music"),
    )
    .upcoming();
    assert_eq!(
        queue_rows(&model),
        vec![
            QueueRow::PlayNext(0),
            QueueRow::PlayNext(1),
            QueueRow::UpNext(0),
            QueueRow::UpNext(1),
        ]
    );
}

#[test]
fn upcoming_tracks_handle_an_empty_queue_and_current_at_the_end() {
    let empty = crate::ui::track_list::queue_sections::compose(None, &[], &[], None);
    assert!(queue_rows(&empty.upcoming()).is_empty());
    let only_current =
        crate::ui::track_list::queue_sections::compose(Some(track(20)), &[], &[], None);
    assert!(queue_rows(&only_current.upcoming()).is_empty());
    let manual =
        crate::ui::track_list::queue_sections::compose(Some(track(20)), &tracks(&[90]), &[], None)
            .upcoming();
    assert_eq!(queue_rows(&manual), vec![QueueRow::PlayNext(0)]);
}

#[test]
fn episode_context_rows_hide_remove_and_reorder_but_manual_episodes_do_not() {
    assert!(!row_is_editable(
        Some(QueueRow::UpNext(0)),
        reprise_core::up_next::QueueItem::Episode(8)
    ));
    assert!(row_is_editable(
        Some(QueueRow::PlayNext(0)),
        reprise_core::up_next::QueueItem::Episode(8)
    ));
    assert!(row_is_editable(
        Some(QueueRow::UpNext(0)),
        reprise_core::up_next::QueueItem::Track(8)
    ));
}

#[test]
fn que_2_two_sections_headers_conditional() {
    let both = crate::ui::track_list::queue_sections::compose(
        Some(track(10)),
        &tracks(&[20, 21]),
        &[30],
        Some("Late Night"),
    )
    .upcoming();
    assert_eq!(
        panel_section_headers(&both),
        vec![
            (0, "Next in Queue".to_owned()),
            (2, "Playing from Late Night · 1 track".to_owned()),
        ]
    );

    let automatic_only =
        crate::ui::track_list::queue_sections::compose(Some(track(10)), &[], &[30], Some("Album"))
            .upcoming();
    assert_eq!(
        panel_section_headers(&automatic_only),
        vec![(0, "Playing from Album · 1 track".to_owned())]
    );

    let manual_only =
        crate::ui::track_list::queue_sections::compose(Some(track(10)), &tracks(&[20]), &[], None)
            .upcoming();
    assert_eq!(
        panel_section_headers(&manual_only),
        vec![(0, "Next in Queue".to_owned())]
    );
    assert!(panel_section_headers(&QueueViewModel::default()).is_empty());
}

#[test]
fn footer_formats_track_count_and_remaining_duration() {
    assert_eq!(format_up_next_footer(&[]), "0 tracks · 0 minutes");
    assert_eq!(format_up_next_footer(&[90_000]), "1 track · 1 minute");
    assert_eq!(
        format_up_next_footer(&[90_000, 330_000]),
        "2 tracks · 7 minutes"
    );
}

#[test]
fn panel_drag_payload_and_edge_autoscroll_are_bounded() {
    assert_eq!(
        decode_drag_row(&encode_drag_row(QueueRow::PlayNext(3))),
        Some(QueueRow::PlayNext(3))
    );
    assert_eq!(
        decode_drag_row(&encode_drag_row(QueueRow::UpNext(7))),
        Some(QueueRow::UpNext(7))
    );
    assert_eq!(
        autoscroll_value(100.0, 0.0, 500.0, 100.0, 300.0, 20.0, 48.0, 24.0),
        76.0
    );
    assert_eq!(
        autoscroll_value(390.0, 0.0, 500.0, 100.0, 300.0, 290.0, 48.0, 24.0),
        400.0
    );
    assert_eq!(
        autoscroll_value(100.0, 0.0, 500.0, 100.0, 300.0, 150.0, 48.0, 24.0),
        100.0
    );
}

#[test]
fn panel_drop_payload_keeps_internal_reorder_and_typed_enqueue_separate() {
    assert_eq!(
        decode_drop_payload("manual:3"),
        Some(PanelDropPayload::Reorder(QueueRow::PlayNext(3)))
    );
    let external = crate::ui::track_list_dnd::format_drag_payload(
        &[
            reprise_core::up_next::QueueItem::Track(7),
            reprise_core::up_next::QueueItem::Episode(7),
        ],
        None,
    );
    assert_eq!(
        decode_drop_payload(&external),
        Some(PanelDropPayload::Enqueue(vec![
            reprise_core::up_next::QueueItem::Track(7),
            reprise_core::up_next::QueueItem::Episode(7),
        ]))
    );
    assert_eq!(decode_drop_payload("7|-"), None);
}

#[test]
fn acc_8_panel_keyboard_reorder_matches_the_drag_rows() {
    let alt = gtk4::gdk::ModifierType::ALT_MASK;
    assert_eq!(
        keyboard_reorder_rows(QueueRow::PlayNext(1), 3, gtk4::gdk::Key::Up, alt),
        Some((QueueRow::PlayNext(1), QueueRow::PlayNext(0)))
    );
    assert_eq!(
        keyboard_reorder_rows(QueueRow::PlayNext(1), 3, gtk4::gdk::Key::Down, alt),
        Some((QueueRow::PlayNext(1), QueueRow::PlayNext(2)))
    );
    assert_eq!(
        keyboard_reorder_rows(QueueRow::PlayNext(0), 3, gtk4::gdk::Key::Up, alt),
        None
    );
    assert_eq!(
        keyboard_reorder_rows(QueueRow::UpNext(0), 3, gtk4::gdk::Key::Down, alt),
        None
    );
}

#[test]
fn row_css_and_metrics_match_the_compact_21a_spec() {
    let css = css();
    assert_eq!(crate::ui::style::tokens::NOW_PLAYING_QUEUE_COVER_SIZE, 32);
    assert!(css.contains(".reprise-up-next-row"));
    assert!(css.contains("font-size: 13.5px"));
    assert!(!css.contains("reorder"));
    assert!(!css.contains("context-menu"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn up_next_row_click_jumps_to_the_exact_queue_entry() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute_batch(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES
         (20, '/tmp/20.mp3', 'Track 20', 'Artist', 0),
         (40, '/tmp/40.mp3', 'Track 40', 'Artist', 0);",
        )
        .unwrap();
    let cover_loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    let panel = UpNextPanel::new(conn, &cover_loader);
    let jumped = Rc::new(RefCell::new(None));
    let jumped_on_click = jumped.clone();
    panel.set_on_jump(move |row| *jumped_on_click.borrow_mut() = Some(row));
    let model = crate::ui::track_list::queue_sections::compose(
        Some(track(10)),
        &tracks(&[20]),
        &[40],
        Some("Music"),
    );
    panel.set_queue_model(&model, &context_window(&[40]));
    let window = gtk4::Window::builder().child(panel.widget()).build();
    window.present();
    while glib::MainContext::default().iteration(false) {}

    let mut buttons = Vec::new();
    collect_buttons_with_class(
        panel.widget().upcast_ref(),
        "reprise-up-next-row",
        &mut buttons,
    );
    buttons[1].emit_clicked();

    assert_eq!(*jumped.borrow(), Some(QueueRow::UpNext(0)));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn panel_remove_targets_the_exact_queue_entry() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute_batch(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES
         (20, '/tmp/20.mp3', 'Track 20', 'Artist', 0),
         (40, '/tmp/40.mp3', 'Track 40', 'Artist', 0);",
        )
        .unwrap();
    let cover_loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    let panel = UpNextPanel::new(conn, &cover_loader);
    let removed = Rc::new(RefCell::new(None));
    let removed_on_click = removed.clone();
    panel.set_on_remove(move |row| *removed_on_click.borrow_mut() = Some(row));
    let model = crate::ui::track_list::queue_sections::compose(
        Some(track(10)),
        &tracks(&[20]),
        &[40],
        Some("Music"),
    );
    panel.set_queue_model(&model, &context_window(&[40]));
    let window = gtk4::Window::builder().child(panel.widget()).build();
    window.present();
    while glib::MainContext::default().iteration(false) {}

    let mut buttons = Vec::new();
    collect_buttons_with_class(
        panel.widget().upcast_ref(),
        "reprise-up-next-remove",
        &mut buttons,
    );
    buttons[1].emit_clicked();

    assert_eq!(*removed.borrow(), Some(QueueRow::UpNext(0)));
}

/// A manual entry that survives an advance must still be removable. The
/// advance emits the O(1) leading-removal delta (`items_changed(0, 1, 0)`),
/// which shifts every surviving row down WITHOUT re-binding it — so a row
/// coordinate cached at bind time addresses the wrong entry afterwards.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn panel_remove_targets_the_exact_entry_after_the_queue_advanced() {
    use crate::ui::track_list::queue_sections::{compose_virtual, VirtualContext};

    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute_batch(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES
         (20, '/tmp/20.mp3', 'Track 20', 'Artist', 0),
         (21, '/tmp/21.mp3', 'Track 21', 'Artist', 0),
         (40, '/tmp/40.mp3', 'Track 40', 'Artist', 0);",
        )
        .unwrap();
    let cover_loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    let panel = UpNextPanel::new(conn, &cover_loader);
    let removed = Rc::new(RefCell::new(None));
    let removed_on_click = removed.clone();
    panel.set_on_remove(move |row| *removed_on_click.borrow_mut() = Some(row));

    // Playing track 10, two manual entries queued behind it, one context row.
    let before = compose_virtual(
        Some(track(10)),
        &tracks(&[20, 21]),
        Some(VirtualContext::identified(1, (5, 9), 1)),
        Some("Music"),
    );
    panel.set_queue_model(&before, &context_window(&[40]));
    let window = gtk4::Window::builder().child(panel.widget()).build();
    window.present();
    while glib::MainContext::default().iteration(false) {}

    // The song ends: manual entry 20 becomes Now Playing, 21 stays queued.
    let after = compose_virtual(
        Some(track(20)),
        &tracks(&[21]),
        Some(VirtualContext::identified(1, (5, 9), 1)),
        Some("Music"),
    );
    panel.set_queue_model(&after, &context_window(&[40]));
    while glib::MainContext::default().iteration(false) {}

    let mut titles = Vec::new();
    collect_label_texts(panel.widget().upcast_ref(), &mut titles);
    let mut buttons = Vec::new();
    collect_buttons_with_class(
        panel.widget().upcast_ref(),
        "reprise-up-next-remove",
        &mut buttons,
    );
    buttons[0].emit_clicked();

    assert_eq!(
        *removed.borrow(),
        Some(QueueRow::PlayNext(0)),
        "the only manual entry left is Play Next slot 0; rendered rows: {titles:?}"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn panel_jump_targets_the_exact_entry_after_the_queue_advanced() {
    use crate::ui::track_list::queue_sections::{compose_virtual, VirtualContext};

    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute_batch(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES
         (20, '/tmp/20.mp3', 'Track 20', 'Artist', 0),
         (21, '/tmp/21.mp3', 'Track 21', 'Artist', 0),
         (40, '/tmp/40.mp3', 'Track 40', 'Artist', 0);",
        )
        .unwrap();
    let cover_loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    let panel = UpNextPanel::new(conn, &cover_loader);
    let jumped = Rc::new(RefCell::new(None));
    let jumped_on_click = jumped.clone();
    panel.set_on_jump(move |row| *jumped_on_click.borrow_mut() = Some(row));

    let before = compose_virtual(
        Some(track(10)),
        &tracks(&[20, 21]),
        Some(VirtualContext::identified(1, (5, 9), 1)),
        Some("Music"),
    );
    panel.set_queue_model(&before, &context_window(&[40]));
    let window = gtk4::Window::builder().child(panel.widget()).build();
    window.present();
    while glib::MainContext::default().iteration(false) {}

    let after = compose_virtual(
        Some(track(20)),
        &tracks(&[21]),
        Some(VirtualContext::identified(1, (5, 9), 1)),
        Some("Music"),
    );
    panel.set_queue_model(&after, &context_window(&[40]));
    while glib::MainContext::default().iteration(false) {}

    let mut titles = Vec::new();
    collect_label_texts(panel.widget().upcast_ref(), &mut titles);
    let mut buttons = Vec::new();
    collect_buttons_with_class(
        panel.widget().upcast_ref(),
        "reprise-up-next-row",
        &mut buttons,
    );
    buttons[0].emit_clicked();

    assert_eq!(
        *jumped.borrow(),
        Some(QueueRow::PlayNext(0)),
        "the rendered Track 21 row is Play Next slot 0; rendered rows: {titles:?}"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn panel_context_tail_row_reports_its_shifted_coordinate_after_advance() {
    use crate::ui::track_list::queue_sections::{compose_virtual, VirtualContext};

    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute_batch(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES
         (20, '/tmp/20.mp3', 'Track 20', 'Artist', 0),
         (40, '/tmp/40.mp3', 'Track 40', 'Artist', 0),
         (41, '/tmp/41.mp3', 'Track 41', 'Artist', 0);",
        )
        .unwrap();
    let cover_loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    let panel = UpNextPanel::new(conn, &cover_loader);
    let jumped = Rc::new(RefCell::new(None));
    let jumped_on_click = jumped.clone();
    panel.set_on_jump(move |row| *jumped_on_click.borrow_mut() = Some(row));

    let before = compose_virtual(
        Some(track(10)),
        &tracks(&[20]),
        Some(VirtualContext::identified(2, (5, 9), 0)),
        Some("Music"),
    );
    panel.set_queue_model(&before, &context_window(&[40, 41]));
    let window = gtk4::Window::builder().child(panel.widget()).build();
    window.present();
    while glib::MainContext::default().iteration(false) {}

    let after = compose_virtual(
        Some(track(10)),
        &tracks(&[20]),
        Some(VirtualContext::identified(1, (5, 9), 1)),
        Some("Music"),
    );
    panel.set_queue_model(&after, &context_window(&[41]));
    while glib::MainContext::default().iteration(false) {}

    let mut titles = Vec::new();
    collect_label_texts(panel.widget().upcast_ref(), &mut titles);
    let mut buttons = Vec::new();
    collect_buttons_with_class(
        panel.widget().upcast_ref(),
        "reprise-up-next-row",
        &mut buttons,
    );
    buttons[1].emit_clicked();

    assert_eq!(
        *jumped.borrow(),
        Some(QueueRow::UpNext(0)),
        "the rendered Track 41 row shifted from context slot 1 to 0; rendered rows: {titles:?}"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mixed_queue_panel_renders_episode_title_and_show() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute_batch(
            "INSERT INTO tracks (id, path, title, artist, added_at)
             VALUES (7, '/tmp/track-seven.mp3', 'Track Seven', 'Track Artist', 0);
             INSERT INTO podcast_subscriptions
             (id, kind, feed_url, title, added_at)
             VALUES (1, 'rss', 'https://example.test/feed', 'Systems Weekly', 0);
             INSERT INTO podcast_episodes
             (id, subscription_id, guid, title, audio_url, duration_secs, first_seen_at)
             VALUES
             (7, 1, 'episode-seven', 'Episode Seven',
              'https://example.test/seven.mp3', 90, 0);",
        )
        .unwrap();
    let cover_loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    let panel = UpNextPanel::new(conn, &cover_loader);
    let model = crate::ui::track_list::queue_sections::compose(
        None,
        &[
            reprise_core::up_next::QueueItem::Track(7),
            reprise_core::up_next::QueueItem::Episode(7),
        ],
        &[],
        None,
    );
    panel.set_queue_model(&model, &context_window(&[]));
    let window = gtk4::Window::builder().child(panel.widget()).build();
    window.present();
    while glib::MainContext::default().iteration(false) {}

    let mut labels = Vec::new();
    collect_label_texts(panel.widget().upcast_ref(), &mut labels);
    assert!(labels.iter().any(|label| label == "Track Seven"));
    assert!(labels.iter().any(|label| label == "Episode Seven"));
    assert!(labels.iter().any(|label| label == "Systems Weekly"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn que_14_shifted_show_context_row_hides_the_remove_button() {
    use crate::ui::track_list::queue_sections::compose_virtual;

    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute_batch(
            "INSERT INTO tracks (id, path, title, artist, added_at)
             VALUES (20, '/tmp/track-twenty.mp3', 'Track Twenty', 'Artist', 0);
             INSERT INTO podcast_subscriptions
             (id, kind, feed_url, title, added_at)
             VALUES (1, 'rss', 'https://example.test/feed', 'Systems Weekly', 0);
             INSERT INTO podcast_episodes
             (id, subscription_id, guid, title, audio_url, duration_secs, first_seen_at)
             VALUES
             (21, 1, 'episode-twenty-one', 'Episode Twenty One',
              'https://example.test/twenty-one.mp3', 90, 0);",
        )
        .unwrap();
    let cover_loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    let panel = UpNextPanel::new(conn, &cover_loader);
    let empty_context: Rc<dyn crate::ui::track_list::queue_sections::ContextWindow> =
        Rc::new(Vec::<reprise_core::up_next::QueueItem>::new());

    let before = compose_virtual(
        Some(reprise_core::up_next::QueueItem::Episode(19)),
        &[track(20), reprise_core::up_next::QueueItem::Episode(21)],
        None,
        Some("Systems Weekly"),
    );
    panel.set_queue_model(&before, &empty_context);
    let window = gtk4::Window::builder().child(panel.widget()).build();
    window.present();
    while glib::MainContext::default().iteration(false) {}

    let mut before_buttons = Vec::new();
    collect_buttons_with_class(
        panel.widget().upcast_ref(),
        "reprise-up-next-remove",
        &mut before_buttons,
    );
    assert!(before_buttons[1].is_visible(), "the manual row is editable");

    let mut after = compose_virtual(
        Some(track(20)),
        &[reprise_core::up_next::QueueItem::Episode(21)],
        None,
        Some("Systems Weekly"),
    );
    // Preserve the leading-removal delta that keeps the bound ListItem alive,
    // while moving its surviving episode from the editable manual section to
    // the read-only show section. This is the exact transition whose stale
    // bind-time visibility left the remove control behind.
    let episode_section = after
        .sections
        .iter_mut()
        .find(|section| section.kind == QueueSectionKind::PlayNext)
        .unwrap();
    episode_section.kind = QueueSectionKind::UpNext {
        source_label: "Systems Weekly".into(),
    };
    panel.set_queue_model(&after, &empty_context);
    while glib::MainContext::default().iteration(false) {}

    let mut after_buttons = Vec::new();
    collect_buttons_with_class(
        panel.widget().upcast_ref(),
        "reprise-up-next-remove",
        &mut after_buttons,
    );
    assert_eq!(after_buttons.len(), 1);
    assert!(
        !after_buttons[0].is_visible(),
        "the shifted episode belongs to the read-only show context"
    );
}
