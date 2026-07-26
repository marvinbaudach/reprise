//! App-authored motion tokens and the central reduced-motion contract.
//!
//! CSS T-V probe result (2026-07-18, executed headless via dbus + Xvfb; see
//! `mot_7_css_honours_enable_animations_setting` in
//! `ui/style/mod.rs` — the style module owns CSS provider construction): GTK's CSS
//! machinery fully honours `gtk-enable-animations=false` — `transition:`
//! properties hard-switch to their end value and `@keyframes` animations do
//! not run at all. CSS therefore needs no additional gating for MOT-7; the
//! central contract here covers Adw animations (follow property), hand-built
//! tick callbacks, and pulse timers.

use std::cell::RefCell;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::AnimationExt;

pub(in crate::ui) const MICRO_MS: u32 = 150;
pub(in crate::ui) const STANDARD_MS: u32 = 250;
pub(in crate::ui) const AMBIENT_MS: u32 = 400;
// My Stats bar-only entrance motion.
pub(in crate::ui) const STATS_CHART_BAR_MS: u32 = 500;
pub(in crate::ui) const STATS_LABEL_MS: u32 = 150;
pub(in crate::ui) const STATS_HORIZONTAL_BAR_MS: u32 = 450;
pub(in crate::ui) const STATS_TWEEN_MS: u32 = 250;
pub(in crate::ui) const STATS_ENTRANCE_DELAY_MS: u32 = 100;
pub(in crate::ui) const STATS_CHART_STAGGER_MS: u32 = 80;
pub(in crate::ui) const STATS_HORIZONTAL_STAGGER_MS: u32 = 40;

pub(in crate::ui) const MICRO_EASING: adw::Easing = adw::Easing::EaseOutQuad;
pub(in crate::ui) const STANDARD_EASING: adw::Easing = adw::Easing::EaseOutCubic;
pub(in crate::ui) const AMBIENT_EASING: adw::Easing = adw::Easing::EaseOutCubic;
const STATS_DRIVER_EASING: adw::Easing = adw::Easing::Linear;
const STATS_BEZIER_X1: f64 = 0.16;
const STATS_BEZIER_Y1: f64 = 1.0;
const STATS_BEZIER_X2: f64 = 0.3;
const STATS_BEZIER_Y2: f64 = 1.0;

pub(in crate::ui) const MICRO_CSS_EASING: &str = "ease-out";
// Kept for a complete CSS-easing token set and pinned by the motion-token
// tests; no CSS rule consumes the standard/ambient curve yet.
#[allow(dead_code)]
pub(in crate::ui) const STANDARD_CSS_EASING: &str = "cubic-bezier(0.16, 1, 0.3, 1)";
#[allow(dead_code)]
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
pub(in crate::ui) const STATS_CHART_BAR: MotionToken = MotionToken {
    duration_ms: STATS_CHART_BAR_MS,
    easing: STATS_DRIVER_EASING,
};
pub(in crate::ui) const STATS_LABEL: MotionToken = MotionToken {
    duration_ms: STATS_LABEL_MS,
    easing: STATS_DRIVER_EASING,
};
pub(in crate::ui) const STATS_HORIZONTAL_BAR: MotionToken = MotionToken {
    duration_ms: STATS_HORIZONTAL_BAR_MS,
    easing: STATS_DRIVER_EASING,
};
pub(in crate::ui) const STATS_TWEEN: MotionToken = MotionToken {
    duration_ms: STATS_TWEEN_MS,
    easing: STATS_DRIVER_EASING,
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

pub(in crate::ui) fn stats_timed(
    widget: &impl IsA<gtk4::Widget>,
    token: MotionToken,
    update: impl Fn(f64) + 'static,
) -> adw::TimedAnimation {
    let target =
        adw::CallbackAnimationTarget::new(move |progress| update(stats_ease_out(progress)));
    timed(widget, 0.0, 1.0, token, target)
}

fn stats_ease_out(progress: f64) -> f64 {
    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }
    let mut lower = 0.0;
    let mut upper = 1.0;
    for _ in 0..16 {
        let parameter = (lower + upper) / 2.0;
        if cubic_bezier(parameter, STATS_BEZIER_X1, STATS_BEZIER_X2) < progress {
            lower = parameter;
        } else {
            upper = parameter;
        }
    }
    cubic_bezier((lower + upper) / 2.0, STATS_BEZIER_Y1, STATS_BEZIER_Y2)
}

fn cubic_bezier(parameter: f64, first: f64, second: f64) -> f64 {
    let inverse = 1.0 - parameter;
    3.0 * inverse * inverse * parameter * first
        + 3.0 * inverse * parameter * parameter * second
        + parameter * parameter * parameter
}

pub(in crate::ui) fn animations_enabled() -> bool {
    gtk4::Settings::default().is_none_or(|settings| settings.is_gtk_enable_animations())
}

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
        assert_eq!(STATS_CHART_BAR_MS, 500);
        assert_eq!(STATS_LABEL_MS, 150);
        assert_eq!(STATS_HORIZONTAL_BAR_MS, 450);
        assert_eq!(STATS_TWEEN_MS, 250);
        assert_eq!(STATS_ENTRANCE_DELAY_MS, 100);
        assert_eq!(STATS_CHART_STAGGER_MS, 80);
        assert_eq!(STATS_HORIZONTAL_STAGGER_MS, 40);
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
        for token in [
            STATS_CHART_BAR,
            STATS_LABEL,
            STATS_HORIZONTAL_BAR,
            STATS_TWEEN,
        ] {
            assert_eq!(token.easing, libadwaita::Easing::Linear);
        }
        assert_eq!(
            (
                STATS_BEZIER_X1,
                STATS_BEZIER_Y1,
                STATS_BEZIER_X2,
                STATS_BEZIER_Y2,
            ),
            (0.16, 1.0, 0.3, 1.0)
        );
        assert_eq!(stats_ease_out(0.0), 0.0);
        assert!((stats_ease_out(0.5) - 0.971_780).abs() < 0.000_1);
        assert_eq!(stats_ease_out(1.0), 1.0);
        assert_eq!(MICRO_CSS_EASING, "ease-out");
        assert_eq!(STANDARD_CSS_EASING, "cubic-bezier(0.16, 1, 0.3, 1)");
        assert_eq!(AMBIENT_CSS_EASING, STANDARD_CSS_EASING);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mot_1_timed_pins_duration_easing_and_follow() {
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
