//! Connected-device cards shown below the scrolling navigation rows.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;

use crate::ui::device_sync_runtime::{
    DeviceSyncRuntime, DeviceSyncState, DeviceView, PlannedSyncPhase, SyncStep,
};
use crate::ui::device_sync_strings;

type OpenCallback = Rc<dyn Fn(String, String)>;

/// Live card widgets, keyed by device id, so a state update can refresh them
/// in place. Rebuilding the section on every update destroyed the card
/// between a click's press and release — during a sync `notify` fires on
/// every progress callback, which made the card permanently unclickable —
/// and re-cloned every widget many times a second for nothing.
type CardRegistry = Rc<RefCell<HashMap<String, DeviceCard>>>;

struct DeviceCard {
    root: gtk4::Button,
    indicator: gtk4::Stack,
    icon: gtk4::Image,
    spinner: gtk4::Spinner,
    name: gtk4::Label,
    detail_stack: gtk4::Stack,
    delta_detail: gtk4::Label,
    progress_detail: gtk4::Label,
    synced_detail: gtk4::Label,
    suffix_stack: gtk4::Stack,
    percent: gtk4::Label,
    progress_revealer: gtk4::Revealer,
    progress: gtk4::ProgressBar,
    progress_generation: Rc<Cell<u64>>,
    /// Read by the click gesture, which outlives any single state update.
    open_name: Rc<RefCell<String>>,
}

pub(super) fn bind(runtime: &Rc<DeviceSyncRuntime>, on_open: OpenCallback) -> gtk4::Box {
    let section = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    section.set_margin_start(10);
    section.set_margin_end(10);
    section.set_margin_top(8);
    section.set_margin_bottom(8);
    section.set_visible(false);
    let heading = gtk4::Label::new(Some("DEVICES"));
    heading.add_css_class("caption");
    heading.add_css_class("dim-label");
    heading.set_xalign(0.0);
    heading.set_margin_start(8);
    section.append(&heading);

    let cards: CardRegistry = Rc::new(RefCell::new(HashMap::new()));
    let subscription = runtime.subscribe(Rc::new({
        let section = section.clone();
        let cards = cards.clone();
        move |state| render(&section, &cards, &state, &on_open)
    }));
    subscription.retain_for_widget(&section);
    section
}

fn render(
    section: &gtk4::Box,
    cards: &CardRegistry,
    state: &DeviceSyncState,
    on_open: &OpenCallback,
) {
    let devices = state
        .devices
        .iter()
        .filter(|device| device.connected)
        .collect::<Vec<_>>();
    section.set_visible(!devices.is_empty());

    let mut registry = cards.borrow_mut();
    // Drop cards for devices that went away.
    registry.retain(|id, card| {
        let keep = devices.iter().any(|device| &device.id == id);
        if !keep {
            section.remove(&card.root);
        }
        keep
    });
    // Update in place, appending only genuinely new devices.
    for device in devices {
        match registry.get(&device.id) {
            Some(card) => card.update(device),
            None => {
                let card = DeviceCard::new(device, on_open);
                section.append(&card.root);
                card.update(device);
                registry.insert(device.id.clone(), card);
            }
        }
    }
}

impl DeviceCard {
    fn new(device: &DeviceView, on_open: &OpenCallback) -> Self {
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
        content.set_valign(gtk4::Align::Center);
        let root = gtk4::Button::builder()
            .child(&content)
            .has_frame(false)
            .hexpand(true)
            .build();
        root.add_css_class("device-card");
        root.set_margin_bottom(3);
        root.set_margin_start(2);
        root.set_margin_end(2);

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
        let synced_detail = detail_label();
        detail_stack.add_named(&delta_detail, Some("delta"));
        detail_stack.add_named(&progress_detail, Some("progress"));
        detail_stack.add_named(&synced_detail, Some("synced"));
        detail_stack.set_visible_child_name("delta");
        labels.append(&name);
        labels.append(&detail_stack);
        top.append(&icon_frame);
        top.append(&labels);
        root.update_property(&[gtk4::accessible::Property::Label(
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

        // The whole card is one native keyboard and pointer target. The name
        // is read fresh because GVfs can replace a generic MTP label with the
        // real model name after the card was built.
        let open_name = Rc::new(RefCell::new(device.name.clone()));
        let open_callback = on_open.clone();
        let id = device.id.clone();
        let click_name = open_name.clone();
        root.connect_clicked(move |_| {
            let name = click_name.borrow().clone();
            open_callback(id.clone(), name);
        });

        Self {
            root,
            indicator,
            icon,
            spinner,
            name,
            detail_stack,
            delta_detail,
            progress_detail,
            synced_detail,
            suffix_stack,
            percent,
            progress_revealer,
            progress,
            progress_generation: Rc::new(Cell::new(0)),
            open_name,
        }
    }

    fn update(&self, device: &DeviceView) {
        self.name.set_text(&card_title(device));
        self.root
            .update_property(&[gtk4::accessible::Property::Label(
                &device_sync_strings::open_device_label(&device.name),
            )]);
        *self.open_name.borrow_mut() = device.name.clone();

        let syncing = matches!(
            device.sync_phase,
            PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing
        );
        // GtkSpinner follows the toolkit's system animation behavior; only
        // Reprise's custom progress tick needs the explicit MOT-7 gate.
        self.spinner.set_spinning(syncing);
        self.icon.set_from_gicon(&device.icon);
        self.indicator
            .set_visible_child_name(if syncing { "syncing" } else { "device" });

        let has_selection = matches!(
            &device.settings.selection,
            reprise_core::device_sync::DeviceSelection::EntireLibrary
        ) || matches!(
            &device.settings.selection,
            reprise_core::device_sync::DeviceSelection::Sources(sources) if !sources.is_empty()
        );
        match detail_mode(&device.sync_phase, device.delta.as_ref(), has_selection) {
            DetailMode::Delta => {
                self.delta_detail.set_text(&card_subtitle(device));
                self.detail_stack.set_visible_child_name("delta");
            }
            DetailMode::Progress => {
                self.progress_detail.set_text(&card_subtitle(device));
                self.detail_stack.set_visible_child_name("progress");
            }
            DetailMode::Synced => {
                self.synced_detail.set_text(&format!(
                    "Synced ✓ · {}",
                    device_sync_strings::free_space(device.storage.free_bytes)
                ));
                self.detail_stack.set_visible_child_name("synced");
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
                self.root
                    .set_tooltip_text(Some(&device_sync_strings::sync_tooltip(
                        *done,
                        *total,
                        *bytes_done,
                        *bytes_total,
                        current_track,
                    )));
            }
            _ => {
                self.progress_generation
                    .set(self.progress_generation.get().saturating_add(1));
                self.suffix_stack.set_visible_child_name("open");
                self.progress_revealer.set_reveal_child(false);
                self.root.set_tooltip_text(None);
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
    Synced,
}

fn detail_mode(
    phase: &PlannedSyncPhase,
    delta: Option<&reprise_core::device_sync::SyncDelta>,
    has_selection: bool,
) -> DetailMode {
    match phase {
        PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing => DetailMode::Progress,
        PlannedSyncPhase::Idle if has_selection && delta.is_some_and(|delta| !has_delta(delta)) => {
            DetailMode::Synced
        }
        PlannedSyncPhase::Idle | PlannedSyncPhase::ComputingDelta => DetailMode::Delta,
    }
}

fn has_delta(delta: &reprise_core::device_sync::SyncDelta) -> bool {
    !delta.to_copy.is_empty() || !delta.to_remove.is_empty()
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
    ".device-card { min-height: 0; padding: 0; border-radius: 14px; \
       border: 1px solid alpha(@window_fg_color, 0.07); \
       background-color: alpha(@window_fg_color, 0.035); }\n\
     .device-card:hover { background-color: alpha(@window_fg_color, 0.065); }\n\
     .device-card:focus-visible { box-shadow: inset 0 0 0 2px \
       alpha(@window_fg_color, 0.32); }\n\
     .device-card-icon { border-radius: 13px; \
       background-color: alpha(@window_fg_color, 0.075); }\n\
     .device-card-glyph { color: alpha(@window_fg_color, 0.82); }\n\
     .device-card-title { font-size: 13.5px; }\n\
     .device-card-detail { font-size: 11.5px; color: alpha(@window_fg_color, 0.55); }\n\
     .device-card-percent { font-size: 11.5px; font-feature-settings: \"tnum\"; \
       color: alpha(@window_fg_color, 0.45); }\n\
     .device-card-progress { min-height: 3px; }\n\
     .device-card-progress trough { min-height: 3px; border-radius: 2px; \
       background-color: alpha(#ffffff, 0.12); }\n\
     .device-card-progress progress { min-height: 3px; border-radius: 2px; \
       background-color: @accent_color; }"
        .to_string()
}

fn card_title(device: &DeviceView) -> String {
    if matches!(
        device.sync_phase,
        PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing
    ) {
        format!("Syncing {}", device.name)
    } else {
        device.name.clone()
    }
}

fn card_subtitle(device: &DeviceView) -> String {
    match &device.sync_phase {
        PlannedSyncPhase::ComputingDelta => "Checking…".into(),
        // The percentage lives in its own fixed-width label; keeping it out of
        // here stops the track name from shifting the number around.
        PlannedSyncPhase::Syncing {
            step,
            current_track,
            ..
        } => device_sync_strings::sync_activity(step_glyph(step), current_track),
        PlannedSyncPhase::Finishing => "Finishing…".into(),
        PlannedSyncPhase::Idle => {
            let queued = device.delta.as_ref().map_or(0, |delta| delta.to_copy.len());
            format!(
                "{queued} queued · {}",
                device_sync_strings::available_space(device.storage.free_bytes)
            )
        }
    }
}

/// What is happening to the named track.
fn step_glyph(step: &SyncStep) -> &'static str {
    match step {
        SyncStep::Transcoding => "⟳ transcoding ·",
        SyncStep::Copying => "↑",
        SyncStep::Removing => "−",
        SyncStep::WritingPlaylists => "≡",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(phase: PlannedSyncPhase) -> DeviceView {
        DeviceView {
            id: "pixel".into(),
            name: "Pixel 8".into(),
            icon: gtk4::gio::ThemedIcon::new("phone-symbolic").upcast(),
            connected: true,
            storage: Default::default(),
            scanning: false,
            scan_error: None,
            draft_playlists: Vec::new(),
            last_enqueue: None,
            snapshot: reprise_core::device_sync::DeviceQueue::new().snapshot(),
            settings: reprise_core::device_sync::DeviceSettings {
                device_serial: "pixel".into(),
                device_name: "Pixel 8".into(),
                selection: reprise_core::device_sync::DeviceSelection::EntireLibrary,
                profile: reprise_core::device_sync::TransferProfile::default(),
                opus_bitrate: 0,
                ratings_back: false,
                remove_deleted: true,
            },
            delta: Some(reprise_core::device_sync::SyncDelta::default()),
            sync_phase: phase,
            sync_error: None,
            last_sync: None,
            tracks: Vec::new(),
            selected_track_count: 0,
            bytes_per_second: 0,
            page: Default::default(),
        }
    }

    #[test]
    fn card_detail_mode_distinguishes_delta_progress_and_synced_states() {
        let pending = reprise_core::device_sync::SyncDelta {
            to_copy: vec![1],
            ..Default::default()
        };
        let empty = reprise_core::device_sync::SyncDelta::default();

        assert_eq!(
            detail_mode(&PlannedSyncPhase::Idle, Some(&pending), true),
            DetailMode::Delta
        );
        assert_eq!(
            detail_mode(
                &PlannedSyncPhase::Syncing {
                    step: SyncStep::Copying,
                    done: 0,
                    total: 1,
                    current_track: "Track".into(),
                    bytes_done: 0,
                    bytes_total: 1,
                },
                Some(&pending),
                true,
            ),
            DetailMode::Progress
        );
        assert_eq!(
            detail_mode(&PlannedSyncPhase::Idle, Some(&empty), true),
            DetailMode::Synced
        );
        assert_eq!(
            detail_mode(&PlannedSyncPhase::Idle, Some(&empty), false),
            DetailMode::Delta
        );
    }

    #[test]
    fn byte_progress_fraction_is_bounded_and_handles_an_unknown_total() {
        assert_eq!(sync_fraction(50, 100), 0.5);
        assert_eq!(sync_fraction(150, 100), 1.0);
        assert_eq!(sync_fraction(50, 0), 0.0);
    }

    #[test]
    fn card_activity_distinguishes_transcoding_and_copying_with_artist() {
        let track = "Immortal — Lorna Shore";

        assert_eq!(
            device_sync_strings::sync_activity(step_glyph(&SyncStep::Transcoding), track),
            "⟳ transcoding · Immortal — Lorna Shore"
        );
        assert_eq!(
            device_sync_strings::sync_activity(step_glyph(&SyncStep::Copying), track),
            "↑ Immortal — Lorna Shore"
        );
    }

    #[test]
    fn syncing_title_is_explicit() {
        assert_eq!(
            card_title(&view(PlannedSyncPhase::Finishing)),
            "Syncing Pixel 8"
        );
    }

    #[test]
    fn sidebar_device_card_has_no_direct_sync_action() {
        let direct_sync_action = ["app", "sync-device"].join(".");

        assert!(!include_str!("sidebar_device_card.rs").contains(&direct_sync_action));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn device_card_open_is_a_native_keyboard_action() {
        gtk4::init().unwrap();
        let opened = Rc::new(RefCell::new(None));
        let opened_for_callback = opened.clone();
        let on_open: OpenCallback = Rc::new(move |id, name| {
            opened_for_callback.borrow_mut().replace((id, name));
        });
        let card = DeviceCard::new(&view(PlannedSyncPhase::Idle), &on_open);
        assert!(card.root.is_focusable());
        card.root.emit_clicked();
        assert_eq!(
            opened.borrow().as_ref(),
            Some(&("pixel".to_owned(), "Pixel 8".to_owned()))
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mot_7_disabled_animations_apply_progress_and_state_changes_immediately() {
        if gtk4::init().is_err() {
            return;
        }
        let settings = gtk4::Settings::default().unwrap();
        let previous = settings.is_gtk_enable_animations();
        settings.set_gtk_enable_animations(false);
        let device = view(PlannedSyncPhase::Syncing {
            step: SyncStep::Copying,
            done: 0,
            total: 1,
            current_track: "Track".into(),
            bytes_done: 50,
            bytes_total: 100,
        });
        let on_open: OpenCallback = Rc::new(|_, _| {});
        let card = DeviceCard::new(&device, &on_open);

        card.update(&device);

        assert_eq!(card.progress.fraction(), 0.5);
        assert_eq!(
            card.detail_stack.transition_duration(),
            crate::ui::motion::STANDARD_MS
        );
        assert_eq!(
            card.detail_stack.visible_child_name().as_deref(),
            Some("progress")
        );
        assert_eq!(
            card.indicator.visible_child_name().as_deref(),
            Some("syncing")
        );
        assert!(card.spinner.is_spinning());
        assert!(card.progress_revealer.reveals_child());
        settings.set_gtk_enable_animations(previous);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn enabled_animations_interpolate_progress_to_the_latest_fraction() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        if gtk4::init().is_err() {
            return;
        }
        let settings = gtk4::Settings::default().unwrap();
        let previous = settings.is_gtk_enable_animations();
        settings.set_gtk_enable_animations(true);
        let idle = view(PlannedSyncPhase::Idle);
        let on_open: OpenCallback = Rc::new(|_, _| {});
        let card = DeviceCard::new(&idle, &on_open);
        assert!(card.root.settings().is_gtk_enable_animations());
        let window = gtk4::Window::new();
        window.set_child(Some(&card.root));
        window.present();
        gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
            std::time::Duration::from_millis(20),
        ));
        let syncing = view(PlannedSyncPhase::Syncing {
            step: SyncStep::Copying,
            done: 0,
            total: 1,
            current_track: "Track".into(),
            bytes_done: 50,
            bytes_total: 100,
        });

        card.update(&syncing);

        assert!(card.progress.fraction() < 0.5);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while (card.progress.fraction() - 0.5).abs() >= 1e-6 && std::time::Instant::now() < deadline
        {
            gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
                std::time::Duration::from_millis(20),
            ));
        }
        assert!((card.progress.fraction() - 0.5).abs() < 1e-6);
        window.close();
        settings.set_gtk_enable_animations(previous);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mot_2_device_background_surfaces_only_crossfade_in_place() {
        gtk4::init().unwrap();
        let device = view(PlannedSyncPhase::Idle);
        let on_open: OpenCallback = Rc::new(|_, _| {});
        let card = DeviceCard::new(&device, &on_open);

        assert_eq!(
            card.indicator.transition_type(),
            gtk4::StackTransitionType::Crossfade
        );
        assert_eq!(
            card.detail_stack.transition_type(),
            gtk4::StackTransitionType::Crossfade
        );
        assert_eq!(
            card.suffix_stack.transition_type(),
            gtk4::StackTransitionType::Crossfade
        );
        assert_eq!(
            card.progress_revealer.transition_type(),
            gtk4::RevealerTransitionType::Crossfade
        );
    }
}

#[cfg(test)]
mod css_tests {
    #[test]
    fn css_covers_the_sync_card_vocabulary() {
        let css = super::css();
        for marker in [
            ".device-card {",
            ".device-card:hover",
            ".device-card:focus-visible",
            ".device-card-icon",
            ".device-card-glyph",
            ".device-card-detail",
            ".device-card-percent",
            ".device-card-progress trough",
            ".device-card-progress progress",
        ] {
            assert!(css.contains(marker), "missing rule: {marker}");
        }
        assert!(
            !css.contains("#1CA98F"),
            "the accent must come from the theme, not a literal, or non-default palettes break"
        );
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
            "GTK reported CSS parsing errors: {errors:?}"
        );
    }
}
