//! Android synchronization preferences and the live device browser.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::device_sync::{SyncPhase, SyncSnapshot};

use super::device_sync_runtime::{
    DeviceSyncRuntime, DeviceSyncState, DeviceView, EnqueueError, Subscription,
};
use super::device_sync_strings as copy;

pub(super) fn build_page(runtime: &Rc<DeviceSyncRuntime>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(copy::text(copy::SYNCHRONIZATION))
        .icon_name("phone-symbolic")
        .build();
    let group = adw::PreferencesGroup::builder()
        .title(copy::text(copy::CONNECTED_DEVICES))
        .build();
    // A real GtkListBox, not a styled Box: AdwActionRow's `activated` signal
    // only fires when a parent list box activates the row — inside a plain
    // Box the device rows render fine but are dead to clicks.
    let list = gtk4::ListBox::new();
    list.set_selection_mode(gtk4::SelectionMode::None);
    list.add_css_class("boxed-list");
    group.add(&list);
    page.add(&group);

    let runtime_for_update = runtime.clone();
    let update_list = list.clone();
    let subscription = runtime.subscribe(Rc::new(move |state| {
        render_devices(&update_list, &state, &runtime_for_update);
    }));
    retain_subscription(&page, subscription);
    page
}

fn render_devices(list: &gtk4::ListBox, state: &DeviceSyncState, runtime: &Rc<DeviceSyncRuntime>) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    if state.devices.is_empty() {
        let empty = adw::StatusPage::builder()
            .icon_name("phone-symbolic")
            .title(copy::text(copy::NO_DEVICE))
            .description(copy::text(copy::NO_DEVICE_DESCRIPTION))
            .build();
        empty.set_vexpand(true);
        list.append(&empty);
        return;
    }
    for device in &state.devices {
        list.append(&device_row(device, runtime));
    }
}

pub(super) fn device_row(device: &DeviceView, runtime: &Rc<DeviceSyncRuntime>) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(&device.name)
        .subtitle(copy::device_subtitle(
            device.connected,
            device.available_bytes,
        ))
        .activatable(true)
        .build();
    let icon = gtk4::Image::from_gicon(&device.icon);
    icon.set_pixel_size(32);
    row.add_prefix(&icon);
    if device.scanning {
        let spinner = gtk4::Spinner::new();
        spinner.set_spinning(true);
        spinner.set_tooltip_text(Some(&copy::text(copy::SCANNING_DEVICE)));
        row.add_suffix(&spinner);
    } else {
        row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
    }
    let device_id = device.id.clone();
    let runtime = runtime.clone();
    row.connect_activated(move |row| {
        // The row hands itself over so `present_device` can find the
        // Preferences NavigationView among its ancestors and push the device
        // page inside it.
        present_device(row, &device_id, &runtime);
    });
    row
}

/// Pushes the device's sync settings as a navigation subpage INSIDE the
/// Preferences dialog (same level as the other preference pages, back arrow
/// included) — deliberately not a second stacked modal.
pub(super) fn present_device(
    parent: &impl IsA<gtk4::Widget>,
    device_id: &str,
    runtime: &Rc<DeviceSyncRuntime>,
) {
    let Some(navigation) = parent
        .ancestor(adw::NavigationView::static_type())
        .and_downcast::<adw::NavigationView>()
    else {
        tracing::warn!("device row activated outside the preferences navigation");
        return;
    };
    let header = adw::HeaderBar::new();
    let refresh = gtk4::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text(copy::text(copy::REFRESH_DEVICE))
        .build();
    header.pack_end(&refresh);
    let detail = gtk4::Box::new(gtk4::Orientation::Vertical, 18);
    detail.set_margin_top(18);
    detail.set_margin_bottom(18);
    detail.set_margin_start(18);
    detail.set_margin_end(18);
    let scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&detail)
        .build();
    let clamp = adw::Clamp::builder()
        .maximum_size(680)
        .child(&scroll)
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&clamp));
    let page = adw::NavigationPage::new(&toolbar, &copy::text(copy::DISCONNECTED));

    let refresh_id = device_id.to_string();
    let refresh_runtime = runtime.clone();
    refresh.connect_clicked(move |_| refresh_runtime.refresh_contents(&refresh_id));

    let update_detail = detail.clone();
    let update_page = page.clone();
    let update_id = device_id.to_string();
    let update_runtime = runtime.clone();
    let subscription = runtime.subscribe(Rc::new(move |state| {
        let Some(device) = state.devices.iter().find(|device| device.id == update_id) else {
            update_page.set_title(&copy::text(copy::DISCONNECTED));
            return;
        };
        update_page.set_title(&device.name);
        let parent = update_detail.clone().upcast::<gtk4::Widget>();
        render_device_detail(&update_detail, device, &parent, &update_runtime);
    }));
    retain_subscription(&page, subscription);
    navigation.push(&page);
}

fn render_device_detail(
    detail: &gtk4::Box,
    device: &DeviceView,
    prompt_parent: &gtk4::Widget,
    runtime: &Rc<DeviceSyncRuntime>,
) {
    clear_box(detail);
    if device.scanning {
        let row = adw::ActionRow::builder()
            .title(copy::text(copy::SCANNING_DEVICE))
            .build();
        let spinner = gtk4::Spinner::new();
        spinner.set_spinning(true);
        row.add_suffix(&spinner);
        detail.append(&single_row_group(&row));
    } else if let Some(error) = &device.scan_error {
        let row = adw::ActionRow::builder()
            .title(copy::text(copy::SCAN_FAILED))
            .subtitle(error)
            .build();
        detail.append(&single_row_group(&row));
    }
    if progress_is_visible(&device.snapshot) {
        detail.append(&progress_group(&device.id, &device.snapshot, runtime));
    }
    detail.append(&planned::device_header_group(device, runtime));
    detail.append(&planned::selection_group(device, runtime));
    detail.append(&planned::delta_group(device));
    detail.append(&planned::settings_group(device, runtime));
    detail.append(&playlist_group(device, prompt_parent, runtime));
    detail.append(&music_group(device));
}

fn playlist_group(
    device: &DeviceView,
    prompt_parent: &gtk4::Widget,
    runtime: &Rc<DeviceSyncRuntime>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(copy::text(copy::PHONE_PLAYLISTS))
        .build();
    let add = gtk4::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text(copy::text(copy::NEW_PHONE_PLAYLIST))
        .valign(gtk4::Align::Center)
        .build();
    group.set_header_suffix(Some(&add));
    let id = device.id.clone();
    let runtime_for_add = runtime.clone();
    let prompt_parent = prompt_parent.clone();
    add.connect_clicked(move |_| {
        let runtime = runtime_for_add.clone();
        let id = id.clone();
        super::dialogs::prompt_name(
            &prompt_parent,
            &copy::text(copy::NEW_PHONE_PLAYLIST),
            &copy::text(copy::PLAYLIST_NAME),
            &copy::text(copy::CREATE),
            move |name| {
                runtime.create_playlist_draft(&id, &name);
            },
        );
    });

    let names = playlist_names(device);
    if names.is_empty() {
        group.set_description(Some(&copy::text(copy::NO_PHONE_PLAYLISTS)));
    }
    for (name, entries, draft) in names {
        let subtitle = if let Some(receipt) = device
            .last_enqueue
            .as_ref()
            .filter(|receipt| receipt.playlist == name)
        {
            copy::tracks_queued(receipt.track_count, receipt.queue_position)
        } else if draft {
            copy::text(copy::PLAYLIST_DRAFT)
        } else {
            copy::playlist_entries(entries)
        };
        let row = adw::ActionRow::builder()
            .title(&name)
            .subtitle(subtitle)
            .build();
        row.add_prefix(&gtk4::Image::from_icon_name("view-list-symbolic"));
        install_sync_drop(&row, &device.id, &name, runtime);
        group.add(&row);
    }
    group
}

fn music_group(device: &DeviceView) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(copy::text(copy::DEVICE_MUSIC))
        .build();
    if device.contents.files.is_empty() && !device.scanning {
        group.set_description(Some(&copy::text(copy::NO_DEVICE_MUSIC)));
    }
    for file in &device.contents.files {
        let row = adw::ActionRow::builder()
            .title(&file.name)
            .subtitle(&file.relative_path)
            .build();
        let size = gtk4::Label::new(Some(&copy::file_size(file.size_bytes)));
        size.add_css_class("dim-label");
        row.add_suffix(&size);
        group.add(&row);
    }
    group
}

fn progress_group(
    device_id: &str,
    snapshot: &SyncSnapshot,
    runtime: &Rc<DeviceSyncRuntime>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(copy::text(copy::SYNC_PROGRESS))
        .build();
    let status = adw::ActionRow::builder()
        .title(phase_text(snapshot))
        .subtitle(progress_summary(snapshot))
        .build();
    group.add(&status);

    let current = progress_row(
        copy::FILE_PROGRESS,
        snapshot.current_name.as_deref().unwrap_or("—"),
        file_fraction(snapshot),
    );
    group.add(&current);
    let overall_bytes = snapshot
        .completed_bytes
        .saturating_add(snapshot.current_bytes);
    let overall = progress_row(
        copy::TOTAL_PROGRESS,
        &format!(
            "{} / {}",
            copy::file_size(overall_bytes),
            copy::file_size(snapshot.total_bytes)
        ),
        overall_fraction(snapshot),
    );
    group.add(&overall);

    if matches!(
        snapshot.phase,
        SyncPhase::Preparing | SyncPhase::Copying | SyncPhase::Cancelling
    ) {
        let cancel = gtk4::Button::with_label(&copy::text(copy::CANCEL_CURRENT));
        cancel.set_valign(gtk4::Align::Center);
        cancel.set_sensitive(snapshot.phase != SyncPhase::Cancelling);
        let id = device_id.to_string();
        let runtime = runtime.clone();
        cancel.connect_clicked(move |_| runtime.cancel_current(&id));
        status.add_suffix(&cancel);
    }
    group
}

fn install_sync_drop(
    row: &adw::ActionRow,
    device_id: &str,
    playlist: &str,
    runtime: &Rc<DeviceSyncRuntime>,
) {
    let target = gtk4::DropTarget::new(glib::Type::STRING, gdk::DragAction::COPY);
    let enter_row = row.clone();
    target.connect_enter(move |_, _, _| {
        enter_row.add_css_class("accent");
        gdk::DragAction::COPY
    });
    let leave_row = row.clone();
    target.connect_leave(move |_| leave_row.remove_css_class("accent"));
    let drop_row = row.clone();
    let device_id = device_id.to_string();
    let playlist = playlist.to_string();
    let runtime = runtime.clone();
    target.connect_drop(move |_, value, _, _| {
        drop_row.remove_css_class("accent");
        let Ok(value) = value.get::<String>() else {
            return false;
        };
        let Some(ids) = sync_drop_ids(&value) else {
            return false;
        };
        match runtime.enqueue(&device_id, &playlist, &ids) {
            Ok(_) => true,
            Err(error) => {
                if let Some((heading, body)) = enqueue_warning(&error) {
                    present_enqueue_warning(&drop_row, &heading, &body);
                }
                tracing::warn!(%error, "phone playlist drop was rejected");
                false
            }
        }
    });
    row.add_controller(target);
}

fn enqueue_warning(error: &EnqueueError) -> Option<(String, String)> {
    let EnqueueError::InsufficientSpace {
        required_bytes,
        available_bytes,
    } = error
    else {
        return None;
    };
    Some((
        copy::text(copy::NOT_ENOUGH_SPACE),
        copy::insufficient_space_description(*required_bytes, *available_bytes),
    ))
}

fn present_enqueue_warning<W: IsA<gtk4::Widget>>(parent: &W, heading: &str, body: &str) {
    let dialog = enqueue_warning_dialog(heading, body);
    dialog.choose(Some(parent), gio::Cancellable::NONE, |_| {});
}

fn enqueue_warning_dialog(heading: &str, body: &str) -> adw::AlertDialog {
    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .close_response("close")
        .build();
    dialog.add_response("close", &super::strings::text(super::strings::CLOSE));
    dialog
}

fn sync_drop_ids(value: &str) -> Option<Vec<i64>> {
    let payload = super::track_list_dnd::parse_drag_payload(value)?;
    payload.ids.iter().all(|id| *id > 0).then_some(payload.ids)
}

fn progress_row(title: &str, subtitle: &str, fraction: f64) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(copy::text(title))
        .subtitle(subtitle)
        .build();
    let bar = gtk4::ProgressBar::new();
    bar.set_fraction(fraction);
    bar.set_hexpand(true);
    bar.set_valign(gtk4::Align::Center);
    bar.set_width_request(180);
    row.add_suffix(&bar);
    row
}

fn progress_summary(snapshot: &SyncSnapshot) -> String {
    let tracks = copy::track_progress(snapshot.completed_tracks, snapshot.total_tracks);
    let queue = copy::queued_jobs(snapshot.queued_jobs);
    let outcomes = copy::outcome_counts(snapshot.copied, snapshot.skipped, snapshot.failed);
    format!("{tracks} · {queue} · {outcomes}")
}

fn phase_text(snapshot: &SyncSnapshot) -> String {
    let message = match snapshot.phase {
        SyncPhase::Idle => copy::IDLE,
        SyncPhase::Preparing => copy::PREPARING,
        SyncPhase::Copying => snapshot.current_name.as_deref().unwrap_or(copy::PREPARING),
        SyncPhase::PausedDisconnected => copy::PAUSED_DISCONNECTED,
        SyncPhase::Cancelling => copy::CANCELLING,
        SyncPhase::Complete => copy::COMPLETE,
        SyncPhase::Failed => copy::FAILED,
    };
    copy::text(message)
}

fn progress_is_visible(snapshot: &SyncSnapshot) -> bool {
    snapshot.phase != SyncPhase::Idle
}

fn file_fraction(snapshot: &SyncSnapshot) -> f64 {
    snapshot
        .current_total
        .filter(|total| *total > 0)
        .map_or(0.0, |total| {
            (snapshot.current_bytes as f64 / total as f64).clamp(0.0, 1.0)
        })
}

fn overall_fraction(snapshot: &SyncSnapshot) -> f64 {
    if snapshot.total_bytes == 0 {
        return 0.0;
    }
    let copied = snapshot
        .completed_bytes
        .saturating_add(snapshot.current_bytes);
    (copied as f64 / snapshot.total_bytes as f64).clamp(0.0, 1.0)
}

fn playlist_names(device: &DeviceView) -> Vec<(String, usize, bool)> {
    let mut names = device
        .contents
        .playlists
        .iter()
        .map(|playlist| (playlist.name.clone(), playlist.entries.len(), false))
        .collect::<Vec<_>>();
    for draft in &device.draft_playlists {
        if !names.iter().any(|(name, _, _)| name == draft) {
            names.push((draft.clone(), 0, true));
        }
    }
    names.sort_by(|left, right| left.0.cmp(&right.0));
    names
}

fn single_row_group(row: &adw::ActionRow) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.add(row);
    group
}

fn clear_box(container: &gtk4::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn retain_subscription<W: IsA<gtk4::Widget>>(widget: &W, subscription: Subscription) {
    let subscription = Rc::new(RefCell::new(Some(subscription)));
    widget.connect_unrealize(move |_| {
        subscription.borrow_mut().take();
    });
}

#[path = "preference_sync_planned.rs"]
mod planned;

#[cfg(test)]
mod tests {
    use reprise_platform_linux::device_sync::DeviceContents;

    use super::*;

    fn view() -> DeviceView {
        DeviceView {
            id: "phone".into(),
            name: "Phone".into(),
            icon: gtk4::gio::ThemedIcon::new("phone-symbolic").upcast(),
            connected: true,
            available_bytes: Some(1_024),
            contents: DeviceContents::default(),
            scanning: false,
            scan_error: None,
            draft_playlists: vec!["Road".into()],
            last_enqueue: None,
            snapshot: reprise_core::device_sync::DeviceQueue::new().snapshot(),
            settings: reprise_core::device_sync::DeviceSettings {
                device_serial: "phone".into(),
                device_name: "Phone".into(),
                selection: reprise_core::device_sync::DeviceSelection::default(),
                opus_bitrate: 0,
                ratings_back: false,
                remove_deleted: true,
            },
            delta: None,
            sync_phase: crate::ui::device_sync_runtime::PlannedSyncPhase::Idle,
            sync_error: None,
            last_sync: None,
            tracks: Vec::new(),
            selected_track_count: 0,
        }
    }

    #[test]
    fn progress_fractions_are_finite_and_clamped() {
        let mut snapshot = view().snapshot;
        snapshot.current_total = Some(100);
        snapshot.current_bytes = 150;
        snapshot.total_bytes = 200;
        snapshot.completed_bytes = 100;
        assert_eq!(file_fraction(&snapshot), 1.0);
        assert_eq!(overall_fraction(&snapshot), 1.0);
        snapshot.current_total = None;
        snapshot.total_bytes = 0;
        assert_eq!(file_fraction(&snapshot), 0.0);
        assert_eq!(overall_fraction(&snapshot), 0.0);
    }

    #[test]
    fn draft_playlist_names_are_sorted_without_duplicates() {
        let mut device = view();
        device
            .contents
            .playlists
            .push(reprise_platform_linux::device_sync::DevicePlaylist {
                name: "Road".into(),
                entries: Vec::new(),
            });
        device.draft_playlists.push("Ambient".into());
        assert_eq!(
            playlist_names(&device),
            [("Ambient".into(), 0, true), ("Road".into(), 0, false)]
        );
    }

    #[test]
    fn idle_progress_is_hidden_but_terminal_progress_remains_visible() {
        let mut snapshot = view().snapshot;
        assert!(!progress_is_visible(&snapshot));
        snapshot.phase = SyncPhase::Complete;
        assert!(progress_is_visible(&snapshot));
    }

    #[test]
    fn sync_drop_accepts_only_the_established_positive_id_payload() {
        assert_eq!(sync_drop_ids("1,2|-"), Some(vec![1, 2]));
        assert!(sync_drop_ids("1,2").is_none());
        assert!(sync_drop_ids("|-").is_none());
        assert!(sync_drop_ids("foreign text").is_none());
        assert_eq!(sync_drop_ids("0|-"), None);
        assert_eq!(sync_drop_ids("-1|-"), None);
    }

    #[test]
    fn only_insufficient_space_maps_to_the_storage_warning() {
        let warning = enqueue_warning(&EnqueueError::InsufficientSpace {
            required_bytes: 2_048,
            available_bytes: 1_024,
        })
        .unwrap();
        assert_eq!(warning.0, copy::text(copy::NOT_ENOUGH_SPACE));
        assert_eq!(
            warning.1,
            copy::insufficient_space_description(2_048, 1_024)
        );
        assert!(enqueue_warning(&EnqueueError::UnknownDevice).is_none());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn storage_warning_dialog_exposes_sizes_and_a_close_action() {
        gtk4::init().unwrap();
        let heading = copy::text(copy::NOT_ENOUGH_SPACE);
        let body = copy::insufficient_space_description(2_048, 1_024);
        let dialog = enqueue_warning_dialog(&heading, &body);

        assert_eq!(dialog.heading().as_deref(), Some(heading.as_str()));
        assert_eq!(dialog.body(), body);
        assert_eq!(
            dialog.response_label("close"),
            crate::ui::strings::text(crate::ui::strings::CLOSE)
        );
    }

    fn display_runtime() -> Rc<DeviceSyncRuntime> {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        DeviceSyncRuntime::new(
            &Rc::new(RefCell::new(conn)),
            reprise_platform_linux::device_sync::DeviceMonitor::new(),
        )
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn empty_device_list_builds_the_usb_instructions_status_page() {
        gtk4::init().unwrap();
        let runtime = display_runtime();
        let list = gtk4::ListBox::new();
        render_devices(&list, &DeviceSyncState::default(), &runtime);
        // The list box wraps appended non-row children in a GtkListBoxRow.
        let status = list
            .first_child()
            .and_downcast::<gtk4::ListBoxRow>()
            .unwrap()
            .child()
            .and_downcast::<adw::StatusPage>()
            .unwrap();
        assert_eq!(status.title(), copy::text(copy::NO_DEVICE));
        assert_eq!(
            status.description().as_deref(),
            Some(copy::text(copy::NO_DEVICE_DESCRIPTION).as_str())
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn connected_device_row_keeps_name_storage_and_system_icon() {
        gtk4::init().unwrap();
        let runtime = display_runtime();
        let device = view();
        let row = device_row(&device, &runtime);
        assert_eq!(row.title(), "Phone");
        assert_eq!(
            row.subtitle().as_deref(),
            Some(copy::device_subtitle(true, Some(1_024)).as_str())
        );
        assert!(row.is_activatable());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn phone_playlist_row_installs_a_copy_drop_target() {
        gtk4::init().unwrap();
        let runtime = display_runtime();
        let row = adw::ActionRow::new();
        install_sync_drop(&row, "phone", "Road", &runtime);

        let controllers = row.observe_controllers();
        assert!((0..controllers.n_items()).any(|index| {
            controllers
                .item(index)
                .is_some_and(|controller| controller.is::<gtk4::DropTarget>())
        }));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn progress_card_rebuilds_from_the_same_live_snapshot() {
        gtk4::init().unwrap();
        let runtime = display_runtime();
        let mut snapshot = view().snapshot;
        snapshot.phase = SyncPhase::Copying;
        snapshot.current_name = Some("song.flac".into());
        snapshot.current_total = Some(100);
        snapshot.current_bytes = 40;
        snapshot.total_tracks = 2;
        snapshot.total_bytes = 200;
        let before = progress_summary(&snapshot);
        let first = progress_group("phone", &snapshot, &runtime);
        let second = progress_group("phone", &snapshot, &runtime);
        assert!(first.first_child().is_some());
        assert!(second.first_child().is_some());
        assert_eq!(before, progress_summary(&snapshot));
        assert_eq!(file_fraction(&snapshot), 0.4);
    }
}

#[cfg(test)]
mod nav_push_tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn device_row_pushes_a_navigation_subpage_instead_of_a_dialog() {
        gtk4::init().unwrap();
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let runtime = DeviceSyncRuntime::new(
            &Rc::new(RefCell::new(conn)),
            reprise_platform_linux::device_sync::DeviceMonitor::new(),
        );

        let navigation = adw::NavigationView::new();
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let root = adw::NavigationPage::new(&content, "Preferences");
        navigation.add(&root);

        // `present_device` walks up from any widget inside the preferences
        // navigation; an unknown device id still pushes the page (titled
        // Disconnected by the immediate subscription callback).
        present_device(&content, "unknown-device", &runtime);

        let visible = navigation.visible_page().expect("a page is visible");
        assert_eq!(visible.title(), copy::text(copy::DISCONNECTED));
    }
}
