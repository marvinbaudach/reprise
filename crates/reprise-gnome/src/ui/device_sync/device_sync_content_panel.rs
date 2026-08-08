//! The device view's one-row-per-source synchronization plan. Each row keeps
//! its selection rule, projected result, content picker, folder browser, cap,
//! and target switch together; the cross-category balance follows below.
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
use reprise_core::device_sync::device_view::project_category_segments;
use reprise_core::device_sync::{aggregate_balance, SyncTargetKind};

use super::device_sync_category_bar::CategoryStorageBar;
use super::device_sync_content_copy::{category_result_text, category_rule_prefix};
use super::device_sync_runtime::{DeviceSyncRuntime, DeviceView};
use super::device_sync_strings;
use super::device_sync_verification_copy::verification_copy;
use crate::ui::style::category_colors::category_css_class;

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
    pub(super) open_picker: Rc<dyn Fn(SyncTargetKind, gtk4::Widget)>,
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
        let open_picker = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move |kind, parent: gtk4::Widget| {
                super::device_sync_picker::present(&parent, &runtime, &device_id, kind);
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
            open_picker,
        }
    }
}

struct CategoryRowWidgets {
    kind: SyncTargetKind,
    path: gtk4::Label,
    rule: gtk4::Label,
    result_title: gtk4::Label,
    result_detail: gtk4::Label,
    cap_button: gtk4::MenuButton,
    cap_popover_label: gtk4::Label,
    /// `MTP-37`: the cap in GiB, 0 meaning unlimited. Playlists have no cap
    /// concept (`MTP-38`) so this stays permanently insensitive for that
    /// row — see [`build_category_row`].
    cap_spin: gtk4::SpinButton,
    toggle: gtk4::Switch,
    /// `MTP-46`: the whole row, kept so a source the user switched off can be
    /// hidden outright rather than reduced to a "0 of N" line.
    container: gtk4::Box,
}

pub(super) struct ContentPanel {
    root: adw::Bin,
    header: gtk4::Box,
    verification_title: gtk4::Label,
    scan_button: gtk4::Button,
    storage_bar: CategoryStorageBar,
    free_space_line: gtk4::Label,
    category_rows: [CategoryRowWidgets; 3],
    balance_label: gtk4::Label,
    remove_deleted_switch: gtk4::Switch,
    sync_automatically_switch: gtk4::Switch,
    prepare_before_sync_switch: gtk4::Switch,
    updating: Rc<Cell<bool>>,
}

impl ContentPanel {
    pub(super) fn new(actions: &ContentPanelActions) -> Self {
        let updating = Rc::new(Cell::new(false));

        let verification_title = detail("");
        verification_title.set_halign(gtk4::Align::End);
        let scan_button =
            gtk4::Button::with_label(&device_sync_strings::text(device_sync_strings::RESCAN));
        {
            let scan = actions.scan_device.clone();
            scan_button.connect_clicked(move |_| scan());
        }
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        header.set_hexpand(true);
        header.set_halign(gtk4::Align::End);
        header.set_valign(gtk4::Align::Center);
        header.append(&verification_title);
        header.append(&scan_button);

        let storage_bar = CategoryStorageBar::new();
        let free_space_line = detail("");

        let category_list = gtk4::ListBox::new();
        category_list.set_selection_mode(gtk4::SelectionMode::None);
        category_list.set_show_separators(true);
        let category_rows = SyncTargetKind::ALL
            .map(|kind| build_category_row(kind, &category_list, actions, &updating));

        let balance_label = gtk4::Label::new(None);
        balance_label.add_css_class("heading");
        balance_label.set_xalign(0.0);
        balance_label.set_hexpand(true);
        let summary = gtk4::Box::new(gtk4::Orientation::Horizontal, 18);
        summary.append(&balance_label);
        free_space_line.set_halign(gtk4::Align::End);
        summary.append(&free_space_line);

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
        content.append(storage_bar.widget());
        content.append(&category_list);
        content.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        content.append(&summary);
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
            header,
            verification_title,
            scan_button,
            storage_bar,
            free_space_line,
            category_rows,
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

    pub(super) fn header(&self) -> &gtk4::Box {
        &self.header
    }

    pub(super) fn update(&self, device: &DeviceView) {
        self.updating.set(true);

        let (title, _subtitle, can_scan) =
            verification_copy(&device.contents_state, device.last_sync, chrono::Utc::now());
        self.verification_title.set_text(&title);
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

        for ((row, content_row), reading) in self
            .category_rows
            .iter()
            .zip(&device.content_rows)
            .zip(&device.category_readings)
        {
            // `MTP-46`: a switched-off source is not a category with nothing
            // in it, it is not a category at all.
            row.container.set_visible(match row.kind {
                SyncTargetKind::YoutubeAudio => device.enabled_sources.youtube,
                SyncTargetKind::PodcastEpisodes => device.enabled_sources.rss,
                SyncTargetKind::Playlists => true,
            });
            row.path.set_text(&device_sync_strings::target_folder(
                &content_row.target_path,
            ));
            row.rule.set_text(&category_rule_prefix(
                row.kind,
                &device.page.playlists,
                device.page.unique_track_count,
                device.youtube_selection,
                device.podcast_selection,
                device.keep_smart_playlists_updated,
            ));
            let cap = device_sync_strings::cap_text(content_row.cap_bytes);
            row.cap_button.set_label(&cap);
            row.cap_popover_label.set_text(&cap);
            let (title, detail) = category_result_text(row.kind, content_row, reading);
            row.result_title.set_text(&title);
            row.result_detail.set_text(&detail);
            row.cap_spin
                .set_value(cap_bytes_to_gib(content_row.cap_bytes));
            row.cap_spin
                .set_tooltip_text(Some(&device_sync_strings::cap_text(content_row.cap_bytes)));
            row.toggle.set_active(content_row.target_enabled);
            row.toggle.set_state(content_row.target_enabled);
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
    let icon = category_icon(kind);

    let title = gtk4::Label::new(Some(device_sync_strings::category_name(kind)));
    title.add_css_class("heading");
    title.set_xalign(0.0);
    let path = detail("");
    let change_content = gtk4::Button::with_label(&device_sync_strings::text(
        device_sync_strings::CHANGE_CONTENT,
    ));
    change_content.add_css_class("flat");
    {
        let open_picker = actions.open_picker.clone();
        change_content.connect_clicked(move |button| {
            open_picker(kind, button.clone().upcast());
        });
    }
    let change_folder = gtk4::Button::with_label(&device_sync_strings::text(
        device_sync_strings::CHANGE_FOLDER,
    ));
    change_folder.add_css_class("flat");
    {
        let open_folder_browser = actions.open_folder_browser.clone();
        change_folder.connect_clicked(move |button| {
            open_folder_browser(kind, button.clone().upcast());
        });
    }
    let change_menu_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    change_menu_box.set_margin_top(8);
    change_menu_box.set_margin_bottom(8);
    change_menu_box.set_margin_start(8);
    change_menu_box.set_margin_end(8);
    change_menu_box.append(&path);
    change_menu_box.append(&change_content);
    change_menu_box.append(&change_folder);
    let change_popover = gtk4::Popover::new();
    change_popover.set_child(Some(&change_menu_box));
    let change_button = gtk4::MenuButton::new();
    change_button.set_label(&device_sync_strings::text(device_sync_strings::CHANGE));
    change_button.set_popover(Some(&change_popover));
    // Three rows produce three buttons all labelled "Change…"; only reading
    // order ties one to its category, which the accessibility tree does not
    // convey. Name each after the row it belongs to.
    change_button.update_property(&[gtk4::accessible::Property::Label(
        &device_sync_strings::change_category_label(kind),
    )]);

    let rule = detail("");

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
    let cap_popover_label = detail("");
    let cap_explanation = detail(&device_sync_strings::text(device_sync_strings::CAP_IN_GIB));
    let cap_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    cap_row.set_margin_top(8);
    cap_row.set_margin_bottom(8);
    cap_row.set_margin_start(8);
    cap_row.set_margin_end(8);
    cap_row.append(&cap_popover_label);
    cap_row.append(&cap_spin);
    let cap_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    cap_box.append(&cap_row);
    cap_box.append(&cap_explanation);
    let cap_popover = gtk4::Popover::new();
    cap_popover.set_child(Some(&cap_box));
    let cap_button = gtk4::MenuButton::new();
    cap_button.add_css_class("flat");
    cap_button.set_popover(Some(&cap_popover));
    // Its visible label is the cap phrase itself ("no size limit"), which says
    // the value but not what it belongs to or that it can be edited.
    cap_button.update_property(&[gtk4::accessible::Property::Label(
        &device_sync_strings::change_cap_label(kind),
    )]);

    // The separator belongs to the sentence, so nothing between it and the cap
    // phrase may expand: giving `rule` the extra width pushed the "·" across
    // the row and left it floating on its own, reading as a stray dot.
    let rule_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    rule_row.set_halign(gtk4::Align::Start);
    rule_row.append(&rule);
    rule_row.append(&gtk4::Label::new(Some("·")));
    rule_row.append(&cap_button);

    let labels = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    labels.set_hexpand(true);
    labels.append(&title);
    labels.append(&rule_row);

    let result_title = gtk4::Label::new(None);
    result_title.add_css_class("heading");
    result_title.set_xalign(1.0);
    let result_detail = detail("");
    result_detail.set_xalign(1.0);
    let result = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    result.set_valign(gtk4::Align::Center);
    result.append(&result_title);
    result.append(&result_detail);

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
    row.append(&icon);
    row.append(&labels);
    row.append(&result);
    row.append(&change_button);
    row.append(&toggle);
    list.append(&row);

    CategoryRowWidgets {
        kind,
        path,
        rule,
        result_title,
        result_detail,
        cap_button,
        cap_popover_label,
        cap_spin,
        toggle,
        container: row,
    }
}

fn category_icon(kind: SyncTargetKind) -> gtk4::Image {
    let icon = gtk4::Image::from_icon_name(match kind {
        SyncTargetKind::Playlists => "view-list-symbolic",
        SyncTargetKind::YoutubeAudio => "video-x-generic-symbolic",
        SyncTargetKind::PodcastEpisodes => "audio-x-generic-symbolic",
    });
    // The icon and its storage segment share one fixed category identity;
    // symbolic icons inherit the mode-aware named color through this class.
    icon.add_css_class(category_css_class(kind));
    icon.set_pixel_size(24);
    icon
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

fn detail(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class("dim-label");
    label.set_xalign(0.0);
    label.set_wrap(true);
    label
}

/// `MTP-37`: the cap spin button's displayed value — GiB, 0 for unlimited.
/// The inverse of the spin button's own `connect_value_changed` conversion
/// in [`build_category_row`].
fn cap_bytes_to_gib(cap_bytes: Option<u64>) -> f64 {
    cap_bytes.map_or(0.0, |bytes| bytes as f64 / GIB_BYTES as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mtp_51_cap_gib_conversion_round_trips_and_treats_zero_as_unlimited() {
        assert_eq!(cap_bytes_to_gib(None), 0.0);
        assert_eq!(cap_bytes_to_gib(Some(8 * GIB_BYTES)), 8.0);
        assert_eq!(cap_bytes_to_gib(Some(4 * GIB_BYTES)), 4.0);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn design_2c_row_icons_use_category_classes_at_full_opacity() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        crate::ui::style::install();

        let icons = SyncTargetKind::ALL.map(category_icon);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        for (icon, kind) in icons.iter().zip(SyncTargetKind::ALL) {
            assert!(icon.has_css_class(category_css_class(kind)));
            assert_eq!(icon.opacity(), 1.0);
            row.append(icon);
        }
        let window = gtk4::Window::builder().child(&row).build();
        window.present();
        assert!(crate::ui::test_settle::settle_until_mapped(&row));

        for (scheme, is_dark) in [("dark", true), ("light", false)] {
            crate::ui::style::set_color_scheme(scheme);
            let manager = libadwaita::StyleManager::default();
            assert!(crate::ui::test_settle::settle_until(
                crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
                || manager.is_dark() == is_dark
            ));
            crate::ui::style::set_theme(crate::ui::style::theme::Theme::DEFAULT);
            crate::ui::test_settle::settle_for(std::time::Duration::from_millis(20));

            for (icon, kind) in icons.iter().zip(SyncTargetKind::ALL) {
                assert_eq!(
                    icon.color(),
                    gtk4::gdk::RGBA::parse(crate::ui::style::category_colors::category_color(
                        kind, is_dark
                    ))
                    .unwrap()
                );
            }
        }
        window.close();
    }
}
