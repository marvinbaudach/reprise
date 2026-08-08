//! The sidebar's device section: what it shows, and what it puts away.
//!
//! The cards themselves live in [`super::sidebar_device_card`]; this module
//! owns only the arrangement around them.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;

use super::sidebar_device_card::{menu, CardRegistry, DeviceCard, OpenCallback};
use crate::ui::device_sync_runtime::{DeviceSyncRuntime, DeviceSyncState, DeviceView};

const ARROW_CLOSED: &str = "pan-end-symbolic";
const ARROW_OPEN: &str = "pan-down-symbolic";

/// The sidebar's device section.
///
/// Hardware that is plugged in stands open — it is a place the user can go.
/// A remembered device is history: it cannot be synced, shows no balance, and
/// waits behind the heading until it is asked for, so an absent phone does not
/// sit in the sidebar every session claiming a row.
struct DeviceSection {
    root: gtk4::Box,
    heading: gtk4::Button,
    arrow: gtk4::Image,
    present: gtk4::Box,
    remembered: gtk4::Revealer,
    history: gtk4::Box,
}

impl DeviceSection {
    fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        root.set_margin_start(10);
        root.set_margin_end(10);
        root.set_margin_top(8);
        root.set_margin_bottom(8);
        root.set_visible(false);

        let label = gtk4::Label::new(Some("DEVICES"));
        label.add_css_class("caption");
        label.add_css_class("dim-label");
        label.set_xalign(0.0);
        label.set_hexpand(true);
        let arrow = gtk4::Image::from_icon_name(ARROW_CLOSED);
        arrow.add_css_class("dim-label");
        arrow.set_visible(false);
        let heading_content = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        heading_content.append(&label);
        heading_content.append(&arrow);
        // A real button, not a label with a gesture: disclosure is an action,
        // and an action has to be reachable from the keyboard.
        let heading = gtk4::Button::builder()
            .child(&heading_content)
            .has_frame(false)
            .build();
        heading.add_css_class("device-section-heading");

        let present = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let history = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let remembered = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .transition_duration(crate::ui::motion::STANDARD_MS)
            .child(&history)
            .reveal_child(false)
            .build();

        heading.connect_clicked({
            let remembered = remembered.clone();
            let arrow = arrow.clone();
            move |_| {
                let opening = !remembered.reveals_child();
                remembered.set_reveal_child(opening);
                arrow.set_icon_name(Some(if opening { ARROW_OPEN } else { ARROW_CLOSED }));
            }
        });

        root.append(&heading);
        root.append(&present);
        root.append(&remembered);
        Self {
            root,
            heading,
            arrow,
            present,
            remembered,
            history,
        }
    }

    /// Heading and disclosure follow what the section actually holds.
    fn apply_layout(&self, present: usize, remembered: usize) {
        self.root.set_visible(present + remembered > 0);
        let has_history = remembered > 0;
        self.arrow.set_visible(has_history);
        // With nothing behind it the heading is a label again — insensitive,
        // so it neither takes focus nor invites a click that does nothing.
        self.heading.set_sensitive(has_history);
        if !has_history {
            self.remembered.set_reveal_child(false);
            self.arrow.set_icon_name(Some(ARROW_CLOSED));
        }
    }
}

/// Devices that stand open in the sidebar, and devices that wait behind the
/// heading. Connection is the whole question: a device that is here can be
/// opened and synced, one that is not is history.
fn present_and_remembered(devices: &[DeviceView]) -> (Vec<&DeviceView>, Vec<&DeviceView>) {
    devices.iter().partition(|device| device.connected)
}

pub(super) fn bind(runtime: &Rc<DeviceSyncRuntime>, on_open: OpenCallback) -> gtk4::Box {
    let section = DeviceSection::new();
    let root = section.root.clone();

    let cards: CardRegistry = Rc::new(RefCell::new(HashMap::new()));
    let subscription = runtime.subscribe(Rc::new({
        let cards = cards.clone();
        let runtime = runtime.clone();
        move |state| render(&section, &cards, &state, &on_open, &runtime)
    }));
    subscription.retain_for_widget(&root);
    root
}

/// Moves `card` into `target` unless it already hangs there. A device that
/// connects or disconnects changes which half of the section it belongs to,
/// and the card travels rather than being rebuilt — rebuilding is what used
/// to destroy a card between a click's press and release.
fn place(card: &gtk4::Button, target: &gtk4::Box) {
    if card.parent().as_ref() == Some(target.upcast_ref::<gtk4::Widget>()) {
        return;
    }
    detach(card);
    target.append(card);
}

fn detach(card: &gtk4::Button) {
    if let Some(parent) = card.parent().and_downcast::<gtk4::Box>() {
        parent.remove(card);
    }
}

fn order(container: &gtk4::Box, devices: &[&DeviceView], registry: &HashMap<String, DeviceCard>) {
    let mut previous: Option<gtk4::Widget> = None;
    for device in devices {
        let card = &registry[&device.id];
        container.reorder_child_after(card.root(), previous.as_ref());
        previous = Some(card.root().clone().upcast());
    }
}

fn render(
    section: &DeviceSection,
    cards: &CardRegistry,
    state: &DeviceSyncState,
    on_open: &OpenCallback,
    runtime: &Rc<DeviceSyncRuntime>,
) {
    let (present, remembered) = present_and_remembered(&state.devices);
    section.apply_layout(present.len(), remembered.len());

    let mut registry = cards.borrow_mut();
    // Drop cards for devices that went away.
    registry.retain(|id, card| {
        let keep = state.devices.iter().any(|device| &device.id == id);
        if !keep {
            detach(card.root());
        }
        keep
    });
    // Update in place, building only genuinely new devices.
    for device in present.iter().chain(remembered.iter()) {
        match registry.get(&device.id) {
            Some(card) => card.update(device),
            None => {
                let card = DeviceCard::new(device, on_open);
                menu::wire(card.root(), runtime, &device.id);
                card.update(device);
                registry.insert(device.id.clone(), card);
            }
        }
    }
    for device in &present {
        place(registry[&device.id].root(), &section.present);
    }
    for device in &remembered {
        place(registry[&device.id].root(), &section.history);
    }
    order(&section.present, &present, &registry);
    order(&section.history, &remembered, &registry);
}

#[cfg(test)]
mod tests {
    use super::{present_and_remembered, DeviceSection};
    use crate::ui::device_sync_runtime::PlannedSyncPhase;
    use crate::ui::sidebar::sidebar_device_card::tests::view;
    use gtk4::prelude::*;

    #[test]
    fn mtp_50_connected_devices_stand_open_while_remembered_ones_wait_behind_the_heading() {
        let present = view(PlannedSyncPhase::Idle);
        let mut absent = view(PlannedSyncPhase::Idle);
        absent.id = "pixel-old".into();
        absent.connected = false;
        absent.session_state = reprise_core::device_sync::DeviceSessionState::Remembered;

        let devices = [present, absent];
        let (open, behind) = present_and_remembered(&devices);

        assert_eq!(
            open.iter().map(|device| &device.id).collect::<Vec<_>>(),
            vec!["pixel"],
            "hardware that is plugged in is a place to go and stays in plain sight"
        );
        assert_eq!(
            behind.iter().map(|device| &device.id).collect::<Vec<_>>(),
            vec!["pixel-old"],
            "a device that is not here is history and waits behind the heading"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mtp_50_the_heading_puts_remembered_devices_away_until_they_are_asked_for() {
        gtk4::init().unwrap();
        let section = DeviceSection::new();
        section.apply_layout(1, 1);

        assert!(
            section.heading.is_sensitive(),
            "with history behind it, the heading is something to open"
        );
        assert!(
            !section.remembered.reveals_child(),
            "a device that is not here does not greet the user unasked"
        );

        section.heading.emit_clicked();
        assert!(
            section.remembered.reveals_child(),
            "one click brings the remembered devices out"
        );

        section.heading.emit_clicked();
        assert!(
            !section.remembered.reveals_child(),
            "and the same click puts them away again"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mtp_50_a_section_without_history_offers_nothing_to_open() {
        gtk4::init().unwrap();
        let section = DeviceSection::new();
        section.apply_layout(1, 0);

        assert!(
            !section.arrow.is_visible(),
            "no history, no disclosure arrow"
        );
        assert!(
            !section.heading.is_sensitive(),
            "a heading with nothing behind it is a label, not a control"
        );
    }
}
