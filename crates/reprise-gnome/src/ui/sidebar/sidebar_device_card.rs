//! Connected-device cards shown below the scrolling navigation rows.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;

use super::device_sync_runtime::{
    DeviceSyncRuntime, DeviceSyncState, DeviceView, PlannedSyncPhase, SyncStep,
};
use super::device_sync_strings;

type OpenCallback = Rc<dyn Fn(String, String)>;

/// Live card widgets, keyed by device id, so a state update can refresh them
/// in place. Rebuilding the section on every update destroyed the card
/// between a click's press and release — during a sync `notify` fires on
/// every progress callback, which made the card permanently unclickable —
/// and re-cloned every widget many times a second for nothing.
type CardRegistry = Rc<RefCell<HashMap<String, DeviceCard>>>;

struct DeviceCard {
    root: gtk4::Box,
    indicator: gtk4::Stack,
    icon: gtk4::Image,
    spinner: gtk4::Spinner,
    name: gtk4::Label,
    detail_stack: gtk4::Stack,
    delta_detail: gtk4::Label,
    progress_detail: gtk4::Label,
    synced_detail: gtk4::Label,
    percent_revealer: gtk4::Revealer,
    percent: gtk4::Label,
    action: gtk4::Button,
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
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
        root.add_css_class("card");
        root.add_css_class("device-card");
        root.set_margin_bottom(3);
        root.set_margin_start(2);
        root.set_margin_end(2);

        let top = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        top.set_margin_top(8);
        top.set_margin_bottom(8);
        top.set_margin_start(10);
        top.set_margin_end(8);
        // Icon and spinner occupy the same slot: idle shows the device, a
        // running sync shows motion — the card morphs rather than being
        // rebuilt.
        let icon = gtk4::Image::from_gicon(&device.icon);
        icon.set_pixel_size(24);
        let spinner = gtk4::Spinner::new();
        spinner.set_size_request(13, 13);
        spinner.set_valign(gtk4::Align::Center);
        let indicator = gtk4::Stack::builder()
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .transition_duration(150)
            .build();
        indicator.add_named(&icon, Some("device"));
        indicator.add_named(&spinner, Some("syncing"));
        indicator.set_visible_child_name("device");
        top.append(&indicator);

        let labels = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
        labels.set_hexpand(true);
        let name = gtk4::Label::new(None);
        name.add_css_class("heading");
        name.add_css_class("device-card-title");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        let detail_stack = gtk4::Stack::builder()
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .transition_duration(150)
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
        top.append(&labels);

        // Fixed-width so the number cannot shove the title around as it grows
        // from "0 %" to "100 %".
        let percent = gtk4::Label::new(None);
        percent.add_css_class("device-card-percent");
        percent.set_width_chars(5);
        percent.set_xalign(1.0);
        let percent_revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::Crossfade)
            .transition_duration(150)
            .child(&percent)
            .reveal_child(false)
            .build();
        top.append(&percent_revealer);

        let action = gtk4::Button::new();
        action.set_valign(gtk4::Align::Center);
        action.add_css_class("device-card-action");
        action.set_action_name(Some("app.sync-device"));
        action.set_action_target_value(Some(&device.id.to_variant()));
        top.append(&action);
        root.append(&top);

        // Inset rather than full-bleed: a full-width bar is clipped by the
        // card's own corner radius and reads as half-drawn.
        let progress = gtk4::ProgressBar::new();
        progress.add_css_class("device-card-progress");
        progress.set_margin_start(12);
        progress.set_margin_end(12);
        progress.set_margin_bottom(8);
        let progress_revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::Crossfade)
            .transition_duration(150)
            .child(&progress)
            .reveal_child(false)
            .build();
        let reset_progress = progress.clone();
        progress_revealer.connect_child_revealed_notify(move |revealer| {
            if !revealer.is_child_revealed() {
                reset_progress.set_fraction(0.0);
            }
        });
        root.append(&progress_revealer);

        // The gesture lives as long as the card, so opening the device view
        // works mid-sync; the name is read fresh on click because it can
        // change (GVfs settles a generic "mtp" into the real model name).
        // The action button claims its own clicks, so Cancel never opens the
        // view.
        let open_name = Rc::new(RefCell::new(device.name.clone()));
        let open = on_open.clone();
        let id = device.id.clone();
        let click_name = open_name.clone();
        let click = gtk4::GestureClick::new();
        click.set_button(gtk4::gdk::BUTTON_PRIMARY);
        click.set_propagation_phase(gtk4::PropagationPhase::Bubble);
        click.connect_released(move |_, _, _, _| {
            let name = click_name.borrow().clone();
            open(id.clone(), name);
        });
        root.add_controller(click);
        root.set_cursor_from_name(Some("pointer"));

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
            percent_revealer,
            percent,
            action,
            progress_revealer,
            progress,
            progress_generation: Rc::new(Cell::new(0)),
            open_name,
        }
    }

    fn update(&self, device: &DeviceView) {
        self.name.set_text(&card_title(device));
        *self.open_name.borrow_mut() = device.name.clone();

        let syncing = matches!(
            device.sync_phase,
            PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing
        );
        // Without animations a spinner is a static blob; keep the device icon
        // instead so the state still switches, just without motion.
        let animate = self.root.settings().is_gtk_enable_animations();
        let transition_ms = if animate { 150 } else { 0 };
        self.indicator.set_transition_duration(transition_ms);
        self.detail_stack.set_transition_duration(transition_ms);
        self.percent_revealer.set_transition_duration(transition_ms);
        self.progress_revealer
            .set_transition_duration(transition_ms);
        self.spinner.set_spinning(syncing && animate);
        self.icon.set_from_gicon(&device.icon);
        self.indicator
            .set_visible_child_name(if syncing && animate {
                "syncing"
            } else {
                "device"
            });

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
                    "Everything in sync ✓ · {}",
                    device_sync_strings::available_space(device.available_bytes)
                ));
                self.detail_stack.set_visible_child_name("synced");
            }
        }

        self.action
            .set_label(if syncing { "Cancel" } else { "Sync" });
        self.action.set_css_classes(if syncing {
            &["flat", "device-card-action", "device-card-cancel"]
        } else {
            &["suggested-action", "device-card-action"]
        });
        self.action.set_sensitive(
            syncing
                || device
                    .delta
                    .as_ref()
                    .is_some_and(|delta| !delta.to_copy.is_empty() || !delta.to_remove.is_empty()),
        );

        match &device.sync_phase {
            PlannedSyncPhase::Syncing {
                done,
                total,
                current_track,
                bytes_done,
                bytes_total,
                ..
            } => {
                self.percent_revealer.set_reveal_child(true);
                self.percent.set_text(&device_sync_strings::sync_percent(
                    *bytes_done,
                    *bytes_total,
                ));
                self.progress_revealer.set_reveal_child(true);
                self.animate_progress(sync_fraction(*bytes_done, *bytes_total), animate);
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
                self.percent_revealer.set_reveal_child(false);
                self.progress_revealer.set_reveal_child(false);
                self.root.set_tooltip_text(None);
            }
        }
    }

    fn animate_progress(&self, target: f64, animate: bool) {
        let generation = self.progress_generation.get().saturating_add(1);
        self.progress_generation.set(generation);
        if !animate {
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
            let start_time = started_at.get().unwrap_or_else(|| {
                let start_time = frame_clock.frame_time();
                started_at.set(Some(start_time));
                start_time
            });
            let elapsed = frame_clock.frame_time().saturating_sub(start_time) as f64;
            let linear = (elapsed / 150_000.0).clamp(0.0, 1.0);
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
    ".device-card:hover { background-color: alpha(#ffffff, 0.03); }\n\
     .device-card-title { font-size: 13px; }\n\
     .device-card-detail { font-size: 11.5px; color: alpha(@window_fg_color, 0.45); }\n\
     .device-card-percent { font-size: 11.5px; font-feature-settings: \"tnum\"; \
       color: alpha(@window_fg_color, 0.45); }\n\
     .device-card-cancel { color: alpha(@window_fg_color, 0.55); }\n\
     .device-card-cancel:hover { color: @window_fg_color; \
       background-color: alpha(#ffffff, 0.10); }\n\
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
                device_sync_strings::available_space(device.available_bytes)
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
            available_bytes: None,
            total_bytes: None,
            contents: Default::default(),
            scanning: false,
            scan_error: None,
            draft_playlists: Vec::new(),
            last_enqueue: None,
            snapshot: reprise_core::device_sync::DeviceQueue::new().snapshot(),
            settings: reprise_core::device_sync::DeviceSettings {
                device_serial: "pixel".into(),
                device_name: "Pixel 8".into(),
                selection: reprise_core::device_sync::DeviceSelection::EntireLibrary,
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
    #[ignore = "requires a display; run via xvfb-run"]
    fn disabled_animations_apply_progress_and_state_changes_immediately() {
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
        assert_eq!(card.detail_stack.transition_duration(), 0);
        assert_eq!(
            card.detail_stack.visible_child_name().as_deref(),
            Some("progress")
        );
        assert!(card.progress_revealer.reveals_child());
        settings.set_gtk_enable_animations(previous);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn enabled_animations_interpolate_progress_to_the_latest_fraction() {
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
        gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
            std::time::Duration::from_millis(250),
        ));
        assert!((card.progress.fraction() - 0.5).abs() < 1e-6);
        window.close();
        settings.set_gtk_enable_animations(previous);
    }
}

#[cfg(test)]
mod css_tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn css_covers_the_sync_card_vocabulary() {
        let css = super::css();
        for marker in [
            ".device-card:hover",
            ".device-card-detail",
            ".device-card-percent",
            ".device-card-cancel:hover",
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
        let errors: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let provider = gtk4::CssProvider::new();
        {
            let errors = errors.clone();
            provider.connect_parsing_error(move |_, section, error| {
                errors.borrow_mut().push(format!("{section:?}: {error}"));
            });
        }
        let combined = format!(
            "{}\n{}",
            crate::ui::style::theme::theme_css(crate::ui::style::theme::Theme::DEFAULT, true),
            super::css()
        );
        provider.load_from_string(&combined);
        assert!(
            errors.borrow().is_empty(),
            "GTK reported CSS parsing errors: {:?}",
            errors.borrow()
        );
    }
}
