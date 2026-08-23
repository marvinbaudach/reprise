use super::*;

fn payload_number(line: &str, field: &str) -> u128 {
    line.split_whitespace()
        .find_map(|part| part.strip_prefix(&format!("{field}=")))
        .expect("payload field must exist")
        .parse()
        .expect("payload field must be numeric")
}

fn reload_lines(trail: &DiagnosticTrail) -> Vec<String> {
    trail
        .snapshot()
        .into_iter()
        .filter(|line| line.contains(" Reload "))
        .collect()
}

#[test]
fn reload_measurement_records_work_query_and_later_frame_honestly() {
    let trail = DiagnosticTrail::default();
    let started = Instant::now();
    let pending = ReloadTimer::started_at(started, ReloadCause::TypedSearch).work_done_at(
        started + Duration::from_millis(17),
        "Library",
        42,
        Some(Duration::from_millis(11)),
    );

    assert!(
        trail.snapshot().is_empty(),
        "work completion is not a frame"
    );
    pending.next_frame_at(&trail, started + Duration::from_millis(23));

    let line = &trail.snapshot()[0];
    assert!(line.contains("cause=typed-search"), "{line}");
    assert!(line.contains("source=Library"), "{line}");
    assert!(line.contains("rows=42"), "{line}");
    assert!(line.contains("query_us=11000"), "{line}");
    assert!(line.contains("work_done_us=17000"), "{line}");
    assert!(line.contains("next_frame_us=23000"), "{line}");

    let query_us = payload_number(line, "query_us");
    let work_done_us = payload_number(line, "work_done_us");
    let next_frame_us = payload_number(line, "next_frame_us");
    assert!(work_done_us >= query_us);
    assert!(next_frame_us >= work_done_us);
}

#[test]
fn queue_reload_measurement_distinguishes_no_query_from_zero_duration() {
    let trail = DiagnosticTrail::default();
    let started = Instant::now();
    ReloadTimer::started_at(started, ReloadCause::SourceSwitch)
        .work_done_at(started + Duration::from_millis(2), "Queue", 0, None)
        .next_frame_at(&trail, started + Duration::from_millis(5));

    let line = &trail.snapshot()[0];
    assert!(line.contains("query_us=none"), "{line}");
    assert!(line.contains("work_done_us=2000"), "{line}");
    assert!(line.contains("next_frame_us=5000"), "{line}");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn production_reload_records_the_real_frame_rows_cause_and_optional_query() {
    use gtk4::prelude::*;

    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    crate::test_db::connection(&conn)
        .execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (1, '/synthetic/1.flac', 'Synthetic Track', 'Synthetic Artist', 0)",
            [],
        )
        .unwrap();
    let track_list = super::super::TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        super::super::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();
    crate::ui::test_settle::settle_until_mapped(track_list.widget());
    while gtk4::glib::MainContext::default().iteration(false) {}

    let before_library = reload_lines(&track_list.shared.diagnostic_trail).len();
    super::super::track_list_reload::set_filter_and_reload(&track_list.shared, "Synthetic");
    assert_eq!(
        reload_lines(&track_list.shared.diagnostic_trail).len(),
        before_library,
        "synchronous work completion must not masquerade as a painted frame"
    );
    assert!(crate::ui::test_settle::settle_until(
        crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
        || reload_lines(&track_list.shared.diagnostic_trail).len() > before_library
    ));
    let library = reload_lines(&track_list.shared.diagnostic_trail)
        .pop()
        .unwrap();
    assert!(library.contains("cause=typed-search"), "{library}");
    assert!(library.contains("rows=1"), "{library}");
    assert!(!library.contains("query_us=none"), "{library}");
    assert!(
        payload_number(&library, "next_frame_us") >= payload_number(&library, "work_done_us"),
        "{library}"
    );

    let before_queue = reload_lines(&track_list.shared.diagnostic_trail).len();
    super::super::track_list_reload::set_source_and_reload(
        &track_list.shared,
        &reprise_core::view_source::ViewSource::Queue,
    );
    assert_eq!(
        reload_lines(&track_list.shared.diagnostic_trail).len(),
        before_queue,
        "queue work completion must also wait for a frame"
    );
    assert!(crate::ui::test_settle::settle_until(
        crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
        || reload_lines(&track_list.shared.diagnostic_trail).len() > before_queue
    ));
    let queue = reload_lines(&track_list.shared.diagnostic_trail)
        .pop()
        .unwrap();
    assert!(queue.contains("cause=source-switch"), "{queue}");
    assert!(queue.contains("rows=0"), "{queue}");
    assert!(queue.contains("query_us=none"), "{queue}");
    window.close();
}

#[test]
fn reload_cause_distinguishes_search_clear_sort_and_source_transitions() {
    use reprise_core::view_source::ViewSource;

    let baseline = (
        ViewSource::Library,
        "artist".to_string(),
        "asc".to_string(),
        String::new(),
    );
    assert_eq!(
        super::super::track_list_reload::reload_cause(
            Some(&baseline),
            &ViewSource::Library,
            "artist",
            "asc",
            "n"
        ),
        ReloadCause::TypedSearch
    );
    let mid_search = (
        ViewSource::Library,
        "artist".to_string(),
        "asc".to_string(),
        "n".to_string(),
    );
    assert_eq!(
        super::super::track_list_reload::reload_cause(
            Some(&mid_search),
            &ViewSource::Library,
            "artist",
            "asc",
            "ne"
        ),
        ReloadCause::TypedSearch
    );
    assert_eq!(
        super::super::track_list_reload::reload_cause(
            Some(&mid_search),
            &ViewSource::Library,
            "artist",
            "asc",
            ""
        ),
        ReloadCause::ClearedSearch
    );
    assert_eq!(
        super::super::track_list_reload::reload_cause(
            Some(&baseline),
            &ViewSource::Library,
            "title",
            "asc",
            ""
        ),
        ReloadCause::SortChange
    );
    assert_eq!(
        super::super::track_list_reload::reload_cause(
            Some(&baseline),
            &ViewSource::Playlist(7),
            "playlist_order",
            "asc",
            ""
        ),
        ReloadCause::SourceSwitch
    );
    assert_eq!(
        super::super::track_list_reload::reload_cause(
            None,
            &ViewSource::Library,
            "artist",
            "asc",
            ""
        ),
        ReloadCause::Other
    );
}

#[test]
#[ignore = "measurement: uses the generated database under the isolated XDG data root"]
fn measure_generated_library_reload_latency() {
    use gtk4::prelude::*;

    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let db_path = reprise_core::db::default_path();
    let conn = reprise_core::db::Db::open_ready(&db_path).unwrap();
    let track_list = super::super::TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        super::super::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let window = gtk4::Window::builder()
        .default_width(1600)
        .default_height(1000)
        .child(track_list.widget())
        .build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    for sample in 1..=5 {
        run_and_print(&track_list.shared, sample, "first-keystroke", || {
            super::super::track_list_reload::set_filter_and_reload(&track_list.shared, "N");
        });
        run_and_print(&track_list.shared, sample, "mid-typing", || {
            super::super::track_list_reload::set_filter_and_reload(&track_list.shared, "Ne");
        });
        run_and_print(&track_list.shared, sample, "clear-search", || {
            super::super::track_list_reload::set_filter_and_reload(&track_list.shared, "");
        });
        run_and_print(&track_list.shared, sample, "sort-change", || {
            *track_list.shared.sort.borrow_mut() = crate::ui::track_list_sort::SortState {
                field: "title".into(),
                dir: "asc".into(),
            };
            super::super::track_list_reload::reload(&track_list.shared);
        });
        run_and_print(&track_list.shared, sample, "source-to-missing", || {
            super::super::track_list_reload::set_source_and_reload(
                &track_list.shared,
                &reprise_core::view_source::ViewSource::Missing,
            );
        });
        run_and_print(&track_list.shared, sample, "source-to-library", || {
            super::super::track_list_reload::set_source_and_reload(
                &track_list.shared,
                &reprise_core::view_source::ViewSource::Library,
            );
        });
    }
    window.close();
}

fn run_and_print(shared: &super::super::Shared, sample: usize, case: &str, run: impl FnOnce()) {
    let before = reload_lines(&shared.diagnostic_trail).len();
    run();
    assert!(crate::ui::test_settle::settle_until(
        crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
        || reload_lines(&shared.diagnostic_trail).len() > before
    ));
    let line = reload_lines(&shared.diagnostic_trail).pop().unwrap();
    eprintln!("RELOAD_SAMPLE sample={sample} case={case} {line}");
}

#[test]
fn trail_keeps_the_newest_64_entries_in_oldest_first_order() {
    let trail = DiagnosticTrail::default();
    for count in 0..70 {
        trail.push(
            count,
            Event::Reload {
                cause: ReloadCause::Other,
                source: "library".into(),
                rows: count as usize,
                query_us: Some(1),
                work_done_us: 2,
                next_frame_us: 3,
            },
        );
    }

    let lines = trail.snapshot();
    assert_eq!(lines.len(), 64);
    assert!(lines[0].contains("rows=6"), "{}", lines[0]);
    assert!(lines[63].contains("rows=69"), "{}", lines[63]);
}

#[test]
fn trail_renders_elapsed_category_and_payload_on_one_line() {
    let trail = DiagnosticTrail::default();
    trail.push(
        42,
        Event::PlaybackState {
            state: "playing".into(),
        },
    );

    assert_eq!(trail.snapshot(), ["42ms PlaybackState state=playing"]);
}

#[test]
fn sections_changed_renders_its_exact_range() {
    let trail = DiagnosticTrail::default();
    trail.push(
        9,
        Event::SectionsChanged {
            position: 3,
            n_items: 12,
        },
    );
    assert_eq!(
        trail.snapshot(),
        ["9ms SectionsChanged position=3 n_items=12"]
    );
}

#[test]
fn trail_truncates_long_payloads_without_splitting_unicode() {
    let trail = DiagnosticTrail::default();
    trail.push(
        7,
        Event::Reload {
            cause: ReloadCause::Other,
            source: format!("{}\nsecond line", "ä".repeat(200)),
            rows: 1,
            query_us: Some(1),
            work_done_us: 2,
            next_frame_us: 3,
        },
    );

    let line = &trail.snapshot()[0];
    let payload = line.splitn(3, ' ').nth(2).unwrap();
    assert_eq!(payload.chars().count(), PAYLOAD_LIMIT);
    assert!(payload.ends_with('…'));
    assert_eq!(line.lines().count(), 1);
}
