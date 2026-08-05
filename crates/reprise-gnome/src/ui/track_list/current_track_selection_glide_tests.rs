use super::*;

fn build_track_list() -> Rc<TrackList> {
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=100 {
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
            (
                id,
                format!("/synthetic/{id:03}.flac"),
                format!("Track {id:03}"),
            ),
        )
        .unwrap();
    }
    tx.commit().unwrap();
    Rc::new(TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        super::super::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    ))
}

fn present(track_list: &TrackList) -> gtk4::Window {
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();
    crate::ui::test_settle::settle_until_mapped(track_list.widget());
    window
}

fn target_for(track_list: &TrackList, position: u32) -> (gtk4::Adjustment, f64) {
    scroll_center::centered_scroll_target(
        &track_list.shared.column_view,
        track_table_row_count(&track_list.shared.column_view),
        position,
    )
    .expect("the allocated long list must have centering geometry")
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_10b_centering_lands_exactly_on_the_target() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    gtk4::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(true);
    let track_list = build_track_list();
    let window = present(&track_list);
    let position = 50;
    let (adjustment, target) = target_for(&track_list, position);
    let start = target - adjustment.page_size();
    adjustment.set_value(start);

    crate::ui::track_list::track_reveal::reveal_position(&track_list.shared, position, 8);
    assert!(
        (adjustment.value() - target).abs() > 0.5,
        "centering must begin a glide instead of teleporting"
    );
    let landed =
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
            adjustment.value() == target
        });

    assert!(
        landed,
        "centering did not reach the exact target: actual {}, target {target}",
        adjustment.value()
    );
    assert_eq!(adjustment.value(), target);
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_10b_a_user_scroll_during_the_glide_wins() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    gtk4::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(true);
    let track_list = build_track_list();
    let window = present(&track_list);
    let position = 50;
    let (adjustment, target) = target_for(&track_list, position);
    let start = target - adjustment.page_size();
    adjustment.set_value(start);

    crate::ui::track_list::track_reveal::reveal_position(&track_list.shared, position, 8);
    let started =
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
            (adjustment.value() - start).abs() > 0.5
        });
    assert!(started, "centering must produce an in-flight glide");
    assert_ne!(adjustment.value(), target);
    let user_value = adjustment.value() + 43.0;
    assert!((user_value - target).abs() > 0.5);
    adjustment.set_value(user_value);

    crate::ui::test_settle::settle_for(std::time::Duration::from_millis(400));
    assert_eq!(adjustment.value(), user_value);
    window.close();
}

#[test]
fn visible_position_finds_the_current_track_in_view_order() {
    assert_eq!(
        visible_position_for_track_in_source(&[41, 42, 43], 42, None, false),
        Some(1)
    );
}

#[test]
fn visible_position_uses_queue_occurrence_then_falls_back_to_first_match() {
    assert_eq!(
        visible_position_for_track_in_source(&[7, 8, 7], 7, Some(2), false),
        Some(2)
    );
    assert_eq!(
        visible_position_for_track_in_source(&[7, 8, 7], 7, Some(1), false),
        Some(0)
    );
    assert_eq!(
        visible_position_for_track_in_source(&[7, 8, 7], 9, None, false),
        None
    );
}

#[test]
fn queue_does_not_highlight_a_pending_duplicate_of_the_current_track() {
    assert_eq!(
        visible_position_for_track_in_source(&[7, 8, 7], 7, None, true),
        None
    );
    assert_eq!(
        visible_position_for_track_in_source(&[7, 8, 7], 7, None, false),
        Some(0)
    );
}
