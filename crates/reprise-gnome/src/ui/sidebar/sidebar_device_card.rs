//! Connected-device cards shown below the scrolling navigation rows.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use chrono::Utc;
use gtk4::prelude::*;
use reprise_core::device_sync::device_view::DeviceContentsState;

use super::sidebar_device_card_text;
#[cfg(test)]
use crate::ui::device_sync_runtime::SyncStep;
use crate::ui::device_sync_runtime::{DeviceView, PlannedSyncPhase};
use crate::ui::device_sync_strings;

#[path = "../device_sync/device_sync_card_menu.rs"]
pub(super) mod menu;

pub(super) type OpenCallback = Rc<dyn Fn(String, String)>;
pub(super) type CancelCallback = Rc<dyn Fn(String)>;

const CANCEL_BUTTON_SIZE: i32 = 28;
const CANCEL_BUTTON_MARGIN_END: i32 = 5;
const ACTIVE_SUFFIX_RESERVATION: i32 = CANCEL_BUTTON_SIZE + CANCEL_BUTTON_MARGIN_END + 1;
pub(super) const CARD_HORIZONTAL_MARGIN: i32 = 2;

/// Live card widgets, keyed by device id, so a state update can refresh them
/// in place. Rebuilding the section on every update destroyed the card
/// between a click's press and release — during a sync `notify` fires on
/// every progress callback, which made the card permanently unclickable —
/// and re-cloned every widget many times a second for nothing.
pub(super) type CardRegistry = Rc<RefCell<HashMap<String, DeviceCard>>>;

pub(super) struct DeviceCard {
    root: gtk4::Overlay,
    surface: gtk4::Button,
    cancel_button: gtk4::Button,
    indicator: gtk4::Stack,
    icon: gtk4::Image,
    spinner: gtk4::Spinner,
    name: gtk4::Label,
    detail_stack: gtk4::Stack,
    delta_detail: gtk4::Label,
    progress_detail: gtk4::Label,
    suffix_stack: gtk4::Stack,
    percent: gtk4::Label,
    progress_revealer: gtk4::Revealer,
    progress: gtk4::ProgressBar,
    progress_generation: Rc<Cell<u64>>,
    /// Read by the click gesture, which outlives any single state update.
    open_name: Rc<RefCell<String>>,
}

impl DeviceCard {
    /// The card's widget, so the section can place and order it.
    pub(super) fn root(&self) -> &gtk4::Overlay {
        &self.root
    }

    /// Both overlay siblings own the same local-memory context actions. The
    /// second target matters while Cancel occupies the card's top-right hit
    /// area or holds keyboard focus.
    pub(super) fn context_menu_targets(&self) -> [&gtk4::Button; 2] {
        [&self.surface, &self.cancel_button]
    }

    pub(super) fn new(
        device: &DeviceView,
        on_open: &OpenCallback,
        on_cancel: &CancelCallback,
    ) -> Self {
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
        content.set_valign(gtk4::Align::Center);
        let surface = gtk4::Button::builder()
            .child(&content)
            .has_frame(false)
            .hexpand(true)
            .build();
        surface.add_css_class("device-card");
        let root = gtk4::Overlay::new();
        root.set_child(Some(&surface));
        root.set_margin_bottom(3);
        root.set_margin_start(CARD_HORIZONTAL_MARGIN);
        root.set_margin_end(CARD_HORIZONTAL_MARGIN);

        let top = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        top.set_margin_top(9);
        top.set_margin_bottom(9);
        top.set_margin_start(9);
        top.set_margin_end(9);
        // Icon and spinner occupy the same slot: idle shows the device, a
        // running sync shows motion — the card morphs rather than being
        // rebuilt.
        let icon = gtk4::Image::from_gicon(&device.icon);
        icon.add_css_class("device-card-glyph");
        icon.set_pixel_size(32);
        let spinner = gtk4::Spinner::new();
        spinner.add_css_class("device-card-glyph");
        spinner.set_size_request(18, 18);
        spinner.set_valign(gtk4::Align::Center);
        let indicator = gtk4::Stack::builder()
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .transition_duration(crate::ui::motion::STANDARD_MS)
            .build();
        indicator.add_named(&icon, Some("device"));
        indicator.add_named(&spinner, Some("syncing"));
        indicator.set_visible_child_name("device");
        let icon_frame = gtk4::CenterBox::new();
        icon_frame.add_css_class("device-card-icon");
        icon_frame.set_size_request(48, 48);
        indicator.set_halign(gtk4::Align::Center);
        indicator.set_valign(gtk4::Align::Center);
        icon_frame.set_center_widget(Some(&indicator));

        let labels = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
        labels.set_hexpand(true);
        let name = gtk4::Label::new(None);
        name.add_css_class("heading");
        name.add_css_class("device-card-title");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        let detail_stack = gtk4::Stack::builder()
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .transition_duration(crate::ui::motion::STANDARD_MS)
            .build();
        let delta_detail = detail_label();
        let progress_detail = detail_label();
        detail_stack.add_named(&delta_detail, Some("delta"));
        detail_stack.add_named(&progress_detail, Some("progress"));
        detail_stack.set_visible_child_name("delta");
        labels.append(&name);
        labels.append(&detail_stack);
        top.append(&icon_frame);
        top.append(&labels);
        surface.update_property(&[gtk4::accessible::Property::Label(
            &device_sync_strings::open_device_label(&device.name),
        )]);

        // Fixed-width so the number cannot shove the title around as it grows
        // from "0 %" to "100 %".
        let percent = gtk4::Label::new(None);
        percent.add_css_class("device-card-percent");
        percent.set_width_chars(5);
        percent.set_xalign(1.0);
        let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
        chevron.add_css_class("dim-label");
        chevron.set_pixel_size(16);
        let suffix_stack = gtk4::Stack::builder()
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .transition_duration(crate::ui::motion::STANDARD_MS)
            .build();
        suffix_stack.set_hhomogeneous(false);
        suffix_stack.set_vhomogeneous(false);
        suffix_stack.add_named(&chevron, Some("open"));
        suffix_stack.add_named(&percent, Some("progress"));
        suffix_stack.set_visible_child_name("open");
        top.append(&suffix_stack);

        content.append(&top);

        // Inset rather than full-bleed: a full-width bar is clipped by the
        // card's own corner radius and reads as half-drawn.
        let progress = gtk4::ProgressBar::new();
        progress.add_css_class("device-card-progress");
        progress.update_property(&[gtk4::accessible::Property::Label(
            &device_sync_strings::text(device_sync_strings::SYNC_PROGRESS),
        )]);
        progress.set_margin_start(12);
        progress.set_margin_end(12);
        progress.set_margin_bottom(8);
        let progress_revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::Crossfade)
            .transition_duration(crate::ui::motion::STANDARD_MS)
            .child(&progress)
            .reveal_child(false)
            .build();
        progress_revealer.set_visible(false);
        let reset_progress = progress.clone();
        progress_revealer.connect_child_revealed_notify(move |revealer| {
            if !revealer.is_child_revealed() {
                reset_progress.set_fraction(0.0);
                revealer.set_visible(false);
            }
        });
        content.append(&progress_revealer);

        let cancel_button = gtk4::Button::builder()
            .icon_name(crate::ui::scan_card_css::SIDEBAR_CANCEL_ICON)
            .has_frame(false)
            .build();
        cancel_button.add_css_class("device-card-cancel");
        cancel_button.add_css_class("circular");
        cancel_button.set_size_request(CANCEL_BUTTON_SIZE, CANCEL_BUTTON_SIZE);
        cancel_button.set_halign(gtk4::Align::End);
        cancel_button.set_valign(gtk4::Align::Start);
        cancel_button.set_margin_top(5);
        cancel_button.set_margin_end(CANCEL_BUTTON_MARGIN_END);
        cancel_button.set_tooltip_text(Some(&device_sync_strings::text(
            device_sync_strings::CANCEL,
        )));
        cancel_button.update_property(&[gtk4::accessible::Property::Label(
            &device_sync_strings::text(device_sync_strings::CANCEL),
        )]);
        cancel_button.set_visible(false);
        root.add_overlay(&cancel_button);

        let cancel_callback = on_cancel.clone();
        let cancel_id = device.id.clone();
        cancel_button.connect_clicked(move |_| cancel_callback(cancel_id.clone()));

        // The whole card is one native keyboard and pointer target. The name
        // is read fresh because GVfs can replace a generic MTP label with the
        // real model name after the card was built.
        let open_name = Rc::new(RefCell::new(device.name.clone()));
        let open_callback = on_open.clone();
        let id = device.id.clone();
        let click_name = open_name.clone();
        surface.connect_clicked(move |_| {
            let name = click_name.borrow().clone();
            open_callback(id.clone(), name);
        });

        let card = Self {
            root,
            surface,
            cancel_button,
            indicator,
            icon,
            spinner,
            name,
            detail_stack,
            delta_detail,
            progress_detail,
            suffix_stack,
            percent,
            progress_revealer,
            progress,
            progress_generation: Rc::new(Cell::new(0)),
            open_name,
        };
        card.update(device);
        card
    }

    pub(super) fn update(&self, device: &DeviceView) {
        for class in [
            "device-card-active",
            "device-card-connected",
            "device-card-remembered",
        ] {
            self.surface.remove_css_class(class);
        }
        let emphasis_class = match sidebar_device_card_text::card_emphasis(device) {
            sidebar_device_card_text::CardEmphasis::Active => "device-card-active",
            sidebar_device_card_text::CardEmphasis::Connected => "device-card-connected",
            sidebar_device_card_text::CardEmphasis::Remembered => "device-card-remembered",
        };
        self.surface.add_css_class(emphasis_class);
        let active = matches!(
            sidebar_device_card_text::card_emphasis(device),
            sidebar_device_card_text::CardEmphasis::Active
        );
        if !active && self.cancel_button.has_focus() {
            self.surface.grab_focus();
        }
        self.cancel_button.set_visible(active);
        // The overlay button owns the top-right corner. Reserve that corner
        // only while it exists so the fixed-width percentage never sits
        // beneath it and idle chevrons retain their normal alignment.
        self.suffix_stack
            .set_margin_end(if active { ACTIVE_SUFFIX_RESERVATION } else { 0 });
        self.name.set_text(&card_title(device));
        self.surface
            .update_property(&[gtk4::accessible::Property::Label(
                &device_sync_strings::open_device_label(&device.name),
            )]);
        *self.open_name.borrow_mut() = device.name.clone();

        let syncing = sidebar_device_card_text::is_syncing(device);
        // GtkSpinner follows the toolkit's system animation behavior; only
        // Reprise's custom progress tick needs the explicit MOT-7 gate.
        self.spinner.set_spinning(syncing);
        self.icon.set_from_gicon(&device.icon);
        self.indicator
            .set_visible_child_name(if syncing { "syncing" } else { "device" });

        match detail_mode(device) {
            DetailMode::Delta => {
                self.delta_detail.set_text(&card_subtitle(device));
                if matches!(
                    device.session_state,
                    reprise_core::device_sync::DeviceSessionState::Inert { .. }
                ) || !device.rememberable
                {
                    self.delta_detail.add_css_class("warning");
                } else {
                    self.delta_detail.remove_css_class("warning");
                }
                self.detail_stack.set_visible_child_name("delta");
            }
            DetailMode::Progress => {
                let detail = match &device.sync_phase {
                    PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing => {
                        sidebar_device_card_text::syncing_file_count(device)
                            .unwrap_or_else(|| card_subtitle(device))
                    }
                    _ => card_subtitle(device),
                };
                self.progress_detail.set_text(&detail);
                self.detail_stack.set_visible_child_name("progress");
            }
        }

        match &device.sync_phase {
            PlannedSyncPhase::Syncing {
                done,
                total,
                current_track,
                bytes_done,
                bytes_total,
                ..
            } => {
                self.suffix_stack.set_visible_child_name("progress");
                self.percent.set_text(&device_sync_strings::sync_percent(
                    *bytes_done,
                    *bytes_total,
                ));
                self.progress_revealer.set_visible(true);
                self.progress_revealer.set_reveal_child(true);
                self.animate_progress(sync_fraction(*bytes_done, *bytes_total));
                self.surface
                    .set_tooltip_text(Some(&device_sync_strings::sync_tooltip(
                        *done,
                        *total,
                        *bytes_done,
                        *bytes_total,
                        current_track,
                    )));
            }
            PlannedSyncPhase::Finishing => {
                self.suffix_stack.set_visible_child_name("progress");
                self.percent.set_text("100 %");
                self.progress_revealer.set_visible(true);
                self.progress_revealer.set_reveal_child(true);
                self.animate_progress(1.0);
                self.surface
                    .set_tooltip_text(Some("Finishing synchronization"));
            }
            _ => {
                self.progress_generation
                    .set(self.progress_generation.get().saturating_add(1));
                self.suffix_stack.set_visible_child_name("open");
                self.progress_revealer.set_reveal_child(false);
                self.surface
                    .set_tooltip_text(idle_tooltip(device).as_deref());
            }
        }
    }

    fn animate_progress(&self, target: f64) {
        let generation = self.progress_generation.get().saturating_add(1);
        self.progress_generation.set(generation);
        if !crate::ui::motion::animations_enabled() {
            self.progress.set_fraction(target);
            return;
        }
        let start = self.progress.fraction();
        if (start - target).abs() < f64::EPSILON {
            return;
        }
        let progress = self.progress.clone();
        let current_generation = self.progress_generation.clone();
        let started_at = Cell::new(None);
        progress.clone().add_tick_callback(move |_, frame_clock| {
            if current_generation.get() != generation {
                return gtk4::glib::ControlFlow::Break;
            }
            if !crate::ui::motion::animations_enabled() {
                progress.set_fraction(target);
                return gtk4::glib::ControlFlow::Break;
            }
            let start_time = started_at.get().unwrap_or_else(|| {
                let start_time = frame_clock.frame_time();
                started_at.set(Some(start_time));
                start_time
            });
            let elapsed = frame_clock.frame_time().saturating_sub(start_time) as f64;
            let duration_us = f64::from(crate::ui::motion::MICRO_MS) * 1_000.0;
            let linear = (elapsed / duration_us).clamp(0.0, 1.0);
            let eased = 1.0 - (1.0 - linear).powi(3);
            progress.set_fraction(start + (target - start) * eased);
            if linear >= 1.0 {
                gtk4::glib::ControlFlow::Break
            } else {
                gtk4::glib::ControlFlow::Continue
            }
        });
    }
}

fn detail_label() -> gtk4::Label {
    let label = gtk4::Label::new(None);
    label.add_css_class("device-card-detail");
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    label.set_xalign(0.0);
    label
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DetailMode {
    Delta,
    Progress,
}

fn detail_mode(device: &DeviceView) -> DetailMode {
    match &device.sync_phase {
        PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing => DetailMode::Progress,
        PlannedSyncPhase::Idle | PlannedSyncPhase::ComputingDelta => DetailMode::Delta,
    }
}

/// Real problems only — a plain `NoPlaylistsSelected` blocker is not one of
/// them (design 7c's four leading-sentence states already cover "nothing
/// selected" honestly via `Up to date`/`Tap to scan device contents`, so it
/// must not also trip a competing "Needs attention" reading).
fn mirror_needs_attention(device: &DeviceView) -> bool {
    sidebar_device_card_text::mirror_needs_attention(device)
}

/// `MTP-29`: "The card carries only the leading sentence; the full balance
/// goes in the tooltip." Only meaningful once the card is idle, verified,
/// and has real pending work — otherwise the leading sentence already says
/// everything the tooltip would.
fn idle_tooltip(device: &DeviceView) -> Option<String> {
    if !device.session_state.shows_diff() {
        return None;
    }
    if device.sync_phase != PlannedSyncPhase::Idle || mirror_needs_attention(device) {
        return None;
    }
    if device.contents_state != DeviceContentsState::Verified {
        return None;
    }
    let balance = reprise_core::device_sync::aggregate_balance(&[device.target_reading]);
    balance
        .has_work()
        .then(|| sidebar_device_card_text::tooltip_text(&balance))
}

fn sync_fraction(bytes_done: u64, bytes_total: u64) -> f64 {
    if bytes_total == 0 {
        0.0
    } else {
        (bytes_done as f64 / bytes_total as f64).clamp(0.0, 1.0)
    }
}

/// Styling for the sidebar device card. Colours come from the theme
/// (`@accent_color` is the palette's petrol), never a literal, so every named
/// dark theme keeps its own accent.
pub(in crate::ui) fn css() -> String {
    sidebar_device_card_text::css()
}

fn card_title(device: &DeviceView) -> String {
    if sidebar_device_card_text::is_syncing(device) {
        format!("Syncing {}", device.name)
    } else {
        device.name.clone()
    }
}

fn card_subtitle(device: &DeviceView) -> String {
    sidebar_device_card_text::card_subtitle(device, Utc::now())
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub(in crate::ui::sidebar) fn view(phase: PlannedSyncPhase) -> DeviceView {
        DeviceView {
            id: "pixel".into(),
            name: "Pixel 8".into(),
            icon: gtk4::gio::ThemedIcon::new("phone-symbolic").upcast(),
            connected: true,
            rememberable: true,
            memory_status: None,
            session_state: reprise_core::device_sync::DeviceSessionState::Active,
            storage: Default::default(),
            scan_error: None,
            settings: reprise_core::device_sync::DeviceSettings {
                device_serial: "pixel".into(),
                device_name: "Pixel 8".into(),
                selection: reprise_core::device_sync::DeviceSelection::EntireLibrary,
                profile: reprise_core::device_sync::TransferProfile::default(),
                opus_bitrate: 0,
                remove_deleted: true,
                sync_automatically: true,
            },
            sync_phase: phase,
            sync_error: None,
            last_sync: None,
            verified_managed_track_count: None,
            size_on_device_bytes: None,
            managed_track_count: 0,
            bytes_per_second: 0,
            contents_state: reprise_core::device_sync::device_view::DeviceContentsState::Verified,
            content_row: crate::ui::device_sync_runtime::empty_content_row(),
            target_reading: crate::ui::device_sync_runtime::empty_target_reading(),
            keep_smart_playlists_updated: true,
            page: Default::default(),
        }
    }
}

#[cfg(test)]
#[path = "sidebar_device_card_mirror_tests.rs"]
mod mirror_tests;
