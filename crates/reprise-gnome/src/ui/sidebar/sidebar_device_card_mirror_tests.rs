use super::tests::view;
use super::*;
use reprise_core::device_sync::device_view::DeviceContentsState;
use reprise_core::device_sync::{CategoryDiff, CategoryReading, DeviceStorageAccess};

fn select_playlist(device: &mut DeviceView) {
    device.page.blockers.clear();
    device.page.playlists = vec![reprise_core::device_sync::SyncPlaylistRow {
        source: reprise_core::device_sync::SelectionSource::Playlist(1),
        name: Some("Road".into()),
        smart: false,
        selected: true,
        available: true,
        entry_count: 1,
        unique_track_count: 1,
        unavailable_count: 0,
        target_bytes: 1,
        last_synced_at: None,
    }];
}

fn diff(
    files_to_copy: usize,
    bytes_to_copy: u64,
    files_to_remove: usize,
    bytes_freed: u64,
) -> CategoryDiff {
    CategoryDiff {
        files_to_copy,
        bytes_to_copy,
        files_to_remove,
        bytes_freed,
        playlists_rewritten: 0,
    }
}

#[test]
fn card_detail_mode_only_distinguishes_delta_and_progress() {
    let mut pending = view(PlannedSyncPhase::Idle);
    select_playlist(&mut pending);
    pending.page.changes.additions = 1;
    assert_eq!(detail_mode(&pending), DetailMode::Delta);

    pending.sync_phase = PlannedSyncPhase::Syncing {
        step: SyncStep::Copying,
        done: 0,
        total: 1,
        current_track: "Track".into(),
        bytes_done: 0,
        bytes_total: 1,
    };
    assert_eq!(detail_mode(&pending), DetailMode::Progress);
    assert_eq!(
        detail_mode(&view(PlannedSyncPhase::ComputingDelta)),
        DetailMode::Delta
    );
}

#[test]
fn mtp_15_sidebar_keeps_free_space_visible_during_sync() {
    let mut copying = view(PlannedSyncPhase::Syncing {
        step: SyncStep::Copying,
        done: 1,
        total: 2,
        current_track: "Track A".into(),
        bytes_done: 1,
        bytes_total: 2,
    });
    copying.name = "Phone A".into();
    copying.bytes_per_second = 2 * 1_024 * 1_024;

    copying.storage.free_bytes = Some(8 * 1_024 * 1_024);
    assert_eq!(card_title(&copying), "Syncing Phone A");
    assert_eq!(
        card_subtitle(&copying),
        "8.0 MiB free · ↑ Track A · 2.0 MiB/s"
    );
    copying.sync_phase = PlannedSyncPhase::ComputingDelta;
    assert_eq!(card_subtitle(&copying), "8.0 MiB free · Checking…");
    copying.sync_phase = PlannedSyncPhase::Finishing;
    assert_eq!(card_subtitle(&copying), "8.0 MiB free · Finishing…");
}

#[test]
fn mtp_29_idle_card_reads_the_aggregate_balance_not_a_blended_change_count() {
    let mut device = view(PlannedSyncPhase::Idle);
    select_playlist(&mut device);
    device.contents_state = DeviceContentsState::Verified;
    device.target_reading = CategoryReading::Diff(diff(1, 1_024 * 1_024, 1, 0));

    assert_eq!(card_title(&device), "Pixel 8");
    assert_eq!(card_subtitle(&device), "1 to copy · 1.0 MiB · 1 to remove");
}

#[test]
fn mtp_48_inert_device_card_names_the_active_device_and_offers_no_sync_copy() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.session_state = reprise_core::device_sync::DeviceSessionState::Inert {
        active_device_name: "Pixel 7a (Anna)".into(),
    };

    assert_eq!(
        card_subtitle(&device),
        "Plugged in · disconnect Pixel 7a (Anna) to use it"
    );
    assert!(!device.page.controls.can_start);
}

#[test]
fn mtp_50_remembered_card_is_dimmed_has_no_diff_and_exposes_local_memory_actions() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.connected = false;
    device.session_state = reprise_core::device_sync::DeviceSessionState::Remembered;
    device.last_sync = Some(chrono::Utc::now() - chrono::Duration::days(3));
    device.target_reading = CategoryReading::Diff(diff(14, 2_600_000_000, 3, 148 * 1_024 * 1_024));

    assert_eq!(card_subtitle(&device), "Not connected · synced 3 days ago");
    assert!(idle_tooltip(&device).is_none());
    assert!(css().contains(".device-card.remembered-device { opacity: 0.58; }"));
    let menu_source = include_str!("../device_sync/device_sync_card_menu.rs");
    assert!(menu_source.contains("BUTTON_SECONDARY"));
    assert!(menu_source.contains("FORGET_DEVICE"));
    assert!(menu_source.contains("device_sync_rename::prompt"));
    assert!(menu_source.contains("forget_remembered_device"));
}

#[test]
fn css_covers_the_sync_card_vocabulary() {
    let css = css();
    for marker in [
        ".device-card {",
        ".device-card:hover",
        ".device-card:focus-visible",
        ".device-card-icon",
        ".device-card-glyph",
        ".device-card-detail",
        ".device-card-percent",
        ".device-card-progress trough",
        ".device-card-progress progress",
    ] {
        assert!(css.contains(marker), "missing rule: {marker}");
    }
    assert!(
        !css.contains("#1CA98F"),
        "the accent must come from the shared style source, not a literal"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn css_parses_in_gtk_without_dropping_declarations() {
    if gtk4::init().is_err() {
        return;
    }
    let combined = format!(
        "{}\n{}",
        crate::ui::style::theme::theme_css(
            crate::ui::style::theme::Theme::DEFAULT,
            true,
            crate::ui::style::accent::AccentSource::App,
        ),
        css()
    );
    let errors = crate::ui::style::css_parse_errors(&combined);
    assert!(
        errors.is_empty(),
        "GTK reported CSS parsing errors: {errors:?}"
    );
}

#[test]
fn mtp_29_deletions_only_idle_card_reads_frees_not_zero_bytes() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.contents_state = DeviceContentsState::Verified;
    device.target_reading = CategoryReading::Diff(diff(0, 0, 3, 148 * 1_024 * 1_024));

    let subtitle = card_subtitle(&device);

    assert_eq!(subtitle, "3 to remove · frees 148.0 MiB");
    assert!(!subtitle.contains("0 B"));
}

#[test]
fn mtp_29_up_to_date_idle_card_names_when_it_last_synced() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.contents_state = DeviceContentsState::Verified;
    device.last_sync = Some(chrono::Utc::now() - chrono::Duration::minutes(12));

    assert_eq!(card_subtitle(&device), "Up to date · synced 12 min ago");
}

#[test]
fn mtp_29_never_verified_idle_card_prompts_a_scan_instead_of_the_balance() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.contents_state = DeviceContentsState::NeverVerified;
    device.target_reading = CategoryReading::Diff(diff(5, 1, 0, 0));

    assert_eq!(card_subtitle(&device), "Tap to scan device contents");
}

#[test]
fn warnings_keep_an_idle_card_reading_needs_attention() {
    let mut device = view(PlannedSyncPhase::Idle);
    select_playlist(&mut device);
    device
        .page
        .warnings
        .push(reprise_core::device_sync::SyncPageWarning::UnsafeManagedItem);

    assert_eq!(detail_mode(&device), DetailMode::Delta);
    assert_eq!(
        card_subtitle(&device),
        "Needs attention · Available space unknown"
    );
}

#[test]
fn mtp_29_a_lone_no_playlists_selected_blocker_does_not_read_as_needs_attention() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.storage.access = DeviceStorageAccess::Writable;
    device.storage.free_bytes = Some(2 * 1_024 * 1_024);
    device.contents_state = DeviceContentsState::Verified;
    device
        .page
        .blockers
        .push(reprise_core::device_sync::MirrorBlocker::NoPlaylistsSelected);

    assert_eq!(
        card_subtitle(&device),
        "Up to date",
        "an unselected mirror is not an error — it reads through the ordinary balance states"
    );
}

#[test]
fn mtp_13_sidebar_device_card_delegates_to_the_main_window_page() {
    let source = include_str!("sidebar.rs");
    assert!(source.contains("on_open: Rc<dyn Fn(String, String)>"));
    assert!(!source.contains("device_sync_dialog::present"));
}

#[test]
fn byte_progress_fraction_is_bounded_and_handles_an_unknown_total() {
    assert_eq!(sync_fraction(50, 100), 0.5);
    assert_eq!(sync_fraction(150, 100), 1.0);
    assert_eq!(sync_fraction(50, 0), 0.0);
}

#[test]
fn card_activity_distinguishes_transcoding_and_copying_with_artist() {
    let track = "Immortal — Lorna Shore";

    assert_eq!(
        device_sync_strings::sync_activity(step_glyph(&SyncStep::Transcoding), track),
        "⟳ transcoding · Immortal — Lorna Shore"
    );
    assert_eq!(
        device_sync_strings::sync_activity(step_glyph(&SyncStep::Copying), track),
        "↑ Immortal — Lorna Shore"
    );
}

#[test]
fn syncing_title_is_explicit() {
    assert_eq!(
        card_title(&view(PlannedSyncPhase::Finishing)),
        "Syncing Pixel 8"
    );
}

#[test]
fn mtp_13_sidebar_device_card_has_no_direct_sync_action() {
    let direct_sync_action = ["app", "sync-device"].join(".");

    assert!(!include_str!("sidebar_device_card.rs").contains(&direct_sync_action));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn device_card_open_is_a_native_keyboard_action() {
    gtk4::init().unwrap();
    let opened = Rc::new(RefCell::new(None));
    let opened_for_callback = opened.clone();
    let on_open: OpenCallback = Rc::new(move |id, name| {
        opened_for_callback.borrow_mut().replace((id, name));
    });
    let card = DeviceCard::new(&view(PlannedSyncPhase::Idle), &on_open);
    assert!(card.root.is_focusable());
    card.root.emit_clicked();
    assert_eq!(
        opened.borrow().as_ref(),
        Some(&("pixel".to_owned(), "Pixel 8".to_owned()))
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mot_7_disabled_animations_apply_progress_and_state_changes_immediately() {
    if gtk4::init().is_err() {
        return;
    }
    let settings = gtk4::Settings::default().unwrap();
    let previous = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(false);
    let device = view(PlannedSyncPhase::Syncing {
        step: SyncStep::Copying,
        done: 0,
        total: 1,
        current_track: "Track".into(),
        bytes_done: 50,
        bytes_total: 100,
    });
    let on_open: OpenCallback = Rc::new(|_, _| {});
    let card = DeviceCard::new(&device, &on_open);

    card.update(&device);

    assert_eq!(card.progress.fraction(), 0.5);
    assert_eq!(
        card.detail_stack.transition_duration(),
        crate::ui::motion::STANDARD_MS
    );
    assert_eq!(
        card.detail_stack.visible_child_name().as_deref(),
        Some("progress")
    );
    assert_eq!(
        card.indicator.visible_child_name().as_deref(),
        Some("syncing")
    );
    assert!(card.spinner.is_spinning());
    assert!(card.progress_revealer.reveals_child());
    settings.set_gtk_enable_animations(previous);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn enabled_animations_interpolate_progress_to_the_latest_fraction() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    if gtk4::init().is_err() {
        return;
    }
    let settings = gtk4::Settings::default().unwrap();
    let previous = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(true);
    let idle = view(PlannedSyncPhase::Idle);
    let on_open: OpenCallback = Rc::new(|_, _| {});
    let card = DeviceCard::new(&idle, &on_open);
    assert!(card.root.settings().is_gtk_enable_animations());
    let window = gtk4::Window::new();
    window.set_child(Some(&card.root));
    window.present();
    gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
        std::time::Duration::from_millis(20),
    ));
    let syncing = view(PlannedSyncPhase::Syncing {
        step: SyncStep::Copying,
        done: 0,
        total: 1,
        current_track: "Track".into(),
        bytes_done: 50,
        bytes_total: 100,
    });

    card.update(&syncing);

    assert!(card.progress.fraction() < 0.5);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while (card.progress.fraction() - 0.5).abs() >= 1e-6 && std::time::Instant::now() < deadline {
        gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
            std::time::Duration::from_millis(20),
        ));
    }
    assert!((card.progress.fraction() - 0.5).abs() < 1e-6);
    window.close();
    settings.set_gtk_enable_animations(previous);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mot_2_device_background_surfaces_only_crossfade_in_place() {
    gtk4::init().unwrap();
    let device = view(PlannedSyncPhase::Idle);
    let on_open: OpenCallback = Rc::new(|_, _| {});
    let card = DeviceCard::new(&device, &on_open);

    assert_eq!(
        card.indicator.transition_type(),
        gtk4::StackTransitionType::Crossfade
    );
    assert_eq!(
        card.detail_stack.transition_type(),
        gtk4::StackTransitionType::Crossfade
    );
    assert_eq!(
        card.suffix_stack.transition_type(),
        gtk4::StackTransitionType::Crossfade
    );
    assert_eq!(
        card.progress_revealer.transition_type(),
        gtk4::RevealerTransitionType::Crossfade
    );
}
