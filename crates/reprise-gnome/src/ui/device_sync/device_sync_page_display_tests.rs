//! Display-dependent device-page tests — split out of
//! `device_sync_page_tests.rs` to keep that file under the project's
//! 800-line limit. Every test here needs a real GTK display (`gtk4::init`,
//! window presentation, measured widget bounds) and is `#[ignore]`d the same
//! way its siblings in the parent file always were; the separate xvfb-run
//! display gate is what actually runs these.

use super::*;

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
    let overview = text.find("Sync overview").expect("sync overview");
    assert!(identity < connection);
    assert!(connection < last_sync);
    assert!(last_sync < storage);
    assert!(storage < playlists);
    assert!(identity < overview);
    assert!(
        !text.contains("Last synchronization"),
        "the header owns device sync history; the overview must not duplicate it"
    );
    assert_eq!(
        count_descendants::<adw::PreferencesPage>(root.upcast_ref()),
        0
    );
    assert_eq!(surface.playlist_rows.borrow().len(), 1);
    assert_eq!(
        surface.playlist_rows.borrow()[0].button.accessible_role(),
        gtk4::AccessibleRole::ToggleButton
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_15_sync_status_text_does_not_resize_the_playlist_workspace() {
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
    window.set_default_size(968, 800);
    window.set_child(Some(&root));
    window.present();
    gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
        std::time::Duration::from_millis(50),
    ));
    let converting_width = surface.playlist_list.width();
    let converting_status_width = surface
        .progress_detail
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
    let copying_width = surface.playlist_list.width();
    let copying_status_width = surface
        .progress_detail
        .measure(gtk4::Orientation::Horizontal, -1)
        .1;

    assert_eq!(
        copying_width, converting_width,
        "dynamic status copy must wrap inside a stable overview column"
    );
    assert_eq!(
        copying_status_width, converting_status_width,
        "dynamic status copy must not change the overview's natural width"
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_15_playlist_and_sync_overview_cards_share_the_same_edges() {
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
    let window = gtk4::Window::new();
    window.set_default_size(968, 800);
    window.set_child(Some(&root));
    window.present();
    gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
        std::time::Duration::from_millis(50),
    ));

    fn collect_cards(widget: &gtk4::Widget, cards: &mut Vec<gtk4::Widget>) {
        if widget.has_css_class("card") {
            cards.push(widget.clone());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            collect_cards(&current, cards);
            child = current.next_sibling();
        }
    }

    let mut cards = Vec::new();
    collect_cards(root.upcast_ref(), &mut cards);
    assert_eq!(
        cards.len(),
        4,
        "hero, playlist workspace, sync overview and the content panel must be cards"
    );
    let body = cards[1].parent().expect("shared dashboard body");
    assert_eq!(cards[2].parent().as_ref(), Some(&body));
    let playlist = cards[1].compute_bounds(&body).expect("playlist bounds");
    let overview = cards[2].compute_bounds(&body).expect("overview bounds");
    assert_eq!(playlist.y(), overview.y(), "top edges must align");
    assert_eq!(
        playlist.height(),
        overview.height(),
        "bottom edges must align"
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_13_full_page_renders_and_wires_only_the_playlist_mirroring_controls() {
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
        surface.profile.model().map(|model| model.n_items()),
        Some(3)
    );
    assert_eq!(surface.profile.selected(), 1);
    assert_eq!(surface.playlist_rows.borrow().len(), 1);
    assert_eq!(
        surface.playlist_rows.borrow()[0].title.label(),
        "Lorna Shore & Similar"
    );
    assert!(!surface.playlist_rows.borrow()[0].title.uses_markup());
    assert_eq!(surface.primary.label().as_deref(), Some("_Sync now"));
    assert!(surface.primary.uses_underline());
    assert!(surface.primary.is_sensitive());
    assert!(!surface.root_text().contains("Entire library"));
    assert!(!surface.root_text().contains("Device files"));
    assert!(!surface.root_text().contains("Songs"));
    assert!(!surface.root_text().contains("ratings"));
    assert!(!surface.root_text().contains("Remove unselected"));

    surface.profile.set_selected(2);
    surface.playlist_rows.borrow()[0].button.set_active(false);
    surface.primary.emit_clicked();
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
    assert_eq!(surface.progress_detail.label(), "Immortal — Lorna Shore");
    assert_eq!(surface.progress_speed.label(), "2.0 MiB/s");
    assert_eq!(surface.progress_bar.fraction(), 0.5);

    device.page.controls = SyncPageControls {
        editable: false,
        can_start: false,
        can_cancel: true,
        can_eject: false,
    };
    surface.update(&device);
    surface.primary.emit_clicked();
    assert_eq!(surface.primary.label().as_deref(), Some("_Cancel"));
    assert!(surface.primary.has_css_class("destructive-action"));
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

    let up_next = surface
        .content_panel
        .root()
        .parent()
        .expect("the content panel must belong to the Up next section");
    let content = up_next
        .parent()
        .expect("Up next must belong to the content column");
    let children = std::iter::successors(
        content.first_child(),
        gtk4::prelude::WidgetExt::next_sibling,
    )
    .collect::<Vec<_>>();
    assert_eq!(
        children.len(),
        3,
        "the content column must contain only hero, body and Up next"
    );
    assert_eq!(
        content.last_child(),
        Some(up_next.clone()),
        "Up next must remain the page's final section"
    );
    assert_eq!(
        surface.content_panel.root().parent().as_ref(),
        Some(&up_next),
        "the content panel belongs inside the explicit Up next section"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn up_next_card_uses_one_row_per_source_without_nested_section_headings() {
    gtk4::init().expect("GTK test display");
    let mut device = device();
    device.content_rows[0].item_count = 291;
    device.content_rows[0].size_on_device_bytes = 1024 * 1024 * 1024;

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
    assert!(lines.contains(&"1 of 1 playlists · smart lists kept up to date"));
    assert!(lines.contains(&"291 tracks"));
    assert!(lines.contains(&"1.0 GiB"));
    assert!(lines.contains(&"Nothing to transfer"));
    assert!(!lines.contains(&"Storage by category"));
    assert!(!lines.contains(&"Content"));
    assert!(!lines.contains(&"Next synchronization"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn design_2c_up_next_legend_uses_the_rows_projected_sizes() {
    gtk4::init().expect("GTK test display");
    let mut device = device();
    device.content_rows[0].size_on_device_bytes = 1024 * 1024 * 1024;
    device.category_readings[0] =
        reprise_core::device_sync::CategoryReading::Diff(reprise_core::device_sync::CategoryDiff {
            bytes_to_copy: 128 * 1024 * 1024,
            ..Default::default()
        });
    device.content_rows[1].size_on_device_bytes = 693 * 1024 * 1024;
    device.content_rows[2].size_on_device_bytes = 217 * 1024 * 1024;

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
    assert!(lines.contains(&"Music 1.1 GiB"));
    assert!(lines.contains(&"YouTube 693.0 MiB"));
    assert!(lines.contains(&"Podcasts 217.0 MiB"));
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

    assert!(!surface.device_name.is_sensitive());
    let tooltip = surface.device_name.tooltip_text().unwrap_or_default();
    assert!(tooltip.contains("no durable identity"));
    assert!(tooltip.contains("per-device settings cannot be kept between connections"));
    assert!(!tooltip.contains("history"));
}

/// `MTP-46`, the visible half: the core gate already keeps a switched-off
/// source out of the plan, but a Content row still reading "0 of 3 channels"
/// would tell the user their phone is set up to receive something it will
/// never receive. Both halves run against the same page, so the only
/// difference is the switch.
///
/// Deliberately not written against `root_text()` like its siblings: that
/// helper walks every child regardless of visibility, so it would happily
/// report a hidden row's label and the test would pass with the feature
/// removed.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_46_a_switched_off_source_has_no_content_row_on_the_device_page() {
    gtk4::init().expect("GTK test display");

    fn visible_text(widget: &gtk4::Widget, output: &mut String) {
        if !widget.is_visible() {
            return;
        }
        if let Ok(label) = widget.clone().downcast::<gtk4::Label>() {
            output.push_str(&label.text());
            output.push('\n');
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            visible_text(&current, output);
            child = current.next_sibling();
        }
    }

    fn page_text(device: &DeviceView) -> String {
        let (_surface, root) = DeviceSyncPage::new(
            device,
            PageActions {
                set_profile: Rc::new(|_| {}),
                set_playlist: Rc::new(|_, _| {}),
                start: Rc::new(|| {}),
                cancel: Rc::new(|| {}),
                eject: Rc::new(|| {}),
            },
            &no_op_content_actions(),
        );
        let mut text = String::new();
        visible_text(root.upcast_ref(), &mut text);
        text
    }

    let mut both_on = device();
    both_on.enabled_sources = reprise_core::device_sync::podcasts::EnabledSyncSources {
        rss: true,
        youtube: true,
    };
    let on = page_text(&both_on);
    assert!(
        on.contains("YouTube audio"),
        "with YouTube on its Content row is visible"
    );
    assert!(
        on.contains("Podcast episodes"),
        "with Podcasts on its Content row is visible"
    );
    assert!(
        on.contains("YouTube 0 B") && on.contains("Podcasts 0 B"),
        "visible sources also have entries in the storage legend"
    );

    let mut youtube_off = both_on.clone();
    youtube_off.enabled_sources.youtube = false;
    let off = page_text(&youtube_off);
    assert!(
        !off.contains("YouTube audio"),
        "switching YouTube off must take its Content row off the page, not leave a zero row"
    );
    assert!(
        !off.contains("YouTube 0 B"),
        "switching YouTube off must hide the same legend entry"
    );
    assert!(
        off.contains("Podcast episodes"),
        "Podcasts is a peer module and must be untouched by YouTube's switch"
    );
    assert!(
        off.contains("Playlists"),
        "local playlists have no module switch and always stay"
    );

    let mut podcasts_off = both_on.clone();
    podcasts_off.enabled_sources.rss = false;
    let off = page_text(&podcasts_off);
    assert!(
        !off.contains("Podcast episodes"),
        "switching Podcasts off must take its Content row off the page"
    );
    assert!(
        !off.contains("Podcasts 0 B"),
        "switching Podcasts off must hide the same legend entry"
    );
    assert!(
        off.contains("YouTube audio"),
        "and must leave YouTube, its peer, alone"
    );
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
