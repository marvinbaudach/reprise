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

fn breakdown_lines(trail: &DiagnosticTrail) -> Vec<String> {
    trail
        .snapshot()
        .into_iter()
        .filter(|line| line.contains(" ReloadBreakdown "))
        .collect()
}

#[test]
fn per_item_timing_reads_no_clock_until_reload_recording_is_armed() {
    reset_recording_timestamp_reads();
    let item = measure_item_call(|| 42);

    assert_eq!(item, 42);
    assert_eq!(recording_timestamp_reads(), 0);

    arm_reload_recording();
    reset_recording_timestamp_reads();
    let item = measure_item_call(|| 84);

    assert_eq!(item, 84);
    assert_eq!(recording_timestamp_reads(), 2);
}

#[test]
fn reload_breakdown_and_step_timing_are_opt_in() {
    reset_recording_timestamp_reads();
    assert!(begin_reload_breakdown().is_none());
    assert_eq!(measure_reload_step(ReloadStep::Geometry, || 42), 42);
    assert_eq!(recording_timestamp_reads(), 0);

    arm_reload_recording();
    assert!(begin_reload_breakdown().is_some());
    reset_recording_timestamp_reads();
    assert_eq!(measure_reload_step(ReloadStep::Geometry, || 84), 84);
    assert_eq!(recording_timestamp_reads(), 2);
}

#[test]
fn every_reload_step_has_one_bounded_breakdown_slot() {
    assert_eq!(RELOAD_STEP_COUNT, ReloadStep::ALL.len());
    for (expected, &step) in ReloadStep::ALL.iter().enumerate() {
        assert_eq!(step.index(), expected);
    }
}

#[test]
fn reload_measurement_records_work_query_and_later_frame_honestly() {
    let trail = DiagnosticTrail::default();
    let started = Instant::now();
    let pending = ReloadTimer::started_at(started, ReloadCause::TypedSearch, 7).work_done_at(
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
    ReloadTimer::started_at(started, ReloadCause::SourceSwitch, 8)
        .work_done_at(started + Duration::from_millis(2), "Queue", 0, None)
        .next_frame_at(&trail, started + Duration::from_millis(5));

    let line = &trail.snapshot()[0];
    assert!(line.contains("query_us=none"), "{line}");
    assert!(line.contains("work_done_us=2000"), "{line}");
    assert!(line.contains("next_frame_us=5000"), "{line}");
}

#[test]
fn reload_breakdown_sums_inside_the_whole_and_resets_per_reload() {
    let trail = DiagnosticTrail::default();
    arm_reload_recording();
    let (_, first) = begin_reload_breakdown().unwrap();
    record_reload_step(ReloadStep::Geometry, Duration::from_millis(2));
    record_reload_step(ReloadStep::ItemsChanged, Duration::from_millis(5));
    record_item_call(Duration::from_millis(3));
    record_window_query(Duration::from_millis(1));
    finish_reload_breakdown(
        &trail,
        first,
        Duration::from_millis(10),
        100,
        100_000,
        4,
        25.0,
        400.0,
    );

    let first_line = trail.snapshot().pop().unwrap();
    assert!(
        first_line.contains(&format!("reload_id={first}")),
        "{first_line}"
    );
    assert!(first_line.contains("step_sum_us=7000"), "{first_line}");
    assert!(first_line.contains("whole_us=10000"), "{first_line}");
    assert!(first_line.contains("item_calls=1"), "{first_line}");
    assert!(first_line.contains("window_calls=1"), "{first_line}");

    let (_, second) = begin_reload_breakdown().unwrap();
    finish_reload_breakdown(
        &trail,
        second,
        Duration::from_millis(1),
        100_000,
        100_000,
        0,
        0.0,
        400.0,
    );
    let second_line = trail.snapshot().pop().unwrap();
    assert!(
        second_line.contains(&format!("reload_id={second}")),
        "{second_line}"
    );
    assert!(second_line.contains("step_sum_us=0"), "{second_line}");
    assert!(second_line.contains("item_calls=0"), "{second_line}");
    assert!(second_line.contains("window_calls=0"), "{second_line}");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn production_reload_records_the_real_frame_rows_cause_and_optional_query() {
    use gtk4::prelude::*;

    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    arm_reload_recording();
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
    let breakdown = breakdown_lines(&track_list.shared.diagnostic_trail)
        .pop()
        .unwrap();
    assert!(payload_number(&breakdown, "item_calls") > 0, "{breakdown}");
    assert!(
        payload_number(&breakdown, "window_calls") > 0,
        "{breakdown}"
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

/// The file whose presence in a data root says "this directory is a throwaway
/// made for the reload measurement". The operator creates it in a fresh
/// `mktemp -d` root for this measurement only; nothing in the repo does.
const THROWAWAY_MARKER: &str = ".reprise-throwaway-data-root";

/// Why `data_root` may not host the reload measurement, or `None` when it may.
///
/// The measurement migrates and writes the database at
/// `$XDG_DATA_HOME/reprise/reprise.db`. Run by hand without isolation that is
/// the owner's real library. Ruling out the default location is not enough:
/// an owner whose library lives under a custom `XDG_DATA_HOME` (say `~/data`
/// exported from a shell profile) passes every check on the path alone, and
/// nothing in the environment says which root is the real one. So the guard
/// also demands positive proof: the root must contain [`THROWAWAY_MARKER`],
/// which the operator creates in a fresh throwaway root, and every root
/// without it is refused. The path checks stay as the first line: the root
/// must be one the caller set explicitly (a relative `XDG_DATA_HOME` counts as
/// unset, `dirs` ignores it) and must not lie under the default
/// `$HOME/.local/share`, marker or no marker.
fn unisolated_data_root_reason(
    xdg_data_home: Option<&std::path::Path>,
    home: Option<&std::path::Path>,
) -> Option<&'static str> {
    let Some(root) = xdg_data_home.filter(|root| !root.as_os_str().is_empty()) else {
        return Some("XDG_DATA_HOME is not set");
    };
    if !root.is_absolute() {
        return Some("XDG_DATA_HOME is relative, so the default data directory would be used");
    }
    let Some(home) = home else {
        return Some("HOME is not set, so the real data directory cannot be ruled out");
    };
    if root.starts_with(resolved(&home.join(".local/share"))) {
        return Some("XDG_DATA_HOME lies under $HOME/.local/share, the real data directory");
    }
    let marked = std::fs::symlink_metadata(root.join(THROWAWAY_MARKER))
        .is_ok_and(|marker| marker.file_type().is_file());
    if !marked {
        return Some(
            "XDG_DATA_HOME has no .reprise-throwaway-data-root marker, so it is not proven to be a throwaway directory",
        );
    }
    None
}

/// Resolves symlinks of an absolute path that exists, so a link into the real
/// data directory is judged by its target. A relative path stays as given:
/// resolving it against the working directory would hide that `dirs` ignores it.
fn resolved(path: &std::path::Path) -> std::path::PathBuf {
    if path.is_absolute() {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    }
}

/// A throwaway-looking data root that carries the marker, as the operator makes it.
fn marked_data_root() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("temp data directory");
    std::fs::write(root.path().join(THROWAWAY_MARKER), b"").expect("write the marker");
    root
}

/// Asserts that `reason` names the check that fired. The roots below do not
/// exist, so the marker check would refuse every one of them; matching on the
/// reason is what makes each case prove its own branch.
fn assert_refused_because(reason: Option<&str>, expected: &str) {
    let reason = reason.expect("the data root must be refused");
    assert!(
        reason.contains(expected),
        "{reason:?} does not say {expected:?}"
    );
}

#[test]
fn the_reload_measurement_refuses_the_real_or_an_unset_data_root() {
    use std::path::Path;

    let home = Some(Path::new("/home/owner"));
    assert_refused_because(
        unisolated_data_root_reason(None, home),
        "XDG_DATA_HOME is not set",
    );
    assert_refused_because(
        unisolated_data_root_reason(Some(Path::new("")), home),
        "XDG_DATA_HOME is not set",
    );
    assert_refused_because(
        unisolated_data_root_reason(Some(Path::new("share")), home),
        "XDG_DATA_HOME is relative",
    );
    assert_refused_because(
        unisolated_data_root_reason(Some(Path::new("/home/owner/.local/share")), home),
        "under $HOME/.local/share",
    );
    assert_refused_because(
        unisolated_data_root_reason(Some(Path::new("/home/owner/.local/share/x")), home),
        "under $HOME/.local/share",
    );
    let marked = marked_data_root();
    assert_refused_because(
        unisolated_data_root_reason(Some(marked.path()), None),
        "HOME is not set",
    );
}

#[cfg(unix)]
#[test]
fn the_reload_measurement_refuses_a_data_root_behind_a_symlinked_default() {
    let home = tempfile::tempdir().expect("temp home");
    let real = marked_data_root();
    std::fs::create_dir_all(home.path().join(".local")).expect("create .local");
    std::os::unix::fs::symlink(real.path(), home.path().join(".local/share"))
        .expect("link the default data directory");

    assert!(
        unisolated_data_root_reason(Some(&resolved(real.path())), Some(&resolved(home.path())))
            .is_some(),
        "a data root that the default `.local/share` link resolves to is the real directory, \
         marker or not"
    );
}

#[test]
fn the_reload_measurement_refuses_a_marked_root_under_the_default_data_directory() {
    let home = tempfile::tempdir().expect("temp home");
    let default_root = home.path().join(".local/share/reprise-data");
    std::fs::create_dir_all(&default_root).expect("create the default data directory");
    std::fs::write(default_root.join(THROWAWAY_MARKER), b"").expect("write the marker");

    assert!(
        unisolated_data_root_reason(Some(&default_root), Some(home.path())).is_some(),
        "the marker cannot launder the real data directory"
    );
}

#[test]
fn the_reload_measurement_refuses_a_custom_root_without_the_marker() {
    let home = tempfile::tempdir().expect("temp home");
    let custom = tempfile::tempdir().expect("an owner's custom XDG_DATA_HOME");
    std::fs::create_dir_all(custom.path().join("reprise")).expect("create the library directory");
    std::fs::write(custom.path().join("reprise/reprise.db"), b"").expect("a real-looking database");

    let reason = unisolated_data_root_reason(Some(custom.path()), Some(home.path()))
        .expect("an unmarked root is refused, wherever it lives");
    assert!(reason.contains(THROWAWAY_MARKER), "{reason}");
}

#[test]
fn the_reload_measurement_accepts_a_marked_isolated_data_root() {
    use std::path::Path;

    let home = tempfile::tempdir().expect("temp home");
    let marked = marked_data_root();
    assert_eq!(
        unisolated_data_root_reason(Some(marked.path()), Some(home.path())),
        None
    );
    assert_eq!(
        unisolated_data_root_reason(Some(marked.path()), Some(Path::new("/home/owner"))),
        None
    );

    let sibling = home.path().join(".local/sharing");
    std::fs::create_dir_all(&sibling).expect("create the sibling");
    std::fs::write(sibling.join(THROWAWAY_MARKER), b"").expect("write the marker");
    assert_eq!(
        unisolated_data_root_reason(Some(&sibling), Some(home.path())),
        None,
        "a sibling that merely shares the prefix is not the real directory"
    );
}

#[test]
fn the_reload_measurement_does_not_take_a_directory_for_the_marker() {
    let home = tempfile::tempdir().expect("temp home");
    let root = tempfile::tempdir().expect("temp data directory");
    std::fs::create_dir(root.path().join(THROWAWAY_MARKER)).expect("a directory by that name");

    assert!(unisolated_data_root_reason(Some(root.path()), Some(home.path())).is_some());
}

/// Prints reload latency samples against whatever library sits in
/// `XDG_DATA_HOME`. Run it only through the isolated recipe in `AGENTS.md`,
/// with a fresh `mktemp -d` data directory. The marker is this measurement's
/// own extra step, which the general recipe does not take:
///
/// ```text
/// data=$(mktemp -d); touch "$data/.reprise-throwaway-data-root"
/// dbus-run-session -- xvfb-run -a env XDG_DATA_HOME=$data XDG_CACHE_HOME=$(mktemp -d) \
///   GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink <test binary> \
///   --ignored --exact <this test> --nocapture
/// ```
///
/// Without the marker it refuses, see [`unisolated_data_root_reason`]. The test
/// creates no library of its own: `$data/reprise/reprise.db` must already be a
/// migrated database seeded from a generated fixture, never a copy of the real
/// one. On an empty root it stops at `SchemaNotReady`.
#[test]
#[ignore = "measurement: needs XDG_DATA_HOME set to a throwaway directory outside $HOME/.local/share that holds a .reprise-throwaway-data-root marker file; panics otherwise, and run it only through the isolated Xvfb recipe"]
fn measure_generated_library_reload_latency() {
    use gtk4::prelude::*;

    let xdg_data_home = std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from);
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    if let Some(reason) = unisolated_data_root_reason(
        xdg_data_home.as_deref().map(resolved).as_deref(),
        home.as_deref().map(resolved).as_deref(),
    ) {
        panic!("refusing to open the real library: {reason}");
    }

    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    // The validated `XDG_DATA_HOME` hosts the library. `default_path()` would
    // resolve the test build's private directory instead, which is empty.
    let db_path = xdg_data_home
        .expect("validated above")
        .join("reprise/reprise.db");
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
                reload_id: 1,
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
            reload_id: 1,
            cause: ReloadCause::Other,
            source: format!("{}\nsecond line", "ä".repeat(1_100)),
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
