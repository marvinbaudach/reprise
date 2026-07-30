use super::*;

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

#[test]
fn upcoming_tracks_are_manual_entries_then_the_snapshot_after_current() {
    let model = crate::ui::track_list::queue_sections::compose(
        Some(10),
        &[90, 91],
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
    let only_current = crate::ui::track_list::queue_sections::compose(Some(20), &[], &[], None);
    assert!(queue_rows(&only_current.upcoming()).is_empty());
    let manual =
        crate::ui::track_list::queue_sections::compose(Some(20), &[90], &[], None).upcoming();
    assert_eq!(queue_rows(&manual), vec![QueueRow::PlayNext(0)]);
}

#[test]
fn que_2_two_sections_headers_conditional() {
    let both = crate::ui::track_list::queue_sections::compose(
        Some(10),
        &[20, 21],
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
        crate::ui::track_list::queue_sections::compose(Some(10), &[], &[30], Some("Album"))
            .upcoming();
    assert_eq!(
        panel_section_headers(&automatic_only),
        vec![(0, "Playing from Album · 1 track".to_owned())]
    );

    let manual_only =
        crate::ui::track_list::queue_sections::compose(Some(10), &[20], &[], None).upcoming();
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
    let model =
        crate::ui::track_list::queue_sections::compose(Some(10), &[20], &[40], Some("Music"));
    panel.set_queue_model(&model);
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
    let model =
        crate::ui::track_list::queue_sections::compose(Some(10), &[20], &[40], Some("Music"));
    panel.set_queue_model(&model);
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
