//! The result-oriented "On this device" section below playlist selection.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::device_sync::{summarize_playlist_selection, StorageProjectionState};

use super::device_sync_content_copy::playlist_result_text;
use super::device_sync_runtime::{DeviceSyncRuntime, DeviceView};
use super::device_sync_storage_bar::{segments, StorageBar};
use super::device_sync_strings;
use super::device_sync_verification_copy::verification_copy;

#[derive(Clone)]
pub(super) struct OnDeviceActions {
    pub(super) set_remove_deleted: Rc<dyn Fn(bool)>,
    pub(super) set_sync_automatically: Rc<dyn Fn(bool)>,
    pub(super) scan_device: Rc<dyn Fn()>,
    pub(super) open_folder_browser: Rc<dyn Fn(gtk4::Widget)>,
    pub(super) open_playlist_picker: Rc<dyn Fn(gtk4::Widget)>,
    pub(super) dismiss_legacy_media_notice: Rc<dyn Fn()>,
    pub(super) legacy_media_notice_pending: Rc<dyn Fn() -> bool>,
}

impl OnDeviceActions {
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
        let open_playlist_picker = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move |parent: gtk4::Widget| {
                super::device_sync_picker::present(&parent, &runtime, &device_id);
            }) as Rc<dyn Fn(gtk4::Widget)>
        };
        let dismiss_legacy_media_notice = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move || runtime.dismiss_legacy_media_notice(&device_id)) as Rc<dyn Fn()>
        };
        let legacy_media_notice_pending = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move || runtime.legacy_media_notice_pending(&device_id)) as Rc<dyn Fn() -> bool>
        };
        Self {
            set_remove_deleted,
            set_sync_automatically,
            scan_device,
            open_folder_browser,
            open_playlist_picker,
            dismiss_legacy_media_notice,
            legacy_media_notice_pending,
        }
    }
}

pub(super) struct OnDeviceSection {
    root: gtk4::Box,
    verification_title: gtk4::Label,
    pub(super) legacy_notice: libadwaita::Banner,
    legacy_notice_pending: Rc<Cell<bool>>,
    check_button: gtk4::Button,
    storage_bar: StorageBar,
    storage_legend: gtk4::Label,
    balance: gtk4::Label,
    policy: gtk4::Label,
    remove_deleted_switch: gtk4::Switch,
    sync_automatically_switch: gtk4::Switch,
    updating: Rc<Cell<bool>>,
}

impl OnDeviceSection {
    pub(super) fn new(actions: &OnDeviceActions, review_playlists: Rc<dyn Fn()>) -> Self {
        let updating = Rc::new(Cell::new(false));
        let title = label(
            &device_sync_strings::text(device_sync_strings::ON_THIS_DEVICE),
            "title-2",
        );
        title.set_hexpand(true);
        let verification_title = detail("");
        verification_title.set_halign(gtk4::Align::End);
        let check_button =
            gtk4::Button::with_label(&device_sync_strings::text(device_sync_strings::CHECK_AGAIN));
        {
            let scan = actions.scan_device.clone();
            check_button.connect_clicked(move |_| scan());
        }
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        header.set_valign(gtk4::Align::Center);
        header.append(&title);
        header.append(&verification_title);
        header.append(&check_button);

        let storage_bar = StorageBar::new();
        let legacy_notice = libadwaita::Banner::new("");
        let legacy_notice_pending = Rc::new(Cell::new((actions.legacy_media_notice_pending)()));
        legacy_notice.set_button_label(Some(&device_sync_strings::text(
            device_sync_strings::DISMISS,
        )));
        {
            let dismiss = actions.dismiss_legacy_media_notice.clone();
            let pending = legacy_notice_pending.clone();
            legacy_notice.connect_button_clicked(move |banner| {
                pending.set(false);
                banner.set_revealed(false);
                dismiss();
            });
        }
        let storage_legend = detail("");
        let balance = label("", "heading");
        let policy = detail("");
        let review = gtk4::Button::with_label(&device_sync_strings::text(
            device_sync_strings::REVIEW_PLAYLISTS_ABOVE,
        ));
        review.add_css_class("link");
        review.connect_clicked(move |_| review_playlists());
        let change_folder = gtk4::Button::with_label(&device_sync_strings::text(
            device_sync_strings::CHANGE_FOLDER,
        ));
        {
            let open = actions.open_folder_browser.clone();
            change_folder.connect_clicked(move |button| open(button.clone().upcast()));
        }
        let set_limit =
            gtk4::Button::with_label(&device_sync_strings::text(device_sync_strings::SET_LIMIT));
        set_limit.set_sensitive(false);
        set_limit.set_tooltip_text(Some(&device_sync_strings::text(
            device_sync_strings::NO_SIZE_LIMIT,
        )));
        let row_copy = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
        row_copy.set_hexpand(true);
        row_copy.append(&balance);
        row_copy.append(&policy);
        row_copy.append(&review);
        let row_actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        row_actions.set_valign(gtk4::Align::Center);
        row_actions.append(&change_folder);
        row_actions.append(&set_limit);
        let inventory = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        inventory.append(&row_copy);
        inventory.append(&row_actions);

        let rules_title = label(
            &device_sync_strings::text(device_sync_strings::RULES_FOR_THIS_PHONE),
            "heading",
        );
        let remove_deleted = labeled_switch(
            &device_sync_strings::text(device_sync_strings::REMOVE_FROM_PHONE),
            {
                let set = actions.set_remove_deleted.clone();
                let updating = updating.clone();
                move |value| {
                    if !updating.get() {
                        set(value);
                    }
                }
            },
        );
        let sync_automatically = labeled_switch(
            &device_sync_strings::text(device_sync_strings::SYNC_AUTOMATICALLY),
            {
                let set = actions.set_sync_automatically.clone();
                let updating = updating.clone();
                move |value| {
                    if !updating.get() {
                        set(value);
                    }
                }
            },
        );

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 14);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.append(storage_bar.widget());
        content.prepend(&legacy_notice);
        content.append(&storage_legend);
        content.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        content.append(&inventory);
        content.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        content.append(&rules_title);
        content.append(&switch_row(&remove_deleted.0, &remove_deleted.1));
        content.append(&switch_row(&sync_automatically.0, &sync_automatically.1));
        let card = libadwaita::Bin::builder().child(&content).build();
        card.add_css_class("card");
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 9);
        root.append(&header);
        root.append(&card);

        Self {
            root,
            verification_title,
            legacy_notice,
            legacy_notice_pending,
            check_button,
            storage_bar,
            storage_legend,
            balance,
            policy,
            remove_deleted_switch: remove_deleted.1,
            sync_automatically_switch: sync_automatically.1,
            updating,
        }
    }

    pub(super) fn root(&self) -> &gtk4::Box {
        &self.root
    }

    #[cfg(test)]
    pub(super) fn check_button_is_sensitive(&self) -> bool {
        self.check_button.is_sensitive()
    }

    pub(super) fn update(&self, device: &DeviceView) {
        self.updating.set(true);
        let (verification, _detail, can_scan) =
            verification_copy(&device.contents_state, device.last_sync, chrono::Utc::now());
        self.verification_title.set_label(&verification);
        self.legacy_notice
            .set_title(&device_sync_strings::legacy_media_notice(
                &device.content_row.target_path,
            ));
        self.legacy_notice
            .set_revealed(self.legacy_notice_pending.get());
        self.check_button
            .set_sensitive(can_scan && device.connected && device.session_state.opens_session());
        self.storage_bar.update(&device.page.storage);
        self.storage_legend.set_label(&storage_legend(device));

        let selection =
            summarize_playlist_selection(&device.page.playlists, device.page.unique_track_count);
        let (tracks, size) = playlist_result_text(&device.content_row, &device.target_reading);
        self.balance.set_label(&device_sync_strings::device_balance(
            selection.selected,
            &tracks,
            &size,
        ));
        self.policy.set_label(&device_sync_strings::device_policy(
            &device.content_row.target_path,
            device.keep_smart_playlists_updated,
        ));
        set_switch(&self.remove_deleted_switch, device.settings.remove_deleted);
        set_switch(
            &self.sync_automatically_switch,
            device.settings.sync_automatically,
        );
        self.updating.set(false);
    }
}

pub(super) fn storage_legend(device: &DeviceView) -> String {
    if let StorageProjectionState::Insufficient { shortfall_bytes } = device.page.storage.state {
        let Some(free_bytes) = device.page.storage.current.free_bytes else {
            return device_sync_strings::text(device_sync_strings::SPACE_UNKNOWN);
        };
        return device_sync_strings::insufficient_storage(free_bytes, shortfall_bytes);
    }
    let Some(segments) = segments(&device.page.storage) else {
        return device_sync_strings::text(device_sync_strings::SPACE_UNKNOWN);
    };
    device_sync_strings::storage_legend(
        segments.music,
        segments.this_run,
        segments.other,
        segments.free,
    )
}

fn labeled_switch(
    label_text: &str,
    on_change: impl Fn(bool) + 'static,
) -> (gtk4::Label, gtk4::Switch) {
    let label = label(label_text, "");
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

fn switch_row(label: &gtk4::Label, switch: &gtk4::Switch) -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    row.append(label);
    row.append(switch);
    row
}

fn detail(text: &str) -> gtk4::Label {
    let label = label(text, "dim-label");
    label.set_wrap(true);
    label
}

fn label(text: &str, class: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_xalign(0.0);
    if !class.is_empty() {
        label.add_css_class(class);
    }
    label
}

fn set_switch(widget: &gtk4::Switch, active: bool) {
    widget.set_active(active);
    widget.set_state(active);
}
