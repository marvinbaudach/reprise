//! The fixed synchronization reading below the scrollable device page.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::device_sync::{
    aggregate_balance, DeviceSessionState, DeviceStorageAccess, PlannedSyncPhase, SyncStep,
};

use super::device_sync_page_copy::{
    blocker_summary, device_last_sync_copy, profile_label, warning_summary,
};
use super::device_sync_runtime::DeviceView;
use super::device_sync_storage_copy::storage_access_notice;
use super::device_sync_strings;

/// `MTP-60`: the docked sync bar. One widget, four live readings and the
/// remembered placeholder that Plan E will complete.
pub(super) struct DeviceSyncDock {
    root: adw::Bin,
    pub(super) title: gtk4::Label,
    pub(super) detail: gtk4::Label,
    pub(super) metrics: gtk4::Label,
    pub(super) progress: gtk4::ProgressBar,
    pub(super) primary: gtk4::Button,
    cancelling: Rc<Cell<bool>>,
}

/// What the bar reads right now. It is derived from the current device view
/// and is never stored separately from that source of truth.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum DockReading {
    Idle {
        summary: String,
        can_start: bool,
    },
    Running {
        step: SyncStep,
        copied: usize,
        total: usize,
        current_track: Option<String>,
        bytes_per_second: u64,
        remaining: Option<Duration>,
        fraction: f64,
    },
    Finishing {
        summary: String,
    },
    Failed {
        message: String,
        can_start: bool,
    },
    Remembered {
        summary: String,
        auto_sync: bool,
    },
}

impl DockReading {
    pub(super) fn for_device(device: &DeviceView) -> Self {
        if device.session_state == DeviceSessionState::Remembered {
            return Self::Remembered {
                summary: device_sync_strings::text(device_sync_strings::NOT_CONNECTED),
                auto_sync: device.settings.sync_automatically,
            };
        }
        if let Some(error) = &device.sync_error {
            return Self::Failed {
                message: error.message.clone(),
                can_start: device.page.controls.can_start,
            };
        }
        match &device.sync_phase {
            PlannedSyncPhase::Syncing {
                step,
                done,
                total,
                current_track,
                unit_bytes_done,
                unit_bytes_total,
                ..
            } => Self::Running {
                step: *step,
                copied: *done as usize,
                total: *total as usize,
                current_track: (!current_track.is_empty()).then(|| current_track.clone()),
                bytes_per_second: device.bytes_per_second,
                remaining: device.estimated_remaining,
                fraction: fraction(*done, *total, *unit_bytes_done, *unit_bytes_total),
            },
            PlannedSyncPhase::Finishing => Self::Finishing {
                summary: device_sync_strings::text(device_sync_strings::FINISHING_SYNC),
            },
            PlannedSyncPhase::ComputingDelta => Self::Idle {
                summary: device_sync_strings::text(device_sync_strings::CHECKING_CHANGES),
                can_start: false,
            },
            PlannedSyncPhase::Idle => Self::Idle {
                summary: idle_summary(device),
                can_start: device.page.controls.can_start,
            },
        }
    }
}

impl DeviceSyncDock {
    pub(super) fn new() -> Self {
        let title = label("", "heading");
        title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        title.set_width_chars(40);
        title.set_max_width_chars(40);
        let detail = label("", "dim-label");
        detail.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        detail.set_width_chars(32);
        detail.set_max_width_chars(32);
        let metrics = label("", "dim-label");
        metrics.add_css_class("numeric");
        metrics.set_xalign(1.0);
        metrics.set_width_chars(24);
        metrics.set_max_width_chars(24);

        let copy = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
        copy.set_hexpand(true);
        copy.append(&title);
        copy.append(&detail);
        let primary = gtk4::Button::with_mnemonic(&device_sync_strings::text(
            device_sync_strings::SYNC_NOW_MNEMONIC,
        ));
        primary.add_css_class("suggested-action");
        primary.set_valign(gtk4::Align::Center);
        primary.set_width_request(112);
        let top = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        top.append(&copy);
        top.append(&metrics);
        top.append(&primary);

        let progress = gtk4::ProgressBar::new();
        progress.set_show_text(false);
        progress.update_property(&[gtk4::accessible::Property::Label(
            &device_sync_strings::text(device_sync_strings::SYNC_PROGRESS),
        )]);
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 7);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.append(&top);
        content.append(&progress);
        let root = adw::Bin::builder().child(&content).build();
        root.add_css_class("card");

        Self {
            root,
            title,
            detail,
            metrics,
            progress,
            primary,
            cancelling: Rc::new(Cell::new(false)),
        }
    }

    pub(super) fn root(&self) -> &adw::Bin {
        &self.root
    }

    pub(super) fn connect_actions(&self, start: Rc<dyn Fn()>, cancel: Rc<dyn Fn()>) {
        let cancelling = self.cancelling.clone();
        self.primary.connect_clicked(move |_| {
            if cancelling.get() {
                cancel();
            } else {
                start();
            }
        });
    }

    pub(super) fn update(&self, device: &DeviceView) {
        let reading = DockReading::for_device(device);
        self.cancelling
            .set(matches!(reading, DockReading::Running { .. }));
        self.primary.remove_css_class("suggested-action");
        self.primary.remove_css_class("destructive-action");
        self.title.remove_css_class("error");
        self.title.remove_css_class("warning");
        match reading {
            DockReading::Idle { summary, can_start } => {
                self.title.set_label(&summary);
                self.detail.set_label(&format!(
                    "{} · {}",
                    profile_label(device.page.profile),
                    device_last_sync_copy(device)
                ));
                self.metrics.set_label("");
                self.progress.set_visible(false);
                self.set_sync_action(can_start);
            }
            DockReading::Running {
                step,
                copied,
                total,
                current_track,
                bytes_per_second,
                remaining,
                fraction,
            } => {
                self.title
                    .set_label(&device_sync_strings::syncing_files(copied, total));
                self.detail.set_label(&device_sync_strings::sync_activity(
                    device_sync_strings::step_glyph(&step),
                    current_track.as_deref().unwrap_or(""),
                ));
                self.metrics
                    .set_label(&device_sync_strings::rate_and_remaining(
                        bytes_per_second,
                        remaining,
                    ));
                self.progress.set_visible(true);
                self.progress.set_fraction(fraction);
                self.primary.set_visible(true);
                self.primary.set_sensitive(device.page.controls.can_cancel);
                self.primary.set_label(&device_sync_strings::text(
                    device_sync_strings::CANCEL_MNEMONIC,
                ));
                self.primary.add_css_class("destructive-action");
            }
            DockReading::Finishing { summary } => {
                self.title.set_label(&summary);
                self.detail.set_label("");
                self.metrics.set_label("");
                self.progress.set_visible(true);
                self.progress.pulse();
                self.primary.set_visible(false);
            }
            DockReading::Failed { message, can_start } => {
                self.title.set_label(&message);
                self.detail.set_label("");
                self.metrics.set_label("");
                self.progress.set_visible(false);
                self.set_sync_action(can_start);
            }
            DockReading::Remembered { summary, auto_sync } => {
                self.title.set_label(&summary);
                self.detail
                    .set_label(&device_sync_strings::remembered_auto_sync(auto_sync));
                self.metrics.set_label("");
                self.progress.set_visible(false);
                self.primary.set_visible(false);
            }
        }
        let notices = notice_messages(device);
        self.title
            .set_tooltip_text((!notices.is_empty()).then(|| notices.join("\n")).as_deref());
        if has_error_notice(device) {
            self.title.add_css_class("error");
        } else if !device.page.warnings.is_empty() {
            self.title.add_css_class("warning");
        }
    }

    fn set_sync_action(&self, sensitive: bool) {
        self.primary.set_visible(true);
        self.primary.set_sensitive(sensitive);
        self.primary.set_label(&device_sync_strings::text(
            device_sync_strings::SYNC_NOW_MNEMONIC,
        ));
        self.primary.add_css_class("suggested-action");
    }
}

fn idle_summary(device: &DeviceView) -> String {
    let notices = notice_messages(device);
    if !notices.is_empty() {
        return notices.join(" · ");
    }
    let balance = aggregate_balance(std::slice::from_ref(&device.target_reading));
    device_sync_strings::ready_to_sync(&device_sync_strings::balance_text(&balance))
}

fn notice_messages(device: &DeviceView) -> Vec<String> {
    let mut notices = Vec::new();
    if let Some(blocker) = blocker_summary(&device.page.blockers) {
        notices.push(blocker);
    }
    if let Some(access) = storage_access_notice(device.page.storage.access) {
        notices.push(access);
    }
    notices.extend(warning_summary(&device.page.warnings));
    if let Some(error) = &device.scan_error {
        notices.push(device_sync_strings::inspection_failed(error));
    }
    if let Some(error) = &device.sync_error {
        notices.push(error.message.clone());
    }
    notices
}

fn has_error_notice(device: &DeviceView) -> bool {
    !device.page.blockers.is_empty()
        || device.page.storage.access == DeviceStorageAccess::ReadOnly
        || device.scan_error.is_some()
        || device.sync_error.is_some()
}

fn fraction(done: u32, total: u32, unit_bytes_done: u64, unit_bytes_total: u64) -> f64 {
    let unit_fraction = if unit_bytes_total > 0 {
        unit_bytes_done as f64 / unit_bytes_total as f64
    } else {
        0.0
    };
    let value = if total > 0 {
        (f64::from(done) + unit_fraction) / f64::from(total)
    } else {
        0.0
    };
    value.clamp(0.0, 1.0)
}

fn label(text: &str, class: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class(class);
    label
}
