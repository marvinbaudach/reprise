//! The device view's Content and Next synchronization sections (design 7a),
//! plus the "Device contents never verified" banner and the two per-device
//! switches. The Content section's target-folder path opens the E6 folder
//! browser (`device_sync_target_browser`, `MTP-31`) via "Change folder…".
//!
//! `MTP-37` (`E-6`, `E-8`): Reprise supports exactly one connected MTP
//! device, so the sync rules that the 2026-07-28 addendum sent to a
//! Preferences page live here instead — that page never carried a "which
//! device" cross-reference to begin with, and with one device the
//! addendum's whole justification (several devices needing one shared
//! rule set) no longer applies. Editable here, per device: each category's
//! target folder (via the browser), its size cap (a `gtk4::SpinButton` in
//! GiB, `None`/0 meaning unlimited), its activation (`SyncTarget::enabled`,
//! `MTP-38`), "Remove from phone when deleted or unsubscribed here", and
//! "Sync automatically when this phone connects". The selection summary
//! ("N of M ... selected") is a live, honest read of the per-item
//! selection edited on the podcast/channel pages and the playlist list
//! (`POD-12`) — deliberately not a second selection control in this row;
//! see `device_view`'s module doc.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::device_sync::device_view::{project_category_segments, DeviceContentsState};
use reprise_core::device_sync::{
    aggregate_balance, summarize_playlist_selection, PodcastSelectionSummary, SyncTargetKind,
    YoutubeSelectionSummary,
};

use super::device_sync_category_bar::CategoryStorageBar;
use super::device_sync_runtime::{DeviceSyncRuntime, DeviceView};
use super::device_sync_strings;

/// A category's cap column is edited in GiB (`MTP-37`); 0 clears the cap.
const GIB_BYTES: u64 = 1024 * 1024 * 1024;
/// Generous but finite upper bound for the cap spin button — large enough
/// not to constrain any real device, small enough that the input stays a
/// spin button rather than needing free-form text entry.
const MAX_CAP_GIB: f64 = 4096.0;

#[derive(Clone)]
pub(super) struct ContentPanelActions {
    pub(super) set_target_enabled: Rc<dyn Fn(SyncTargetKind, bool)>,
    /// `MTP-37`: `None` clears the cap (unlimited), `Some` sets it in bytes.
    pub(super) set_target_cap: Rc<dyn Fn(SyncTargetKind, Option<u64>)>,
    pub(super) set_remove_deleted: Rc<dyn Fn(bool)>,
    pub(super) set_sync_automatically: Rc<dyn Fn(bool)>,
    /// `MTP-43`: "Download missing files before syncing".
    pub(super) set_prepare_before_sync: Rc<dyn Fn(bool)>,
    pub(super) scan_device: Rc<dyn Fn()>,
    /// `MTP-31` (design 7d): opens the target-folder browser for one
    /// category, relative to the widget that triggered it.
    pub(super) open_folder_browser: Rc<dyn Fn(SyncTargetKind, gtk4::Widget)>,
}

impl ContentPanelActions {
    pub(super) fn for_runtime(runtime: &Rc<DeviceSyncRuntime>, device_id: &str) -> Self {
        let set_target_enabled = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move |kind, enabled| {
                if let Err(error) = runtime.set_target_enabled(&device_id, kind, enabled) {
                    tracing::warn!(%error, "could not update Android sync target activation");
                }
            }) as Rc<dyn Fn(SyncTargetKind, bool)>
        };
        let set_target_cap = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move |kind, cap_bytes| {
                if let Err(error) = runtime.set_target_cap(&device_id, kind, cap_bytes) {
                    tracing::warn!(%error, "could not update Android sync target cap");
                }
            }) as Rc<dyn Fn(SyncTargetKind, Option<u64>)>
        };
        let set_remove_deleted = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move |value| {
                if let Err(error) = runtime.set_remove_deleted(&device_id, value) {
                    tracing::warn!(%error, "could not update remove-deleted setting");
                }
            }) as Rc<dyn Fn(bool)>
        };
        let set_sync_automatically = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move |value| {
                if let Err(error) = runtime.set_sync_automatically(&device_id, value) {
                    tracing::warn!(%error, "could not update sync-automatically setting");
                }
            }) as Rc<dyn Fn(bool)>
        };
        let set_prepare_before_sync = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move |value| {
                if let Err(error) = runtime.set_prepare_before_sync(&device_id, value) {
                    tracing::warn!(%error, "could not update prepare-before-sync setting");
                }
            }) as Rc<dyn Fn(bool)>
        };
        let scan_device = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move || runtime.refresh_contents(&device_id)) as Rc<dyn Fn()>
        };
        let open_folder_browser = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move |kind, parent: gtk4::Widget| {
                super::device_sync_target_browser::present(&parent, &runtime, &device_id, kind);
            }) as Rc<dyn Fn(SyncTargetKind, gtk4::Widget)>
        };
        Self {
            set_target_enabled,
            set_target_cap,
            set_remove_deleted,
            set_sync_automatically,
            set_prepare_before_sync,
            scan_device,
            open_folder_browser,
        }
    }
}

struct CategoryRowWidgets {
    kind: SyncTargetKind,
    path: gtk4::Label,
    selection: gtk4::Label,
    size_label: gtk4::Label,
    /// `MTP-37`: the cap in GiB, 0 meaning unlimited. Playlists have no cap
    /// concept (`MTP-38`) so this stays permanently insensitive for that
    /// row — see [`build_category_row`].
    cap_spin: gtk4::SpinButton,
    toggle: gtk4::Switch,
}

pub(super) struct ContentPanel {
    root: adw::Bin,
    verification_title: gtk4::Label,
    verification_detail: gtk4::Label,
    scan_button: gtk4::Button,
    storage_bar: CategoryStorageBar,
    free_space_line: gtk4::Label,
    category_rows: [CategoryRowWidgets; 3],
    next_sync_rows: [gtk4::Label; 3],
    balance_label: gtk4::Label,
    remove_deleted_switch: gtk4::Switch,
    sync_automatically_switch: gtk4::Switch,
    prepare_before_sync_switch: gtk4::Switch,
    updating: Rc<Cell<bool>>,
}

impl ContentPanel {
    pub(super) fn new(actions: &ContentPanelActions) -> Self {
        let updating = Rc::new(Cell::new(false));

        let verification_title = heading("");
        let verification_detail = detail("");
        let scan_button = gtk4::Button::with_label("Scan device");
        {
            let scan = actions.scan_device.clone();
            scan_button.connect_clicked(move |_| scan());
        }
        let verification_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        let verification_labels = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        verification_labels.set_hexpand(true);
        verification_labels.append(&verification_title);
        verification_labels.append(&verification_detail);
        verification_row.append(&verification_labels);
        verification_row.append(&scan_button);

        let storage_title = heading("Storage by category");
        let storage_bar = CategoryStorageBar::new();
        let free_space_line = detail("");
        let storage_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        storage_box.append(&storage_title);
        storage_box.append(storage_bar.widget());
        storage_box.append(&free_space_line);

        let content_title = heading("Content");
        let category_list = gtk4::ListBox::new();
        category_list.set_selection_mode(gtk4::SelectionMode::None);
        category_list.set_show_separators(true);
        let category_rows = SyncTargetKind::ALL
            .map(|kind| build_category_row(kind, &category_list, actions, &updating));

        let next_sync_title = heading("Next synchronization");
        let next_sync_rows = std::array::from_fn(|_| detail(""));
        let next_sync_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        for row in &next_sync_rows {
            next_sync_box.append(row);
        }
        let balance_label = gtk4::Label::new(None);
        balance_label.add_css_class("heading");
        balance_label.set_xalign(0.0);
        next_sync_box.append(&balance_label);

        let remove_deleted_switch =
            labeled_switch("Remove from phone when deleted or unsubscribed here", {
                let set_remove_deleted = actions.set_remove_deleted.clone();
                let updating = updating.clone();
                move |value| {
                    if !updating.get() {
                        set_remove_deleted(value);
                    }
                }
            });
        let sync_automatically_switch =
            labeled_switch("Sync automatically when this phone connects", {
                let set_sync_automatically = actions.set_sync_automatically.clone();
                let updating = updating.clone();
                move |value| {
                    if !updating.get() {
                        set_sync_automatically(value);
                    }
                }
            });
        // `MTP-43`: defaults on (`DeviceSettings::prepare_before_sync`).
        // Offline/metered overrides are `preparation::plan_preparation`'s
        // job — this switch only ever stores what the user chose here.
        let prepare_before_sync_switch = labeled_switch("Download missing files before syncing", {
            let set_prepare_before_sync = actions.set_prepare_before_sync.clone();
            let updating = updating.clone();
            move |value| {
                if !updating.get() {
                    set_prepare_before_sync(value);
                }
            }
        });

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.append(&verification_row);
        content.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        content.append(&storage_box);
        content.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        content.append(&content_title);
        content.append(&category_list);
        content.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        content.append(&next_sync_title);
        content.append(&next_sync_box);
        content.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        content.append(&row_widget(
            &remove_deleted_switch.0,
            &remove_deleted_switch.1,
        ));
        content.append(&row_widget(
            &sync_automatically_switch.0,
            &sync_automatically_switch.1,
        ));
        content.append(&row_widget(
            &prepare_before_sync_switch.0,
            &prepare_before_sync_switch.1,
        ));

        let root = adw::Bin::builder().child(&content).build();
        root.add_css_class("card");

        Self {
            root,
            verification_title,
            verification_detail,
            scan_button,
            storage_bar,
            free_space_line,
            category_rows,
            next_sync_rows,
            balance_label,
            remove_deleted_switch: remove_deleted_switch.1,
            sync_automatically_switch: sync_automatically_switch.1,
            prepare_before_sync_switch: prepare_before_sync_switch.1,
            updating,
        }
    }

    pub(super) fn root(&self) -> &adw::Bin {
        &self.root
    }

    pub(super) fn update(&self, device: &DeviceView) {
        self.updating.set(true);

        let (title, subtitle, can_scan) = verification_copy(&device.contents_state);
        self.verification_title.set_text(&title);
        self.verification_detail.set_text(&subtitle);
        self.scan_button.set_sensitive(can_scan);

        let balance = aggregate_balance(&device.category_readings);
        let segments = project_category_segments(
            &device.storage,
            device.youtube_bytes,
            device.podcast_bytes,
            balance.bytes_to_copy,
            balance.bytes_freed,
        );
        self.storage_bar.update(segments);
        self.free_space_line.set_text(&segments.map_or_else(
            || "Free space unknown".to_string(),
            |segments| {
                device_sync_strings::free_space_line(
                    segments.free_before_bytes,
                    segments.free_after_bytes,
                )
            },
        ));

        for (row, content_row) in self.category_rows.iter().zip(&device.content_rows) {
            row.path.set_text(&content_row.target_path);
            row.selection.set_text(&selection_summary_text(
                row.kind,
                &device.page.playlists,
                device.page.unique_track_count,
                device.youtube_selection,
                device.podcast_selection,
            ));
            row.size_label.set_text(&format!(
                "{} on device",
                device_sync_strings::file_size(content_row.size_on_device_bytes)
            ));
            row.cap_spin
                .set_value(cap_bytes_to_gib(content_row.cap_bytes));
            row.cap_spin
                .set_tooltip_text(Some(&device_sync_strings::cap_text(content_row.cap_bytes)));
            row.toggle.set_active(content_row.target_enabled);
            row.toggle.set_state(content_row.target_enabled);
        }

        for ((kind, reading), label) in SyncTargetKind::ALL
            .iter()
            .zip(&device.category_readings)
            .zip(&self.next_sync_rows)
        {
            label.set_text(&format!(
                "{}: {}",
                device_sync_strings::category_name(*kind),
                device_sync_strings::category_reading_text(reading)
            ));
        }
        self.balance_label
            .set_text(&device_sync_strings::balance_text(&balance));

        self.remove_deleted_switch
            .set_active(device.settings.remove_deleted);
        self.remove_deleted_switch
            .set_state(device.settings.remove_deleted);
        self.sync_automatically_switch
            .set_active(device.settings.sync_automatically);
        self.sync_automatically_switch
            .set_state(device.settings.sync_automatically);
        self.prepare_before_sync_switch
            .set_active(device.settings.prepare_before_sync);
        self.prepare_before_sync_switch
            .set_state(device.settings.prepare_before_sync);

        self.updating.set(false);
    }
}

fn build_category_row(
    kind: SyncTargetKind,
    list: &gtk4::ListBox,
    actions: &ContentPanelActions,
    updating: &Rc<Cell<bool>>,
) -> CategoryRowWidgets {
    let title = gtk4::Label::new(Some(device_sync_strings::category_name(kind)));
    title.add_css_class("heading");
    title.set_xalign(0.0);
    let path = detail("");
    let browse_button = gtk4::Button::with_label("Change folder…");
    browse_button.add_css_class("flat");
    browse_button.set_halign(gtk4::Align::Start);
    {
        let open_folder_browser = actions.open_folder_browser.clone();
        browse_button.connect_clicked(move |button| {
            open_folder_browser(kind, button.clone().upcast());
        });
    }
    let path_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    path_row.append(&path);
    path_row.append(&browse_button);
    let selection = detail("");
    let size_label = detail("");

    // `MTP-37`: the cap becomes a real editable control here. Playlists
    // have no cap concept (`MTP-38`'s `default_cap_bytes` is always
    // `None`, and no eviction path reads a playlist cap), so that row's
    // spin button stays permanently insensitive rather than offering a
    // control that would silently do nothing.
    let cap_spin = gtk4::SpinButton::with_range(0.0, MAX_CAP_GIB, 1.0);
    cap_spin.set_digits(0);
    cap_spin.set_valign(gtk4::Align::Center);
    cap_spin.update_property(&[gtk4::accessible::Property::Label(&format!(
        "{} cap in gibibytes, 0 for no cap",
        device_sync_strings::category_name(kind)
    ))]);
    if kind == SyncTargetKind::Playlists {
        cap_spin.set_sensitive(false);
        cap_spin.set_tooltip_text(Some("Playlists have no size cap"));
    } else {
        let set_target_cap = actions.set_target_cap.clone();
        let updating = updating.clone();
        cap_spin.connect_value_changed(move |spin| {
            if updating.get() {
                return;
            }
            let value = spin.value();
            let cap_bytes = if value <= 0.0 {
                None
            } else {
                Some((value * GIB_BYTES as f64).round() as u64)
            };
            set_target_cap(kind, cap_bytes);
        });
    }
    let cap_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    cap_row.append(&gtk4::Label::new(Some("Cap (GiB):")));
    cap_row.append(&cap_spin);

    let labels = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    labels.set_hexpand(true);
    labels.append(&title);
    labels.append(&path_row);
    labels.append(&selection);
    labels.append(&size_label);
    labels.append(&cap_row);

    let toggle = gtk4::Switch::new();
    toggle.set_valign(gtk4::Align::Center);
    toggle.update_property(&[gtk4::accessible::Property::Label(&format!(
        "Sync {} on this device",
        device_sync_strings::category_name(kind)
    ))]);
    {
        let set_target_enabled = actions.set_target_enabled.clone();
        let updating = updating.clone();
        toggle.connect_state_set(move |_, value| {
            if !updating.get() {
                set_target_enabled(kind, value);
            }
            gtk4::glib::Propagation::Proceed
        });
    }

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    row.set_margin_top(8);
    row.set_margin_bottom(8);
    row.append(&labels);
    row.append(&toggle);
    list.append(&row);

    CategoryRowWidgets {
        kind,
        path,
        selection,
        size_label,
        cap_spin,
        toggle,
    }
}

fn labeled_switch(
    label_text: &str,
    on_change: impl Fn(bool) + 'static,
) -> (gtk4::Label, gtk4::Switch) {
    let label = gtk4::Label::new(Some(label_text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_hexpand(true);
    let switch = gtk4::Switch::new();
    switch.set_valign(gtk4::Align::Center);
    switch.update_property(&[gtk4::accessible::Property::Label(label_text)]);
    switch.connect_state_set(move |_, value| {
        on_change(value);
        gtk4::glib::Propagation::Proceed
    });
    (label, switch)
}

fn row_widget(label: &gtk4::Label, switch: &gtk4::Switch) -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    row.append(label);
    row.append(switch);
    row
}

fn heading(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class("heading");
    label.set_xalign(0.0);
    label
}

fn detail(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class("dim-label");
    label.set_xalign(0.0);
    label.set_wrap(true);
    label
}

/// `MTP-26`: the verification banner's title, detail text, and whether
/// "Scan device" should be enabled. Pure — kept separate from the widget
/// so the exact copy is unit-tested without a display.
fn verification_copy(state: &DeviceContentsState) -> (String, String, bool) {
    match state {
        DeviceContentsState::NeverVerified => (
            "Device contents never verified".to_string(),
            "Scan the device to see what's already there before syncing.".to_string(),
            true,
        ),
        DeviceContentsState::Verifying => (
            "Verifying device contents…".to_string(),
            "Reading storage over MTP — this can take a moment.".to_string(),
            false,
        ),
        DeviceContentsState::Verified => (
            "Device contents verified".to_string(),
            "Storage, content and the sync plan below reflect what Reprise found.".to_string(),
            true,
        ),
        DeviceContentsState::Failed(error) => (
            "Could not verify device contents".to_string(),
            error.clone(),
            true,
        ),
    }
}

/// `MTP-37`: design 7a's per-category selection summary. Every branch is
/// now a live read of real per-device selection state — Playlists via the
/// existing engine (`selection::summarize_playlist_selection`, `MTP-41`),
/// YouTube and podcasts via `POD-12`'s per-device subscription selection
/// (`podcasts::phone_sync::selection_summary`, gathered by the caller into
/// `youtube_selection`/`podcast_selection`). None of these are edited in
/// this row — see the module doc on why that stays intentional — but none
/// of them are fabricated either, unlike the "Rules from Preferences" stub
/// this replaces.
fn selection_summary_text(
    kind: SyncTargetKind,
    playlists: &[reprise_core::device_sync::SyncPlaylistRow],
    unique_track_count: usize,
    youtube_selection: YoutubeSelectionSummary,
    podcast_selection: PodcastSelectionSummary,
) -> String {
    match kind {
        SyncTargetKind::Playlists => {
            let summary = summarize_playlist_selection(playlists, unique_track_count);
            format!(
                "{} of {} selected · {}",
                summary.selected,
                summary.available_total,
                counted(summary.unique_track_count, "unique track", "unique tracks")
            )
        }
        SyncTargetKind::YoutubeAudio => {
            // `MTP-36`: the live pipeline now always resolves a real
            // `latest_per_channel` (the global default or a channel's own
            // override) before this is ever rendered, so `usize::MAX` no
            // longer arrives from `recompute_delta_silent`. The sentinel
            // check stays so a test or an as-yet-unrecomputed view still
            // omits the "latest K each" clause instead of claiming a
            // number nothing enforces.
            if youtube_selection.latest_per_channel == usize::MAX {
                format!(
                    "{} of {} channels selected",
                    youtube_selection.channels_selected, youtube_selection.channels_total
                )
            } else {
                format!(
                    "{} of {} channels · latest {} each",
                    youtube_selection.channels_selected,
                    youtube_selection.channels_total,
                    youtube_selection.latest_per_channel
                )
            }
        }
        SyncTargetKind::PodcastEpisodes => format!(
            "{} of {} shows selected · unplayed downloads only",
            podcast_selection.shows_selected, podcast_selection.shows_total
        ),
    }
}

/// `MTP-37`: the cap spin button's displayed value — GiB, 0 for unlimited.
/// The inverse of the spin button's own `connect_value_changed` conversion
/// in [`build_category_row`].
fn cap_bytes_to_gib(cap_bytes: Option<u64>) -> f64 {
    cap_bytes.map_or(0.0, |bytes| bytes as f64 / GIB_BYTES as f64)
}

fn counted(count: usize, singular: &str, plural: &str) -> String {
    let noun = if count == 1 { singular } else { plural };
    format!("{count} {noun}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mtp_26_verification_copy_names_all_four_states_and_gates_the_scan_action() {
        let (title, _, can_scan) = verification_copy(&DeviceContentsState::NeverVerified);
        assert_eq!(title, "Device contents never verified");
        assert!(can_scan);

        let (_, _, can_scan) = verification_copy(&DeviceContentsState::Verifying);
        assert!(
            !can_scan,
            "a scan already in flight must not offer a second one"
        );

        let (title, _, can_scan) = verification_copy(&DeviceContentsState::Verified);
        assert_eq!(title, "Device contents verified");
        assert!(can_scan);

        let (title, detail, can_scan) =
            verification_copy(&DeviceContentsState::Failed("MTP timeout".into()));
        assert_eq!(title, "Could not verify device contents");
        assert_eq!(detail, "MTP timeout");
        assert!(can_scan, "a failed scan must still offer retry");
    }

    #[test]
    fn selection_summary_reads_the_live_playlist_projection() {
        let playlists = [reprise_core::device_sync::SyncPlaylistRow {
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

        assert_eq!(
            selection_summary_text(
                SyncTargetKind::Playlists,
                &playlists,
                278,
                YoutubeSelectionSummary::default(),
                PodcastSelectionSummary::default(),
            ),
            "1 of 1 selected · 278 unique tracks"
        );
    }

    /// `MTP-37`: the actual live-count *behaviour* — that enabling or
    /// disabling a channel/show for this device changes what these
    /// summaries report — is exercised end to end (DB through
    /// `phone_sync::selection_summary` through the live runtime) by
    /// `device_sync_selection_summary_tests::
    /// mtp_37_the_youtube_selection_summary_changes_when_a_channel_is_enabled_for_this_device`
    /// and its podcast counterpart. This test only pins the exact copy
    /// [`selection_summary_text`] renders for given inputs — it must not be
    /// the only coverage the label has, the same gap that let the old
    /// "Rules from Preferences" stub go unnoticed.
    #[test]
    fn mtp_37_selection_summary_renders_live_youtube_and_podcast_counts() {
        assert_eq!(
            selection_summary_text(
                SyncTargetKind::YoutubeAudio,
                &[],
                0,
                YoutubeSelectionSummary {
                    channels_selected: 2,
                    channels_total: 6,
                    latest_per_channel: usize::MAX,
                },
                PodcastSelectionSummary::default(),
            ),
            "2 of 6 channels selected",
            "MTP-36 has not landed yet, so no 'latest K each' clause is claimed"
        );
        assert_eq!(
            selection_summary_text(
                SyncTargetKind::YoutubeAudio,
                &[],
                0,
                YoutubeSelectionSummary {
                    channels_selected: 2,
                    channels_total: 6,
                    latest_per_channel: 5,
                },
                PodcastSelectionSummary::default(),
            ),
            "2 of 6 channels · latest 5 each",
            "once MTP-36 lands and provides a real latest_per_channel, it is honoured"
        );
        assert_eq!(
            selection_summary_text(
                SyncTargetKind::PodcastEpisodes,
                &[],
                0,
                YoutubeSelectionSummary::default(),
                PodcastSelectionSummary {
                    shows_selected: 3,
                    shows_total: 5,
                },
            ),
            "3 of 5 shows selected · unplayed downloads only"
        );
    }

    #[test]
    fn mtp_37_cap_gib_conversion_round_trips_and_treats_zero_as_unlimited() {
        assert_eq!(cap_bytes_to_gib(None), 0.0);
        assert_eq!(cap_bytes_to_gib(Some(8 * GIB_BYTES)), 8.0);
        assert_eq!(cap_bytes_to_gib(Some(4 * GIB_BYTES)), 4.0);
    }
}
