//! Connected-device cards shown below the scrolling navigation rows.

use std::cell::RefCell;
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
    icon: gtk4::Image,
    spinner: gtk4::Spinner,
    name: gtk4::Label,
    detail: gtk4::Label,
    percent: gtk4::Label,
    action: gtk4::Button,
    progress: gtk4::ProgressBar,
    /// Read by the click gesture, which outlives any single state update.
    open_name: Rc<RefCell<String>>,
}

pub(super) fn bind(
    sidebar_root: &gtk4::Box,
    runtime: &Rc<DeviceSyncRuntime>,
    on_open: OpenCallback,
) {
    let section = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    section.set_margin_start(10);
    section.set_margin_end(10);
    section.set_margin_top(8);
    section.set_margin_bottom(8);
    section.set_visible(false);
    let first = sidebar_root.first_child();
    sidebar_root.insert_child_after(&section, first.as_ref());

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
        top.append(&icon);
        let spinner = gtk4::Spinner::new();
        spinner.set_size_request(13, 13);
        spinner.set_valign(gtk4::Align::Center);
        spinner.set_visible(false);
        top.append(&spinner);

        let labels = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
        labels.set_hexpand(true);
        let name = gtk4::Label::new(None);
        name.add_css_class("heading");
        name.add_css_class("device-card-title");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        let detail = gtk4::Label::new(None);
        detail.add_css_class("device-card-detail");
        detail.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        detail.set_xalign(0.0);
        labels.append(&name);
        labels.append(&detail);
        top.append(&labels);

        // Fixed-width so the number cannot shove the title around as it grows
        // from "0 %" to "100 %".
        let percent = gtk4::Label::new(None);
        percent.add_css_class("device-card-percent");
        percent.set_width_chars(5);
        percent.set_xalign(1.0);
        percent.set_visible(false);
        top.append(&percent);

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
        progress.set_visible(false);
        root.append(&progress);

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
            icon,
            spinner,
            name,
            detail,
            percent,
            action,
            progress,
            open_name,
        }
    }

    fn update(&self, device: &DeviceView) {
        self.name.set_text(&card_title(device));
        self.detail.set_text(&card_subtitle(device));
        *self.open_name.borrow_mut() = device.name.clone();

        let syncing = matches!(
            device.sync_phase,
            PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing
        );
        // Without animations a spinner is a static blob; keep the device icon
        // instead so the state still switches, just without motion.
        let animate = syncing && self.root.settings().is_gtk_enable_animations();
        self.spinner.set_visible(animate);
        self.spinner.set_spinning(animate);
        self.icon.set_visible(!animate);
        if !animate {
            self.icon.set_from_gicon(&device.icon);
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
                self.percent.set_visible(true);
                self.percent.set_text(&device_sync_strings::sync_percent(
                    *bytes_done,
                    *bytes_total,
                ));
                self.progress.set_visible(true);
                self.progress.set_fraction(if *bytes_total == 0 {
                    0.0
                } else {
                    (*bytes_done as f64 / *bytes_total as f64).clamp(0.0, 1.0)
                });
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
                // Completion drops straight back to the idle card — no
                // artificial hold at 100 %; the toast reports the outcome.
                self.percent.set_visible(false);
                self.progress.set_visible(false);
                self.root.set_tooltip_text(None);
            }
        }
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

/// What is happening to the named track. Transcoding is deliberately absent:
/// the encoder pipeline never reports it as a step, so there is no honest
/// glyph for it yet.
fn step_glyph(step: &SyncStep) -> &'static str {
    match step {
        SyncStep::Copying => "↑",
        SyncStep::Removing => "−",
        SyncStep::WritingPlaylists => "≡",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syncing_title_is_explicit() {
        assert_eq!(
            card_title(&DeviceView {
                id: "pixel".into(),
                name: "Pixel 8".into(),
                icon: gtk4::gio::ThemedIcon::new("phone-symbolic").upcast(),
                connected: true,
                available_bytes: None,
                contents: Default::default(),
                scanning: false,
                scan_error: None,
                draft_playlists: Vec::new(),
                last_enqueue: None,
                snapshot: reprise_core::device_sync::DeviceQueue::new().snapshot(),
                settings: reprise_core::device_sync::DeviceSettings {
                    device_serial: "pixel".into(),
                    device_name: "Pixel 8".into(),
                    selection: Default::default(),
                    opus_bitrate: 0,
                    ratings_back: false,
                    remove_deleted: true,
                },
                delta: None,
                sync_phase: PlannedSyncPhase::Finishing,
                sync_error: None,
                last_sync: None,
                tracks: Vec::new(),
                selected_track_count: 0,
            }),
            "Syncing Pixel 8"
        );
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
