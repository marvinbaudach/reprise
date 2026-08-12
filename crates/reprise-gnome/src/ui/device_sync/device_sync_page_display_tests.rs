//! Display-dependent device-page tests — split out of
//! `device_sync_page_tests.rs` to keep that file under the project's
//! 800-line limit. Every test here needs a real GTK display (`gtk4::init`,
//! window presentation, measured widget bounds) and is `#[ignore]`d the same
//! way its siblings in the parent file always were; the separate xvfb-run
//! display gate is what actually runs these.

use super::*;

/// The width every presented probe window in this file is built at. Named
/// rather than repeated so a pinned probe and its siblings cannot drift apart.
const PROBE_WINDOW_WIDTH: i32 = 968;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_60_the_sync_bar_is_not_inside_the_scrollview() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");
    let (surface, _root) = DeviceSyncPage::new(
        &device(),
        PageActions {
            set_profile: Rc::new(|_| {}),
            set_playlist: Rc::new(|_, _| {}),
            start: Rc::new(|| {}),
            cancel: Rc::new(|| {}),
            eject: Rc::new(|| {}),
        },
        &no_op_content_actions(),
    );

    let mut ancestor = surface
        .dashboard
        .dock
        .primary
        .clone()
        .upcast::<gtk4::Widget>()
        .parent();
    while let Some(widget) = ancestor {
        assert!(
            !widget.is::<gtk4::ScrolledWindow>(),
            "the docked sync bar must be a sibling of the scroller, not its descendant"
        );
        ancestor = widget.parent();
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_60_a_failed_run_is_read_only_in_the_dock() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");
    let mut failed = device();
    failed.sync_error = Some(SyncFailure {
        message: "The phone stopped responding.".into(),
        failed_tracks: Vec::new(),
    });
    let (surface, _root) = DeviceSyncPage::new(
        &failed,
        PageActions {
            set_profile: Rc::new(|_| {}),
            set_playlist: Rc::new(|_, _| {}),
            start: Rc::new(|| {}),
            cancel: Rc::new(|| {}),
            eject: Rc::new(|| {}),
        },
        &no_op_content_actions(),
    );

    assert_eq!(
        surface
            .root_text()
            .matches("The phone stopped responding.")
            .count(),
        1,
        "a failed run must not be duplicated in a page notice"
    );
    assert!(surface.dashboard.dock.title.has_css_class("error"));
    assert_eq!(
        surface.dashboard.dock.title.tooltip_text().as_deref(),
        Some("The phone stopped responding.")
    );
    failed.sync_error = None;
    failed.page.warnings = vec![SyncPageWarning::UnavailableNotOnDevice { track_id: 7 }];
    surface.update(&failed);
    assert!(surface.dashboard.dock.title.has_css_class("warning"));
    assert!(!surface.dashboard.dock.title.has_css_class("error"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_14_full_page_uses_a_device_dashboard_instead_of_preferences_chrome() {
    gtk4::init().expect("GTK test display");
    let (surface, root) = DeviceSyncPage::new(
        &device(),
        PageActions {
            set_profile: Rc::new(|_| {}),
            set_playlist: Rc::new(|_, _| {}),
            start: Rc::new(|| {}),
            cancel: Rc::new(|| {}),
            eject: Rc::new(|| {}),
        },
        &no_op_content_actions(),
    );

    fn count_descendants<T: IsA<gtk4::Widget> + StaticType>(widget: &gtk4::Widget) -> usize {
        let mut count = usize::from(widget.is::<T>());
        let mut child = widget.first_child();
        while let Some(current) = child {
            count += count_descendants::<T>(&current);
            child = current.next_sibling();
        }
        count
    }

    let text = surface.root_text();
    let identity = text.find("Pixel 8").expect("device identity");
    let connection = text.find("MTP connected").expect("connection status");
    let last_sync = text
        .find("Never synchronized")
        .expect("device sync history");
    let storage = text.find("Internal storage").expect("device storage");
    let playlists = text.find("Playlists").expect("playlist workspace");
    let dock = text.find("Ready to sync").expect("docked sync status");
    assert!(identity < connection);
    assert!(connection < last_sync);
    assert!(last_sync < storage);
    assert!(storage < playlists);
    assert!(identity < dock);
    assert!(
        !text.contains("Last synchronization"),
        "the header owns device sync history; the overview must not duplicate it"
    );
    assert_eq!(
        count_descendants::<adw::PreferencesPage>(root.upcast_ref()),
        0
    );
    assert_eq!(surface.playlist_card.rows.borrow().len(), 1);
    assert_eq!(
        surface.playlist_card.rows.borrow()[0]
            .button
            .accessible_role(),
        gtk4::AccessibleRole::ToggleButton
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_60_sync_status_text_does_not_resize_the_playlist_workspace() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");
    let mut device = device();
    device.sync_phase = PlannedSyncPhase::Syncing {
        step: crate::ui::device_sync_runtime::SyncStep::Transcoding,
        done: 8,
        total: 278,
        current_track: "Claw Marks — Brand of Sacrifice".into(),
        bytes_done: 8,
        bytes_total: 278,
    };
    let (surface, root) = DeviceSyncPage::new(
        &device,
        PageActions {
            set_profile: Rc::new(|_| {}),
            set_playlist: Rc::new(|_, _| {}),
            start: Rc::new(|| {}),
            cancel: Rc::new(|| {}),
            eject: Rc::new(|| {}),
        },
        &no_op_content_actions(),
    );
    let window = gtk4::Window::new();
    window.set_default_size(PROBE_WINDOW_WIDTH, 800);
    // Pin the toplevel so this MTP-60 probe measures dock behaviour, not
    // post-present window settlement. With fresh XDG roots the window/root
    // shrank from 881 to 873 px while the overview card stayed at 414 px, so
    // the playlist allocation alone fell from 343 to 335 px between phases.
    // The width request is the line that holds: with `set_resizable(false)`
    // alone the window still settled at 881 px and the flake came straight
    // back, because the bare Xvfb the display gate runs under has no window
    // manager to honour `set_default_size` for a non-resizable toplevel.
    window.set_width_request(PROBE_WINDOW_WIDTH);
    window.set_resizable(false);
    window.set_child(Some(&root));
    window.present();
    gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
        std::time::Duration::from_millis(50),
    ));
    let converting_width = surface.playlist_card.list.width();
    let converting_status_width = surface
        .dashboard
        .dock
        .detail
        .measure(gtk4::Orientation::Horizontal, -1)
        .1;

    device.sync_phase = PlannedSyncPhase::Syncing {
        step: crate::ui::device_sync_runtime::SyncStep::Copying,
        done: 16,
        total: 278,
        current_track: "Lifeblood (feat. Will Ramos) — Brand of Sacrifice".into(),
        bytes_done: 16,
        bytes_total: 278,
    };
    device.bytes_per_second = 29_200_000;
    surface.update(&device);
    gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
        std::time::Duration::from_millis(50),
    ));
    let copying_width = surface.playlist_card.list.width();
    let copying_status_width = surface
        .dashboard
        .dock
        .detail
        .measure(gtk4::Orientation::Horizontal, -1)
        .1;

    assert_eq!(
        copying_width, converting_width,
        "dynamic status copy must not resize the playlist workspace"
    );
    assert_eq!(
        copying_status_width, converting_status_width,
        "dynamic status copy must not change the dock's natural width"
    );

    device.sync_error = Some(SyncFailure {
        message: "The first exceptionally detailed synchronization failure keeps explaining itself until the device status is fully understood by the user.".repeat(3),
        failed_tracks: Vec::new(),
    });
    surface.update(&device);
    let first_title_width = surface
        .dashboard
        .dock
        .title
        .measure(gtk4::Orientation::Horizontal, -1)
        .1;
    device.sync_error = Some(SyncFailure {
        message: "A different and substantially longer failure still belongs inside exactly the same fixed synchronization dock without imposing its natural text width on the page.".repeat(6),
        failed_tracks: Vec::new(),
    });
    surface.update(&device);
    let second_title_width = surface
        .dashboard
        .dock
        .title
        .measure(gtk4::Orientation::Horizontal, -1)
        .1;
    assert_eq!(
        second_title_width, first_title_width,
        "ellipsized dock titles must keep one bounded natural width"
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_60_playlist_and_sync_overview_cards_share_the_same_edges() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");
    let (_surface, root) = DeviceSyncPage::new(
        &device(),
        PageActions {
            set_profile: Rc::new(|_| {}),
            set_playlist: Rc::new(|_, _| {}),
            start: Rc::new(|| {}),
            cancel: Rc::new(|| {}),
            eject: Rc::new(|| {}),
        },
        &no_op_content_actions(),
    );
    fn label_with_text(widget: &gtk4::Widget, text: &str) -> Option<gtk4::Widget> {
        if widget
            .clone()
            .downcast::<gtk4::Label>()
            .is_ok_and(|label| label.text() == text)
        {
            return Some(widget.clone());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            if let Some(found) = label_with_text(&current, text) {
                return Some(found);
            }
            child = current.next_sibling();
        }
        None
    }

    fn card_ancestor(widget: &gtk4::Widget) -> gtk4::Widget {
        std::iter::successors(widget.parent(), gtk4::prelude::WidgetExt::parent)
            .find(|ancestor| ancestor.has_css_class("card"))
            .expect("label must belong to a card")
    }

    let profile = label_with_text(root.upcast_ref(), "Music transfer profile")
        .expect("transfer profile heading");
    let changes =
        label_with_text(root.upcast_ref(), "Playlist changes").expect("playlist changes heading");
    let profile_card = card_ancestor(&profile);
    let changes_card = card_ancestor(&changes);
    assert_ne!(
        profile_card, changes_card,
        "the two readings must be separate equally sized cards"
    );
    let pair = profile_card.parent().expect("responsive card pair");
    assert!(pair.is::<adw::WrapBox>());
    assert_eq!(changes_card.parent().as_ref(), Some(&pair));
    let window = gtk4::Window::new();
    window.set_default_size(PROBE_WINDOW_WIDTH, 800);
    window.set_child(Some(&root));
    window.present();
    gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
        std::time::Duration::from_millis(50),
    ));
    let profile_bounds = profile_card.compute_bounds(&pair).expect("profile bounds");
    let changes_bounds = changes_card.compute_bounds(&pair).expect("changes bounds");
    if profile_bounds.y() == changes_bounds.y() {
        assert_eq!(
            profile_bounds.height(),
            changes_bounds.height(),
            "side-by-side overview cards must share top and bottom edges"
        );
    } else {
        assert_eq!(
            profile_bounds.x(),
            changes_bounds.x(),
            "stacked overview cards must share their left edge"
        );
        assert_eq!(
            profile_bounds.width(),
            changes_bounds.width(),
            "stacked overview cards must share their right edge"
        );
    }
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_64_full_page_renders_and_wires_only_the_playlist_mirroring_controls() {
    gtk4::init().expect("GTK test display");
    let profile_events = Rc::new(RefCell::new(Vec::new()));
    let playlist_events = Rc::new(RefCell::new(Vec::new()));
    let starts = Rc::new(RefCell::new(0));
    let cancels = Rc::new(RefCell::new(0));
    let actions = PageActions {
        set_profile: {
            let events = profile_events.clone();
            Rc::new(move |profile| events.borrow_mut().push(profile))
        },
        set_playlist: {
            let events = playlist_events.clone();
            Rc::new(move |source, selected| events.borrow_mut().push((source, selected)))
        },
        start: {
            let starts = starts.clone();
            Rc::new(move || *starts.borrow_mut() += 1)
        },
        cancel: {
            let cancels = cancels.clone();
            Rc::new(move || *cancels.borrow_mut() += 1)
        },
        eject: Rc::new(|| {}),
    };
    let mut device = device();
    device.page.playlists[0].name = Some("Lorna Shore & Similar".into());
    let (surface, root) = DeviceSyncPage::new(&device, actions, &no_op_content_actions());

    assert_eq!(root.visible_child_name().as_deref(), Some("connected"));
    assert_eq!(
        surface
            .dashboard
            .profile
            .model()
            .map(|model| model.n_items()),
        Some(3)
    );
    assert_eq!(surface.dashboard.profile.selected(), 1);
    assert_eq!(surface.playlist_card.rows.borrow().len(), 1);
    assert_eq!(
        surface.playlist_card.rows.borrow()[0].title.label(),
        "Lorna Shore & Similar"
    );
    assert!(!surface.playlist_card.rows.borrow()[0].title.uses_markup());
    assert_eq!(
        surface.dashboard.dock.primary.label().as_deref(),
        Some("_Sync now")
    );
    assert!(surface.dashboard.dock.primary.uses_underline());
    assert!(surface.dashboard.dock.primary.is_sensitive());
    let root_text = surface.root_text();
    assert!(root_text.contains("Playlists"));
    for removed in [
        "YouTube audio",
        "Podcast episodes",
        "Size limit in GiB",
        "Entire library",
        "Device files",
        "Songs",
        "ratings",
        "Remove unselected",
    ] {
        assert!(!root_text.contains(removed), "removed control: {removed}");
    }

    surface.dashboard.profile.set_selected(2);
    surface.playlist_card.rows.borrow()[0]
        .button
        .set_active(false);
    surface.dashboard.dock.primary.emit_clicked();
    assert_eq!(*profile_events.borrow(), [TransferProfile::Original]);
    assert_eq!(
        *playlist_events.borrow(),
        [(SelectionSource::Smart(7), false)]
    );
    assert_eq!(*starts.borrow(), 1);

    device.sync_phase = PlannedSyncPhase::Syncing {
        step: crate::ui::device_sync_runtime::SyncStep::Copying,
        done: 1,
        total: 2,
        current_track: "Immortal — Lorna Shore".into(),
        bytes_done: 50,
        bytes_total: 100,
    };
    device.bytes_per_second = 2 * 1_024 * 1_024;
    surface.update(&device);
    assert_eq!(
        surface.dashboard.dock.detail.label(),
        "Immortal — Lorna Shore"
    );
    assert_eq!(
        surface.dashboard.dock.metrics.label(),
        "2.0 MiB/s · 1 s left"
    );
    assert_eq!(surface.dashboard.dock.progress.fraction(), 0.5);

    device.page.controls = SyncPageControls {
        editable: false,
        can_start: false,
        can_cancel: true,
        can_eject: false,
    };
    surface.update(&device);
    surface.dashboard.dock.primary.emit_clicked();
    assert_eq!(
        surface.dashboard.dock.primary.label().as_deref(),
        Some("_Cancel")
    );
    assert!(surface
        .dashboard
        .dock
        .primary
        .has_css_class("destructive-action"));
    assert_eq!(*cancels.borrow(), 1);

    let weak_surface = Rc::downgrade(&surface);
    let callback = page_state_callback(weak_surface.clone(), "phone".into());
    drop(surface);
    assert!(
        weak_surface.upgrade().is_some(),
        "the page widget must retain its live update controller"
    );
    callback(DeviceSyncState {
        devices: vec![device],
    });
    drop(root);
    assert!(
        weak_surface.upgrade().is_none(),
        "the runtime callback must not retain a removed device page"
    );
    callback(DeviceSyncState::default());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn device_page_sections_have_one_explicit_owner_and_order() {
    gtk4::init().expect("GTK test display");

    let (surface, _root) = DeviceSyncPage::new(
        &device(),
        PageActions {
            set_profile: Rc::new(|_| {}),
            set_playlist: Rc::new(|_, _| {}),
            start: Rc::new(|| {}),
            cancel: Rc::new(|| {}),
            eject: Rc::new(|| {}),
        },
        &no_op_content_actions(),
    );

    let on_device = surface
        .on_device
        .root()
        .parent()
        .expect("the balance must belong to the On this device section");
    let content = on_device
        .parent()
        .expect("On this device must belong to the content column");
    let children = std::iter::successors(
        content.first_child(),
        gtk4::prelude::WidgetExt::next_sibling,
    )
    .collect::<Vec<_>>();
    assert_eq!(
        children.len(),
        3,
        "the content column must contain only hero, body and On this device"
    );
    assert_eq!(
        content.last_child(),
        Some(on_device.clone()),
        "On this device must remain the page's final section"
    );
    assert_eq!(
        surface.on_device.root().parent().as_ref(),
        Some(&on_device),
        "the balance belongs inside the explicit On this device section"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_61_on_this_device_reads_as_one_balance_without_nested_section_headings() {
    gtk4::init().expect("GTK test display");
    let mut device = device();
    device.content_row.item_count = 291;
    device.content_row.size_on_device_bytes = 1024 * 1024 * 1024;

    let (surface, _root) = DeviceSyncPage::new(
        &device,
        PageActions {
            set_profile: Rc::new(|_| {}),
            set_playlist: Rc::new(|_, _| {}),
            start: Rc::new(|| {}),
            cancel: Rc::new(|| {}),
            eject: Rc::new(|| {}),
        },
        &no_op_content_actions(),
    );

    let text = surface.root_text();
    let lines = text.lines().collect::<Vec<_>>();
    assert!(lines.contains(&"1 playlist · 291 tracks · 1.0 GiB"));
    assert!(lines.contains(&"Folder /Music/Reprise · Smart lists stay current · no size limit"));
    assert!(!lines.contains(&"Storage by category"));
    assert!(!lines.contains(&"Content"));
    assert!(!lines.contains(&"Next synchronization"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_61_on_this_device_offers_no_playlist_selection() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");
    let (surface, _root) = DeviceSyncPage::new(
        &device(),
        PageActions {
            set_profile: Rc::new(|_| {}),
            set_playlist: Rc::new(|_, _| {}),
            start: Rc::new(|| {}),
            cancel: Rc::new(|| {}),
            eject: Rc::new(|| {}),
        },
        &no_op_content_actions(),
    );

    let text = surface.root_text();
    assert!(text.contains("On this device"));
    assert!(text.contains("Review playlists above"));
    assert!(!text.contains("Choose…"));
    assert!(!text.contains("Change…"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_61_the_rules_block_carries_both_device_switches() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");
    let settings = Rc::new(RefCell::new(device().settings));
    let content_actions = OnDeviceActions {
        set_remove_deleted: {
            let settings = settings.clone();
            Rc::new(move |value| settings.borrow_mut().remove_deleted = value)
        },
        set_sync_automatically: {
            let settings = settings.clone();
            Rc::new(move |value| settings.borrow_mut().sync_automatically = value)
        },
        scan_device: Rc::new(|| {}),
        open_folder_browser: Rc::new(|_| {}),
        open_playlist_picker: Rc::new(|_| {}),
        dismiss_legacy_media_notice: Rc::new(|| {}),
        legacy_media_notice_pending: Rc::new(|| false),
    };
    let (surface, _root) = DeviceSyncPage::new(
        &device(),
        PageActions {
            set_profile: Rc::new(|_| {}),
            set_playlist: Rc::new(|_, _| {}),
            start: Rc::new(|| {}),
            cancel: Rc::new(|| {}),
            eject: Rc::new(|| {}),
        },
        &content_actions,
    );

    let text = surface.root_text();
    assert!(text.contains("Rules for this phone"));
    assert!(text.contains("Remove from phone when removed from a playlist"));
    assert!(text.contains("Sync automatically when this phone connects"));
    fn switches(widget: &gtk4::Widget, found: &mut Vec<gtk4::Switch>) {
        if let Ok(switch) = widget.clone().downcast::<gtk4::Switch>() {
            found.push(switch);
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            switches(&current, found);
            child = current.next_sibling();
        }
    }
    let mut rules = Vec::new();
    switches(surface.on_device.root().upcast_ref(), &mut rules);
    assert_eq!(rules.len(), 2);
    rules[0].set_active(true);
    rules[1].set_active(false);
    assert!(settings.borrow().remove_deleted);
    assert!(!settings.borrow().sync_automatically);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_54_retired_media_notice_is_scoped_and_dismissible() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");
    let dismissed = Rc::new(Cell::new(false));
    let pending_reads = Rc::new(Cell::new(0));
    let content_actions = OnDeviceActions {
        set_remove_deleted: Rc::new(|_| {}),
        set_sync_automatically: Rc::new(|_| {}),
        scan_device: Rc::new(|| {}),
        open_folder_browser: Rc::new(|_| {}),
        open_playlist_picker: Rc::new(|_| {}),
        dismiss_legacy_media_notice: {
            let dismissed = dismissed.clone();
            Rc::new(move || dismissed.set(true))
        },
        legacy_media_notice_pending: {
            let reads = pending_reads.clone();
            Rc::new(move || {
                reads.set(reads.get() + 1);
                true
            })
        },
    };
    let (surface, _root) = DeviceSyncPage::new(
        &device(),
        PageActions {
            set_profile: Rc::new(|_| {}),
            set_playlist: Rc::new(|_, _| {}),
            start: Rc::new(|| {}),
            cancel: Rc::new(|| {}),
            eject: Rc::new(|| {}),
        },
        &content_actions,
    );

    assert!(surface.on_device.legacy_notice.is_revealed());
    surface.update(&device());
    assert_eq!(
        pending_reads.get(),
        1,
        "the durable notice state must be read once per page session, not on every update tick"
    );
    surface
        .on_device
        .legacy_notice
        .emit_by_name::<()>("button-clicked", &[]);
    assert!(dismissed.get());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn unrememberable_device_disables_hero_rename_with_the_identity_explanation() {
    gtk4::init().expect("GTK test display");
    let mut device = device();
    device.rememberable = false;
    let (surface, _root) = DeviceSyncPage::new(
        &device,
        PageActions {
            set_profile: Rc::new(|_| {}),
            set_playlist: Rc::new(|_, _| {}),
            start: Rc::new(|| {}),
            cancel: Rc::new(|| {}),
            eject: Rc::new(|| {}),
        },
        &no_op_content_actions(),
    );

    assert!(!surface.dashboard.device_name.is_sensitive());
    let tooltip = surface
        .dashboard
        .device_name
        .tooltip_text()
        .unwrap_or_default();
    assert!(tooltip.contains("no durable identity"));
    assert!(tooltip.contains("per-device settings cannot be kept between connections"));
    assert!(!tooltip.contains("history"));
}

/// `NPP-1`'s second pitfall is not confined to the panel itself: a label
/// without `ellipsize` reports its full text width as a *minimum*, and
/// `AdwOverlaySplitView` hands the content pane that minimum before it lays
/// out its own fixed 300 px column — which then leaves the window. The device
/// hero showed it plainly: with a long connection status and sync history the
/// card's width tracked the primary button's label, "Download & sync" versus
/// "Cancel", 85 px apart. Measured against the clamp the page declares for
/// itself, since a minimum above that clamp is exactly the broken state.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_1_a_talkative_device_hero_stays_inside_the_page_clamp() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");
    let mut device = device();
    device.name = "Marvin's Pixel 8 Pro".into();
    device.memory_status = Some("This device can be used now but cannot be remembered".into());
    device.last_sync = Some(chrono::Utc.with_ymd_and_hms(2026, 8, 7, 11, 49, 0).unwrap());
    device.verified_managed_track_count = Some(214);
    device.managed_track_count = 214;

    let (_surface, root) = DeviceSyncPage::new(
        &device,
        PageActions {
            set_profile: Rc::new(|_| {}),
            set_playlist: Rc::new(|_, _| {}),
            start: Rc::new(|| {}),
            cancel: Rc::new(|| {}),
            eject: Rc::new(|| {}),
        },
        &no_op_content_actions(),
    );

    let clamp = super::super::device_sync_page_layout::CONTENT_MAX_WIDTH;
    let minimum = root.measure(gtk4::Orientation::Horizontal, -1).0;
    assert!(
        minimum <= clamp,
        "the device page demands {minimum} px at minimum, past its own {clamp} px clamp — \
         at that width AdwOverlaySplitView pushes the now-playing column out of the window"
    );
}
