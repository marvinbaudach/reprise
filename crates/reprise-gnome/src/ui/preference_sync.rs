//! Android synchronization preferences and the live device browser.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::device_sync::{SyncPhase, SyncSnapshot};

use super::device_sync_runtime::{DeviceSyncRuntime, DeviceSyncState, DeviceView, Subscription};
use super::device_sync_strings as copy;

pub(super) fn build_page(runtime: &Rc<DeviceSyncRuntime>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(copy::text(copy::SYNCHRONIZATION))
        .icon_name("phone-symbolic")
        .build();
    let group = adw::PreferencesGroup::builder()
        .title(copy::text(copy::CONNECTED_DEVICES))
        .build();
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
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

fn render_devices(list: &gtk4::Box, state: &DeviceSyncState, runtime: &Rc<DeviceSyncRuntime>) {
    clear_box(list);
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
        let Some(parent) = row.root().and_downcast::<adw::Window>() else {
            return;
        };
        present_device(&parent, &device_id, &runtime);
    });
    row
}

pub(super) fn present_device(
    parent: &adw::Window,
    device_id: &str,
    runtime: &Rc<DeviceSyncRuntime>,
) {
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
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&scroll));
    let window = adw::Window::builder()
        .application(
            &parent
                .application()
                .expect("preferences have an application"),
        )
        .transient_for(parent)
        .modal(false)
        .destroy_with_parent(true)
        .default_width(680)
        .default_height(720)
        .content(&toolbar)
        .build();
    window.set_size_request(480, 420);

    let refresh_id = device_id.to_string();
    let refresh_runtime = runtime.clone();
    refresh.connect_clicked(move |_| refresh_runtime.refresh_contents(&refresh_id));

    let update_detail = detail.clone();
    let update_window = window.clone();
    let update_id = device_id.to_string();
    let update_runtime = runtime.clone();
    let subscription = runtime.subscribe(Rc::new(move |state| {
        let Some(device) = state.devices.iter().find(|device| device.id == update_id) else {
            update_window.set_title(Some(&copy::text(copy::DISCONNECTED)));
            return;
        };
        update_window.set_title(Some(&device.name));
        render_device_detail(&update_detail, device, &update_window, &update_runtime);
    }));
    retain_subscription(&window, subscription);
    window.present();
}

fn render_device_detail(
    detail: &gtk4::Box,
    device: &DeviceView,
    window: &adw::Window,
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
    detail.append(&playlist_group(device, window, runtime));
    detail.append(&music_group(device));
}

fn playlist_group(
    device: &DeviceView,
    window: &adw::Window,
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
    let window = window.clone();
    add.connect_clicked(move |_| {
        let runtime = runtime_for_add.clone();
        let id = id.clone();
        super::dialogs::prompt_name(
            &window,
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
        let subtitle = if draft {
            copy::text(copy::PLAYLIST_DRAFT)
        } else {
            copy::playlist_entries(entries)
        };
        let row = adw::ActionRow::builder()
            .title(name)
            .subtitle(subtitle)
            .build();
        row.add_prefix(&gtk4::Image::from_icon_name("view-list-symbolic"));
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

#[cfg(test)]
mod tests {
    use reprise_platform_linux::device_sync::DeviceContents;

    use super::*;

    fn view() -> DeviceView {
        DeviceView {
            id: "phone".into(),
            name: "Phone".into(),
            root_uri: "mtp://phone".into(),
            icon: gtk4::gio::ThemedIcon::new("phone-symbolic").upcast(),
            reconnectable: true,
            connected: true,
            available_bytes: Some(1_024),
            contents: DeviceContents::default(),
            scanning: false,
            scan_error: None,
            draft_playlists: vec!["Road".into()],
            snapshot: reprise_core::device_sync::DeviceQueue::new().snapshot(),
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
        let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        render_devices(&list, &DeviceSyncState::default(), &runtime);
        let status = list
            .first_child()
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
