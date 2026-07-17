//! App-authored motion tokens and the central reduced-motion contract.
//!
//! CSS T-V probe result (2026-07-18, executed headless via dbus + Xvfb; see
//! `css_probe_transitions_and_keyframes_under_disabled_animations` in
//! `ui/style/mod.rs` — the style module owns CSS provider construction): GTK's CSS
//! machinery fully honours `gtk-enable-animations=false` — `transition:`
//! properties hard-switch to their end value and `@keyframes` animations do
//! not run at all. CSS therefore needs no additional gating for MOT-7; the
//! central contract here covers Adw animations (follow property), hand-built
//! tick callbacks, and pulse timers.

#![allow(dead_code)] // Token consumers land in the following Phase 1 tasks.

use std::cell::RefCell;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::AnimationExt;

pub(in crate::ui) const MICRO_MS: u32 = 150;
pub(in crate::ui) const STANDARD_MS: u32 = 250;
pub(in crate::ui) const AMBIENT_MS: u32 = 400;

pub(in crate::ui) const MICRO_EASING: adw::Easing = adw::Easing::EaseOutQuad;
pub(in crate::ui) const STANDARD_EASING: adw::Easing = adw::Easing::EaseOutCubic;
pub(in crate::ui) const AMBIENT_EASING: adw::Easing = adw::Easing::EaseOutCubic;

pub(in crate::ui) const MICRO_CSS_EASING: &str = "ease-out";
pub(in crate::ui) const STANDARD_CSS_EASING: &str = "cubic-bezier(0.16, 1, 0.3, 1)";
pub(in crate::ui) const AMBIENT_CSS_EASING: &str = STANDARD_CSS_EASING;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct MotionToken {
    duration_ms: u32,
    easing: adw::Easing,
}

pub(in crate::ui) const MICRO: MotionToken = MotionToken {
    duration_ms: MICRO_MS,
    easing: MICRO_EASING,
};
pub(in crate::ui) const STANDARD: MotionToken = MotionToken {
    duration_ms: STANDARD_MS,
    easing: STANDARD_EASING,
};
pub(in crate::ui) const AMBIENT: MotionToken = MotionToken {
    duration_ms: AMBIENT_MS,
    easing: AMBIENT_EASING,
};

pub(in crate::ui) const fn half(token: MotionToken) -> u32 {
    token.duration_ms / 2
}

pub(in crate::ui) fn timed(
    widget: &impl IsA<gtk4::Widget>,
    from: f64,
    to: f64,
    token: MotionToken,
    target: impl IsA<adw::AnimationTarget>,
) -> adw::TimedAnimation {
    let animation = adw::TimedAnimation::new(widget, from, to, token.duration_ms, target);
    animation.set_easing(token.easing);
    animation.set_follow_enable_animations_setting(true);
    animation
}

#[allow(dead_code)] // First consumer lands in the following Phase 1 task.
pub(in crate::ui) fn animations_enabled() -> bool {
    gtk4::Settings::default().is_none_or(|settings| settings.is_gtk_enable_animations())
}

#[allow(dead_code)] // First consumers land in the following Phase 1 task.
pub(in crate::ui) fn replace_animation(
    slot: &RefCell<Option<adw::TimedAnimation>>,
    animation: adw::TimedAnimation,
) {
    let previous = slot.borrow_mut().take();
    if let Some(previous) = previous {
        previous.skip();
    }
    *slot.borrow_mut() = Some(animation);
}

#[cfg(test)]
mod tests {
    use super::*;
    use libadwaita::prelude::AnimationExt;

    #[test]
    fn motion_tokens_match_the_approved_values() {
        assert_eq!(MICRO_MS, 150);
        assert_eq!(STANDARD_MS, 250);
        assert_eq!(AMBIENT_MS, 400);
        assert_eq!(half(MICRO), 75);
        assert_eq!(half(STANDARD), 125);
        assert_eq!(half(AMBIENT), 200);
    }

    #[test]
    fn motion_tokens_use_the_approved_easings() {
        assert_eq!(MICRO_EASING, libadwaita::Easing::EaseOutQuad);
        assert_eq!(STANDARD_EASING, libadwaita::Easing::EaseOutCubic);
        assert_eq!(AMBIENT_EASING, libadwaita::Easing::EaseOutCubic);
        assert_eq!(MICRO.easing, MICRO_EASING);
        assert_eq!(STANDARD.easing, STANDARD_EASING);
        assert_eq!(AMBIENT.easing, AMBIENT_EASING);
        assert_eq!(MICRO_CSS_EASING, "ease-out");
        assert_eq!(STANDARD_CSS_EASING, "cubic-bezier(0.16, 1, 0.3, 1)");
        assert_eq!(AMBIENT_CSS_EASING, STANDARD_CSS_EASING);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn timed_enables_the_system_animation_setting_contract() {
        gtk4::init().unwrap();
        let label = gtk4::Label::new(None);

        // Note: the installed libadwaita already defaults
        // `follow-enable-animations-setting` to true; `timed()` still pins it
        // explicitly so the MOT-7 contract holds regardless of library default.
        let target = libadwaita::PropertyAnimationTarget::new(&label, "opacity");
        let animation = timed(&label, 0.0, 1.0, STANDARD, target);
        assert!(animation.follows_enable_animations_setting());
        assert_eq!(animation.duration(), STANDARD_MS);
        assert_eq!(animation.easing(), libadwaita::Easing::EaseOutCubic);
    }
}
