//! Compact scan status shown in the Preferences dialog header chrome.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::AnimationExt;

use super::strings;

#[cfg(test)]
const CHIP_FADE_MS: u32 = crate::ui::motion::MICRO_MS;

type Callback = Rc<dyn Fn()>;
type CallbackSlot = Rc<RefCell<Option<Callback>>>;

#[derive(Clone, Default)]
pub(super) struct FadeGeneration(Rc<Cell<u64>>);

impl FadeGeneration {
    pub(super) fn start(&self) -> u64 {
        let generation = self.0.get().wrapping_add(1);
        self.0.set(generation);
        generation
    }

    pub(super) fn is_current(&self, generation: u64) -> bool {
        self.0.get() == generation
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ChipState {
    label: String,
    tooltip: Option<String>,
    show_cancel: bool,
    warning: bool,
}

fn chip_state(label: &str, tooltip: Option<&str>, warning: bool) -> ChipState {
    ChipState {
        label: label.to_owned(),
        tooltip: tooltip.map(str::to_owned),
        show_cancel: !warning,
        warning,
    }
}

fn gear_should_spin(running: bool, animations_enabled: bool) -> bool {
    running && animations_enabled
}

fn apply_gear_motion(gear: &gtk4::Image, running: bool) {
    if gear_should_spin(running, crate::ui::motion::animations_enabled()) {
        gear.add_css_class("scan-chip-gear-spinning");
    } else {
        gear.remove_css_class("scan-chip-gear-spinning");
    }
}

#[derive(Clone)]
pub(in crate::ui) struct ScanChip {
    inner: Rc<ScanChipWidgets>,
}

struct ScanChipWidgets {
    root: gtk4::Overlay,
    action: gtk4::Button,
    gear: gtk4::Image,
    label: gtk4::Label,
    cancel: gtk4::Button,
    running: Rc<Cell<bool>>,
    fade_generation: FadeGeneration,
    fade: RefCell<Option<adw::TimedAnimation>>,
    on_activate: CallbackSlot,
    on_cancel: CallbackSlot,
}

impl ScanChip {
    pub(in crate::ui) fn new() -> Self {
        let gear = gtk4::Image::from_icon_name("emblem-system-symbolic");
        gear.add_css_class("scan-chip-gear");

        let label = gtk4::Label::new(None);
        label.add_css_class("scan-chip-label");
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);

        let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        content.append(&gear);
        content.append(&label);

        let action = gtk4::Button::builder()
            .child(&content)
            .has_frame(false)
            .focusable(true)
            .build();
        action.add_css_class("scan-chip-action");

        let cancel = gtk4::Button::with_label("×");
        cancel.set_halign(gtk4::Align::End);
        cancel.set_valign(gtk4::Align::Center);
        // a11y-semantics: role=button name=cancel-scan state=focusable action=activate
        cancel.set_focusable(true);
        cancel.add_css_class("scan-chip-cancel");
        cancel.update_property(&[gtk4::accessible::Property::Label(&strings::text(
            strings::SCAN_CHIP_CANCEL,
        ))]);

        let root = gtk4::Overlay::new();
        root.add_css_class("scan-chip");
        root.set_child(Some(&action));
        root.add_overlay(&cancel);
        root.set_opacity(0.0);
        root.set_visible(false);

        let on_activate: CallbackSlot = Rc::new(RefCell::new(None));
        let activate_slot = on_activate.clone();
        action.connect_clicked(move |_| {
            let callback = activate_slot.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
        });

        let on_cancel: CallbackSlot = Rc::new(RefCell::new(None));
        let cancel_slot = on_cancel.clone();
        cancel.connect_clicked(move |_| {
            let callback = cancel_slot.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
        });

        let running = Rc::new(Cell::new(false));
        if let Some(settings) = gtk4::Settings::default() {
            let gear = gear.downgrade();
            let running = running.clone();
            settings.connect_gtk_enable_animations_notify(move |_| {
                if let Some(gear) = gear.upgrade() {
                    apply_gear_motion(&gear, running.get());
                }
            });
        }

        Self {
            inner: Rc::new(ScanChipWidgets {
                root,
                action,
                gear,
                label,
                cancel,
                running,
                fade_generation: FadeGeneration::default(),
                fade: RefCell::new(None),
                on_activate,
                on_cancel,
            }),
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Widget {
        self.inner.root.upcast_ref()
    }

    pub(in crate::ui) fn set_running(&self, label: &str, tooltip: Option<&str>) {
        self.apply_state(&chip_state(label, tooltip, false));
    }

    pub(in crate::ui) fn set_warning(&self, label: &str, tooltip: Option<&str>) {
        self.apply_state(&chip_state(label, tooltip, true));
    }

    pub(in crate::ui) fn hide(&self) {
        self.fade_to(0.0, true);
    }

    pub(in crate::ui) fn set_on_activate(&self, callback: impl Fn() + 'static) {
        *self.inner.on_activate.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_cancel(&self, callback: impl Fn() + 'static) {
        *self.inner.on_cancel.borrow_mut() = Some(Rc::new(callback));
    }

    fn apply_state(&self, state: &ChipState) {
        self.inner.label.set_label(&state.label);
        self.inner.action.set_tooltip_text(state.tooltip.as_deref());
        self.inner
            .action
            .update_property(&[gtk4::accessible::Property::Label(
                &strings::scan_chip_accessible_label(&state.label),
            )]);
        self.inner.cancel.set_visible(state.show_cancel);
        if state.warning {
            self.inner.root.add_css_class("warning");
        } else {
            self.inner.root.remove_css_class("warning");
        }
        self.inner.running.set(!state.warning);
        apply_gear_motion(&self.inner.gear, self.inner.running.get());
        self.inner.root.set_visible(true);
        self.fade_to(1.0, false);
    }

    fn fade_to(&self, opacity: f64, hide_when_done: bool) {
        let generation = self.inner.fade_generation.start();
        if !crate::ui::motion::animations_enabled() {
            if let Some(animation) = self.inner.fade.borrow_mut().take() {
                animation.skip();
            }
            self.inner.root.set_opacity(opacity);
            if hide_when_done {
                self.inner.root.set_visible(false);
            }
            return;
        }
        let target = adw::PropertyAnimationTarget::new(&self.inner.root, "opacity");
        let animation = crate::ui::motion::timed(
            &self.inner.root,
            self.inner.root.opacity(),
            opacity,
            crate::ui::motion::MICRO,
            target,
        );
        if hide_when_done {
            let root = self.inner.root.downgrade();
            let fade_generation = self.inner.fade_generation.clone();
            animation.connect_done(move |_| {
                if fade_generation.is_current(generation) {
                    if let Some(root) = root.upgrade() {
                        root.set_visible(false);
                    }
                }
            });
        }
        crate::ui::motion::replace_animation(&self.inner.fade, animation.clone());
        animation.play();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_and_warning_states_keep_information_explicit() {
        let running = chip_state("Scanning · 39%", Some("748 of 1,909 checked"), false);
        assert_eq!(running.label, "Scanning · 39%");
        assert_eq!(running.tooltip.as_deref(), Some("748 of 1,909 checked"));
        assert!(running.show_cancel);
        assert!(!running.warning);

        let warning = chip_state(
            "Library unavailable",
            Some("/media/Music not mounted"),
            true,
        );
        assert_eq!(warning.label, "Library unavailable");
        assert!(!warning.label.contains('%'));
        assert!(!warning.show_cancel);
        assert!(warning.warning);
    }

    #[test]
    fn scan_chip_css_uses_the_approved_geometry_and_colour_tokens() {
        let css = super::super::scan_card_css::css();
        for token in [
            "border-radius: 999px",
            "rgba(46, 194, 126, 0.13)",
            "rgba(46, 194, 126, 0.32)",
            "#a9e6c8",
            "font-size: 11.5px",
            "font-weight: 600",
            "@keyframes scan-chip-gear-spin",
            "transform: rotate(360deg)",
        ] {
            assert!(css.contains(token), "missing scan-chip CSS token: {token}");
        }
    }

    #[test]
    fn chip_fade_is_micro_and_gear_motion_obeys_the_central_gate() {
        assert_eq!(CHIP_FADE_MS, 150);
        assert_eq!(CHIP_FADE_MS, crate::ui::motion::MICRO_MS);
        assert!(gear_should_spin(true, true));
        assert!(!gear_should_spin(true, false));
        assert!(!gear_should_spin(false, true));
    }

    #[test]
    fn replacing_a_fade_invalidates_its_completion_callback() {
        let generation = FadeGeneration::default();
        let hiding = generation.start();
        assert!(generation.is_current(hiding));

        generation.start();
        assert!(!generation.is_current(hiding));
    }
}
