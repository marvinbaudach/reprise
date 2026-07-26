//! Dedicated view for one connected MTP device.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::device_sync_runtime::{
    DeviceSyncRuntime, DeviceSyncState, DeviceTrackStatus, DeviceTrackView, DeviceView,
    PlannedSyncPhase, Subscription,
};
use crate::ui::device_sync_strings;

type SettingsCallback = Rc<dyn Fn()>;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum TrackFilter {
    #[default]
    All,
    Queued,
    Remove,
    Synced,
}

pub(in crate::ui) struct DeviceViewPage {
    root: gtk4::ScrolledWindow,
    content: gtk4::Box,
    runtime: Rc<DeviceSyncRuntime>,
    serial: RefCell<Option<String>>,
    latest: RefCell<DeviceSyncState>,
    filter: Cell<TrackFilter>,
    on_settings: RefCell<Option<SettingsCallback>>,
    _subscription: RefCell<Option<Subscription>>,
}

impl DeviceViewPage {
    pub(in crate::ui) fn new(runtime: &Rc<DeviceSyncRuntime>) -> Rc<Self> {
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 18);
        content.set_margin_top(24);
        content.set_margin_bottom(24);
        content.set_margin_start(24);
        content.set_margin_end(24);
        let root = gtk4::ScrolledWindow::builder()
            .child(&content)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .build();
        let page = Rc::new(Self {
            root,
            content,
            runtime: runtime.clone(),
            serial: RefCell::new(None),
            latest: RefCell::new(DeviceSyncState::default()),
            filter: Cell::new(TrackFilter::All),
            on_settings: RefCell::new(None),
            _subscription: RefCell::new(None),
        });
        let weak = Rc::downgrade(&page);
        let subscription = runtime.subscribe(Rc::new(move |state| {
            let Some(page) = weak.upgrade() else { return };
            page.latest.replace(state);
            page.render();
        }));
        page._subscription.replace(Some(subscription));
        page
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::ScrolledWindow {
        &self.root
    }

    pub(in crate::ui) fn show_device(self: &Rc<Self>, serial: &str) {
        self.serial.replace(Some(serial.to_string()));
        self.filter.set(TrackFilter::All);
        self.render();
    }

    pub(in crate::ui) fn set_on_settings(&self, callback: impl Fn() + 'static) {
        self.on_settings.replace(Some(Rc::new(callback)));
    }

    fn render(self: &Rc<Self>) {
        while let Some(child) = self.content.first_child() {
            self.content.remove(&child);
        }
        let serial = self.serial.borrow().clone();
        let device = serial.as_deref().and_then(|serial| {
            self.latest
                .borrow()
                .devices
                .iter()
                .find(|device| device.id == serial && device.connected)
                .cloned()
        });
        let Some(device) = device else {
            self.content.append(
                &adw::StatusPage::builder()
                    .icon_name("phone-symbolic")
                    .title("Device disconnected")
                    .description("Reconnect the device to continue synchronization.")
                    .build(),
            );
            return;
        };
        self.content.append(&self.build_header(&device));
        self.content.append(&self.build_delta(&device));
        self.content.append(&self.build_chips(&device));
        self.content.append(&self.build_track_list(&device));
    }

    fn build_header(self: &Rc<Self>, device: &DeviceView) -> gtk4::Box {
        let box_ = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        let icon = gtk4::Image::from_gicon(&device.icon);
        icon.set_pixel_size(48);
        row.append(&icon);
        let identity = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
        let name = gtk4::Label::new(Some(&device.name));
        name.add_css_class("title-1");
        name.set_xalign(0.0);
        identity.append(&name);
        let detail = device.last_sync.map_or_else(
            || "MTP · connected · never synchronized".to_string(),
            |time| {
                format!(
                    "MTP · connected · last sync {}",
                    time.format("%Y-%m-%d %H:%M")
                )
            },
        );
        let subtitle = gtk4::Label::new(Some(&detail));
        subtitle.add_css_class("dim-label");
        subtitle.set_xalign(0.0);
        identity.append(&subtitle);
        identity.set_hexpand(true);
        row.append(&identity);

        let syncing = is_syncing(&device.sync_phase);
        let sync = gtk4::Button::with_label(if syncing { "Cancel" } else { "Sync now" });
        sync.add_css_class(if syncing {
            "destructive-action"
        } else {
            "suggested-action"
        });
        sync.set_sensitive(syncing || device.delta.as_ref().is_some_and(has_delta));
        let id = device.id.clone();
        sync.set_action_name(Some("app.sync-device"));
        sync.set_action_target_value(Some(&id.to_variant()));
        row.append(&sync);

        let settings = gtk4::Button::with_label("Sync settings…");
        let callback = self.on_settings.borrow().clone();
        settings.connect_clicked(move |_| {
            if let Some(callback) = &callback {
                callback();
            }
        });
        row.append(&settings);
        let eject = gtk4::Button::builder()
            .icon_name("media-eject-symbolic")
            .tooltip_text(device_sync_strings::eject_tooltip(syncing))
            .sensitive(!syncing)
            .build();
        let runtime = self.runtime.clone();
        let id = device.id.clone();
        eject.connect_clicked(move |_| runtime.eject(&id));
        row.append(&eject);
        box_.append(&row);

        let storage = reprise_core::device_sync::storage_composition(&device.storage);
        if storage.knowledge == reprise_core::device_sync::StorageKnowledge::Complete {
            box_.append(&build_storage_bar(&storage));
        }
        let other = storage.other_used_bytes.map_or_else(
            || "Other data unknown".to_string(),
            |bytes| format!("Other data {}", format_bytes(bytes)),
        );
        let free = storage
            .free_bytes
            .map_or_else(|| "unknown".to_string(), format_bytes);
        let legend = gtk4::Label::new(Some(&format!(
            "Reprise {} · Other music {} · {other} · Free {free}",
            format_bytes(storage.reprise_music_bytes),
            format_bytes(storage.other_music_bytes),
        )));
        legend.add_css_class("device-storage-legend");
        legend.set_xalign(0.0);
        legend.set_wrap(true);
        if storage.knowledge != reprise_core::device_sync::StorageKnowledge::Complete {
            legend.set_tooltip_text(Some(&device_sync_strings::text(
                device_sync_strings::STORAGE_TOTALS_UNKNOWN,
            )));
        }
        box_.append(&legend);
        box_
    }

    fn build_delta(&self, device: &DeviceView) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::builder()
            .title("Synchronization")
            .build();
        let (title, subtitle, fraction) = phase_copy(device);
        let row = adw::ActionRow::builder()
            .title(title)
            .subtitle(subtitle)
            .build();
        group.add(&row);
        if fraction > 0.0 {
            let progress = gtk4::ProgressBar::new();
            progress.set_fraction(fraction);
            group.add(&progress);
        }
        if let Some(error) = &device.sync_error {
            let title = if error.message.starts_with("sync needs ") {
                "Device full"
            } else {
                "Synchronization issue"
            };
            let error_row = adw::ActionRow::builder()
                .title(title)
                .subtitle(&error.message)
                .build();
            error_row.add_css_class("error");
            group.add(&error_row);
        }
        group
    }

    fn build_chips(self: &Rc<Self>, device: &DeviceView) -> gtk4::Box {
        let statuses = device
            .tracks
            .iter()
            .map(|track| track.status)
            .collect::<Vec<_>>();
        let counts = status_counts(&statuses);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        for (filter, label, count) in [
            (TrackFilter::All, "All", counts[0]),
            (TrackFilter::Queued, "↑ Queued", counts[1]),
            (TrackFilter::Remove, "− To remove", counts[2]),
            (TrackFilter::Synced, "✓ Synced", counts[3]),
        ] {
            let button = gtk4::Button::with_label(&format!("{label} · {count}"));
            if self.filter.get() == filter {
                button.add_css_class("suggested-action");
            } else {
                button.add_css_class("flat");
            }
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                let Some(page) = weak.upgrade() else { return };
                page.filter.set(filter);
                page.render();
            });
            row.append(&button);
        }
        row
    }

    fn build_track_list(self: &Rc<Self>, device: &DeviceView) -> gtk4::ListBox {
        let list = gtk4::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk4::SelectionMode::None);
        for track in device
            .tracks
            .iter()
            .filter(|track| matches_filter(track.status, self.filter.get()))
        {
            list.append(&self.build_track_row(device, track));
        }
        if list.first_child().is_none() {
            let row = adw::ActionRow::builder()
                .title("No tracks in this category")
                .build();
            list.append(&row);
        }
        list
    }

    fn build_track_row(
        self: &Rc<Self>,
        device: &DeviceView,
        track: &DeviceTrackView,
    ) -> adw::ActionRow {
        let glyph = match track.status {
            DeviceTrackStatus::Queued => "↑",
            DeviceTrackStatus::Remove => "−",
            DeviceTrackStatus::Synced => "✓",
        };
        let subtitle = format!(
            "{} · {} · {}",
            track.artist,
            format_bytes(track.size),
            format_duration(track.duration_ms)
        );
        let row = adw::ActionRow::builder()
            .title(&track.title)
            .subtitle(subtitle)
            .build();
        let status = gtk4::Label::new(Some(glyph));
        status.add_css_class("title-3");
        row.add_prefix(&status);
        if track.pinned {
            let pin = gtk4::Image::from_icon_name("view-pin-symbolic");
            pin.set_tooltip_text(Some(&device_sync_strings::text(
                device_sync_strings::KEPT_ON_DEVICE,
            )));
            row.add_suffix(&pin);
        }
        if track.status != DeviceTrackStatus::Queued {
            install_pin_menu(&row, &self.runtime, device, track);
        }
        row
    }
}

fn install_pin_menu(
    row: &adw::ActionRow,
    runtime: &Rc<DeviceSyncRuntime>,
    device: &DeviceView,
    track: &DeviceTrackView,
) {
    let menu = gio::Menu::new();
    menu.append(
        Some(if track.pinned {
            "Allow removal"
        } else {
            "Keep on device"
        }),
        Some("track.toggle-pin"),
    );
    let actions = gio::SimpleActionGroup::new();
    let action = gio::SimpleAction::new("toggle-pin", None);
    let runtime = runtime.clone();
    let id = device.id.clone();
    let track_id = track.track_id;
    let pinned = track.pinned;
    action.connect_activate(move |_, _| {
        if let Err(error) = runtime.set_pinned(&id, track_id, !pinned) {
            tracing::warn!(%error, "could not update device track pin");
        }
    });
    actions.add_action(&action);
    row.insert_action_group("track", Some(&actions));
    let popover = gtk4::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(row);
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::for_invoker(row);
    focus_guard.restore_on_popover_close(popover.upcast_ref());
    // input-parity: ACC-8 keyboard=menu-shift-f10
    let click = gtk4::GestureClick::new();
    click.set_button(3);
    let pointer_popover = popover.clone();
    click.connect_pressed(move |_, _, x, y| {
        popup_pin_menu(&pointer_popover, x, y);
    });
    row.add_controller(click);

    let keys = gtk4::EventControllerKey::new();
    let row_for_keys = row.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if !crate::ui::track_list::track_list_context_keys::is_context_menu_shortcut(key, modifiers)
        {
            return gtk4::glib::Propagation::Proceed;
        }
        popup_pin_menu(
            &popover,
            f64::from(row_for_keys.width()) / 2.0,
            f64::from(row_for_keys.height()) / 2.0,
        );
        gtk4::glib::Propagation::Stop
    });
    row.add_controller(keys);
}

fn popup_pin_menu(popover: &gtk4::PopoverMenu, x: f64, y: f64) {
    popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    popover.popup();
}

fn is_syncing(phase: &PlannedSyncPhase) -> bool {
    matches!(
        phase,
        PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing
    )
}

fn has_delta(delta: &reprise_core::device_sync::SyncDelta) -> bool {
    !delta.to_copy.is_empty() || !delta.to_remove.is_empty()
}

fn build_storage_bar(storage: &reprise_core::device_sync::StorageComposition) -> gtk4::Overlay {
    let overlay = gtk4::Overlay::new();
    overlay.add_css_class("device-storage-bar");
    let total_bytes = storage.total_bytes.unwrap_or(1).max(1);
    let total = total_bytes as f64;
    let used = total_bytes.saturating_sub(storage.free_bytes.unwrap_or(0));
    let music = storage
        .reprise_music_bytes
        .saturating_add(storage.other_music_bytes);
    let segments = [
        ("device-storage-free", total_bytes),
        ("device-storage-other", used),
        ("device-storage-other-music", music),
        ("device-storage-reprise", storage.reprise_music_bytes),
    ];
    for (index, (class, cumulative_bytes)) in segments.into_iter().enumerate() {
        let bar = gtk4::ProgressBar::new();
        bar.add_css_class("device-storage-segment");
        bar.add_css_class(class);
        bar.set_fraction((cumulative_bytes as f64 / total).clamp(0.0, 1.0));
        if index == 0 {
            overlay.set_child(Some(&bar));
        } else {
            overlay.add_overlay(&bar);
        }
    }
    overlay
}

pub(in crate::ui) fn css() -> String {
    ".device-storage-bar { min-height: 7px; }
     .device-storage-segment trough { min-height: 7px; border-radius: 4px; background: transparent; }
     .device-storage-segment progress { min-height: 7px; border-radius: 4px; }
     .device-storage-free progress { background-color: alpha(@window_fg_color, 0.10); }
     .device-storage-other progress { background-color: alpha(@window_fg_color, 0.28); }
     .device-storage-other-music progress { background-color: alpha(@accent_color, 0.45); }
     .device-storage-reprise progress { background-color: @accent_color; }
     .device-storage-legend { font-size: 10.5px; color: alpha(@window_fg_color, 0.50); }"
        .into()
}

fn matches_filter(status: DeviceTrackStatus, filter: TrackFilter) -> bool {
    filter == TrackFilter::All
        || matches!(
            (status, filter),
            (DeviceTrackStatus::Queued, TrackFilter::Queued)
                | (DeviceTrackStatus::Remove, TrackFilter::Remove)
                | (DeviceTrackStatus::Synced, TrackFilter::Synced)
        )
}

fn status_counts(statuses: &[DeviceTrackStatus]) -> [usize; 4] {
    let mut counts = [statuses.len(), 0, 0, 0];
    for status in statuses {
        counts[match status {
            DeviceTrackStatus::Queued => 1,
            DeviceTrackStatus::Remove => 2,
            DeviceTrackStatus::Synced => 3,
        }] += 1;
    }
    counts
}

fn phase_copy(device: &DeviceView) -> (String, String, f64) {
    match &device.sync_phase {
        PlannedSyncPhase::ComputingDelta => ("Checking device…".into(), String::new(), 0.0),
        PlannedSyncPhase::Syncing {
            done,
            total,
            current_track,
            bytes_done,
            bytes_total,
            ..
        } => (
            format!("Synchronizing {done} of {total}"),
            device_sync_strings::syncing_subtitle(current_track, *bytes_done, *bytes_total),
            if *bytes_total == 0 {
                0.0
            } else {
                *bytes_done as f64 / *bytes_total as f64
            },
        ),
        PlannedSyncPhase::Finishing => ("Finishing synchronization…".into(), String::new(), 1.0),
        // Nothing selected yet (fresh device, empty selection): guide the user
        // to Sync settings instead of the misleading "Everything in sync ✓" —
        // an empty selection produces an empty delta, which is not the same as
        // "already up to date". Keyed on the selection itself, not the delta.
        PlannedSyncPhase::Idle
            if matches!(
                &device.settings.selection,
                reprise_core::device_sync::DeviceSelection::Sources(sources) if sources.is_empty()
            ) =>
        {
            (
                "Nothing selected to sync yet".into(),
                "Open Sync settings to choose playlists or the entire library.".into(),
                0.0,
            )
        }
        PlannedSyncPhase::Idle => device.delta.as_ref().map_or_else(
            || ("Ready to synchronize".into(), String::new(), 0.0),
            |delta| {
                if has_delta(delta) {
                    (
                        format!(
                            "Next sync: +{} tracks · −{} removed",
                            delta.to_copy.len(),
                            delta.to_remove.len()
                        ),
                        format!(
                            "{} will be copied · about {} s via USB",
                            format_bytes(delta.bytes),
                            delta.est_secs
                        ),
                        0.0,
                    )
                } else {
                    ("Everything in sync ✓".into(), String::new(), 0.0)
                }
            },
        ),
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

fn format_duration(duration_ms: i64) -> String {
    let seconds = duration_ms.max(0) / 1_000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_filter_counts_map_each_sync_state() {
        let statuses = [
            DeviceTrackStatus::Queued,
            DeviceTrackStatus::Synced,
            DeviceTrackStatus::Remove,
            DeviceTrackStatus::Synced,
        ];
        assert_eq!(status_counts(&statuses), [4, 1, 1, 2]);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn storage_bar_builds_all_four_cumulative_segments() {
        if gtk4::init().is_err() {
            return;
        }
        let summary = reprise_core::device_sync::StorageComposition {
            total_bytes: Some(3_000),
            reprise_music_bytes: 1_000,
            other_music_bytes: 200,
            other_used_bytes: Some(1_000),
            free_bytes: Some(800),
            knowledge: reprise_core::device_sync::StorageKnowledge::Complete,
        };

        let overlay = build_storage_bar(&summary);
        let mut child = overlay.first_child();
        let mut segments = Vec::new();
        while let Some(widget) = child {
            let next = widget.next_sibling();
            if let Ok(bar) = widget.downcast::<gtk4::ProgressBar>() {
                segments.push((bar.css_classes(), bar.fraction()));
            }
            child = next;
        }

        assert_eq!(segments.len(), 4);
        for (class, fraction) in [
            ("device-storage-free", 1.0),
            ("device-storage-other", 2_200.0 / 3_000.0),
            ("device-storage-other-music", 1_200.0 / 3_000.0),
            ("device-storage-reprise", 1_000.0 / 3_000.0),
        ] {
            let actual = segments
                .iter()
                .find(|(classes, _)| classes.iter().any(|candidate| candidate == class))
                .map(|(_, fraction)| *fraction)
                .unwrap();
            assert!((actual - fraction).abs() < f64::EPSILON);
        }
    }
}

#[cfg(test)]
mod css_tests {
    #[test]
    fn storage_css_uses_theme_colors_and_covers_every_segment() {
        let css = super::css();
        for marker in [
            ".device-storage-free progress",
            ".device-storage-other progress",
            ".device-storage-other-music progress",
            ".device-storage-reprise progress",
        ] {
            assert!(css.contains(marker), "missing rule: {marker}");
        }
        assert!(!css.contains('#'));
        assert!(!css.contains("@define-color"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn css_parses_in_gtk_without_dropping_declarations() {
        if gtk4::init().is_err() {
            return;
        }
        let combined = format!(
            "{}\n{}",
            crate::ui::style::theme::theme_css(crate::ui::style::theme::Theme::DEFAULT, true),
            super::css()
        );
        let errors = crate::ui::style::css_parse_errors(&combined);
        assert!(
            errors.is_empty(),
            "GTK reported CSS parsing errors: {errors:?}",
        );
    }
}
