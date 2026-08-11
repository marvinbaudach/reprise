//! The device page's single playlists synchronization target.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::device_sync::aggregate_balance;
use reprise_core::device_sync::device_view::project_category_segments;

use super::device_sync_category_bar::CategoryStorageBar;
use super::device_sync_content_copy::{playlist_result_text, playlist_rule_text};
use super::device_sync_runtime::{DeviceSyncRuntime, DeviceView};
use super::device_sync_strings;
use super::device_sync_verification_copy::verification_copy;
use crate::ui::style::category_colors::music_css_class;

#[derive(Clone)]
pub(super) struct ContentPanelActions {
    pub(super) set_remove_deleted: Rc<dyn Fn(bool)>,
    pub(super) set_sync_automatically: Rc<dyn Fn(bool)>,
    pub(super) scan_device: Rc<dyn Fn()>,
    pub(super) open_folder_browser: Rc<dyn Fn(gtk4::Widget)>,
    pub(super) open_picker: Rc<dyn Fn(gtk4::Widget)>,
}

impl ContentPanelActions {
    pub(super) fn for_runtime(runtime: &Rc<DeviceSyncRuntime>, device_id: &str) -> Self {
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
        let scan_device = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move || runtime.refresh_contents(&device_id)) as Rc<dyn Fn()>
        };
        let open_folder_browser = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move |parent: gtk4::Widget| {
                super::device_sync_target_browser::present(&parent, &runtime, &device_id);
            }) as Rc<dyn Fn(gtk4::Widget)>
        };
        let open_picker = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move |parent: gtk4::Widget| {
                super::device_sync_picker::present(&parent, &runtime, &device_id);
            }) as Rc<dyn Fn(gtk4::Widget)>
        };
        Self {
            set_remove_deleted,
            set_sync_automatically,
            scan_device,
            open_folder_browser,
            open_picker,
        }
    }
}

struct TargetRowWidgets {
    path: gtk4::Label,
    rule: gtk4::Label,
    result_title: gtk4::Label,
    result_detail: gtk4::Label,
}

pub(super) struct ContentPanel {
    root: adw::Bin,
    header: gtk4::Box,
    verification_title: gtk4::Label,
    scan_button: gtk4::Button,
    storage_bar: CategoryStorageBar,
    free_space_line: gtk4::Label,
    target_row: TargetRowWidgets,
    balance_label: gtk4::Label,
    remove_deleted_switch: gtk4::Switch,
    sync_automatically_switch: gtk4::Switch,
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
        let target_list = gtk4::ListBox::new();
        target_list.set_selection_mode(gtk4::SelectionMode::None);
        let target_row = build_target_row(&target_list, actions);

        let balance_label = gtk4::Label::new(None);
        balance_label.add_css_class("heading");
        balance_label.set_xalign(0.0);
        balance_label.set_hexpand(true);
        let summary = gtk4::Box::new(gtk4::Orientation::Horizontal, 18);
        summary.append(&balance_label);
        free_space_line.set_halign(gtk4::Align::End);
        summary.append(&free_space_line);

        let remove_deleted_switch = labeled_switch(
            &device_sync_strings::text(device_sync_strings::REMOVE_FROM_PHONE),
            {
                let set_remove_deleted = actions.set_remove_deleted.clone();
                let updating = updating.clone();
                move |value| {
                    if !updating.get() {
                        set_remove_deleted(value);
                    }
                }
            },
        );
        let sync_automatically_switch = labeled_switch(
            &device_sync_strings::text(device_sync_strings::SYNC_AUTOMATICALLY),
            {
                let set_sync_automatically = actions.set_sync_automatically.clone();
                let updating = updating.clone();
                move |value| {
                    if !updating.get() {
                        set_sync_automatically(value);
                    }
                }
            },
        );

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.append(storage_bar.widget());
        content.append(&target_list);
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
        let root = adw::Bin::builder().child(&content).build();
        root.add_css_class("card");

        Self {
            root,
            header,
            verification_title,
            scan_button,
            storage_bar,
            free_space_line,
            target_row,
            balance_label,
            remove_deleted_switch: remove_deleted_switch.1,
            sync_automatically_switch: sync_automatically_switch.1,
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

        let reading = &device.target_reading;
        let balance = aggregate_balance(std::slice::from_ref(reading));
        let segments =
            project_category_segments(&device.storage, balance.bytes_to_copy, balance.bytes_freed);
        self.storage_bar.update(segments);
        self.free_space_line.set_text(&segments.map_or_else(
            || device_sync_strings::text(device_sync_strings::SPACE_UNKNOWN),
            |segments| {
                device_sync_strings::free_space_line(
                    segments.free_before_bytes,
                    segments.free_after_bytes,
                )
            },
        ));

        let content_row = &device.content_row;
        self.target_row
            .path
            .set_text(&device_sync_strings::target_folder(
                &content_row.target_path,
            ));
        self.target_row.rule.set_text(&playlist_rule_text(
            &device.page.playlists,
            device.page.unique_track_count,
            device.keep_smart_playlists_updated,
        ));
        let (title, detail) = playlist_result_text(content_row, reading);
        self.target_row.result_title.set_text(&title);
        self.target_row.result_detail.set_text(&detail);
        self.balance_label
            .set_text(&device_sync_strings::balance_text(&balance));

        set_switch(&self.remove_deleted_switch, device.settings.remove_deleted);
        set_switch(
            &self.sync_automatically_switch,
            device.settings.sync_automatically,
        );
        self.updating.set(false);
    }
}

fn build_target_row(list: &gtk4::ListBox, actions: &ContentPanelActions) -> TargetRowWidgets {
    let icon = gtk4::Image::from_icon_name("view-list-symbolic");
    icon.add_css_class(music_css_class());
    icon.set_pixel_size(24);
    let title = gtk4::Label::new(Some(&device_sync_strings::text(
        device_sync_strings::PLAYLISTS,
    )));
    title.add_css_class("heading");
    title.set_xalign(0.0);
    let path = detail("");
    let rule = detail("");
    let labels = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    labels.set_hexpand(true);
    labels.append(&title);
    labels.append(&path);
    labels.append(&rule);

    let result_title = gtk4::Label::new(None);
    result_title.add_css_class("heading");
    result_title.set_xalign(1.0);
    let result_detail = detail("");
    result_detail.set_xalign(1.0);
    let result = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    result.set_valign(gtk4::Align::Center);
    result.append(&result_title);
    result.append(&result_detail);

    let choose = gtk4::Button::with_label(&device_sync_strings::text(
        device_sync_strings::CHOOSE_CONTENT,
    ));
    {
        let open_picker = actions.open_picker.clone();
        choose.connect_clicked(move |button| open_picker(button.clone().upcast()));
    }
    let change_folder = gtk4::Button::with_label(&device_sync_strings::text(
        device_sync_strings::CHANGE_FOLDER,
    ));
    {
        let open_folder_browser = actions.open_folder_browser.clone();
        change_folder.connect_clicked(move |button| open_folder_browser(button.clone().upcast()));
    }
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    row.set_margin_top(8);
    row.set_margin_bottom(8);
    row.append(&icon);
    row.append(&labels);
    row.append(&result);
    row.append(&choose);
    row.append(&change_folder);
    list.append(&row);
    TargetRowWidgets {
        path,
        rule,
        result_title,
        result_detail,
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

fn detail(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class("dim-label");
    label.set_xalign(0.0);
    label.set_wrap(true);
    label
}

fn set_switch(widget: &gtk4::Switch, active: bool) {
    widget.set_active(active);
    widget.set_state(active);
}
