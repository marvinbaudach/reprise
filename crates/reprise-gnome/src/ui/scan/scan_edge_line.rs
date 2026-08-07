//! Two-pixel scan progress line overlaid on the Preferences dialog edge.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::AnimationExt;

use super::scan_chip::FadeGeneration;
use super::scan_progress::{PulseGeneration, PULSE_INTERVAL, PULSE_STEP};

fn clamped_fraction(fraction: f64) -> f64 {
    fraction.clamp(0.0, 1.0)
}

#[derive(Clone)]
pub(in crate::ui) struct ScanEdgeLine {
    inner: Rc<ScanEdgeLineWidgets>,
}

struct ScanEdgeLineWidgets {
    progress: gtk4::ProgressBar,
    pulse_generation: PulseGeneration,
    fade_generation: FadeGeneration,
    fade: RefCell<Option<adw::TimedAnimation>>,
}

impl ScanEdgeLine {
    pub(in crate::ui) fn new() -> Self {
        let progress = gtk4::ProgressBar::builder()
            .valign(gtk4::Align::Start)
            .hexpand(true)
            .visible(false)
            .opacity(0.0)
            .build();
        progress.add_css_class("scan-edge-line");
        progress.set_pulse_step(PULSE_STEP);

        Self {
            inner: Rc::new(ScanEdgeLineWidgets {
                progress,
                pulse_generation: PulseGeneration::default(),
                fade_generation: FadeGeneration::default(),
                fade: RefCell::new(None),
            }),
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Widget {
        self.inner.progress.upcast_ref()
    }

    pub(in crate::ui) fn set_fraction(&self, fraction: f64) {
        self.inner.pulse_generation.cancel();
        self.show();
        self.inner.progress.set_fraction(clamped_fraction(fraction));
    }

    pub(in crate::ui) fn set_indeterminate(&self) {
        let generation = self.inner.pulse_generation.start();
        self.show();
        self.inner.progress.set_fraction(0.0);
        if !crate::ui::motion::animations_enabled() {
            return;
        }
        self.inner.progress.pulse();

        let progress = self.inner.progress.downgrade();
        let pulse_generation = self.inner.pulse_generation.clone();
        glib::timeout_add_local(PULSE_INTERVAL, move || {
            if !pulse_generation.is_current(generation) {
                return glib::ControlFlow::Break;
            }
            let Some(progress) = progress.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if !crate::ui::motion::animations_enabled() {
                progress.set_fraction(0.0);
                return glib::ControlFlow::Break;
            }
            progress.pulse();
            glib::ControlFlow::Continue
        });
    }

    pub(in crate::ui) fn hide(&self) {
        self.inner.pulse_generation.cancel();
        let generation = self.inner.fade_generation.start();
        if !self.inner.progress.is_visible() {
            return;
        }
        if !crate::ui::motion::animations_enabled() {
            self.cancel_fade();
            self.inner.progress.set_opacity(0.0);
            self.inner.progress.set_visible(false);
            return;
        }
        let target = adw::PropertyAnimationTarget::new(&self.inner.progress, "opacity");
        let animation = crate::ui::motion::timed(
            &self.inner.progress,
            self.inner.progress.opacity(),
            0.0,
            crate::ui::motion::MICRO,
            target,
        );
        let progress = self.inner.progress.downgrade();
        let fade_generation = self.inner.fade_generation.clone();
        animation.connect_done(move |_| {
            if fade_generation.is_current(generation) {
                if let Some(progress) = progress.upgrade() {
                    progress.set_visible(false);
                }
            }
        });
        crate::ui::motion::replace_animation(&self.inner.fade, animation.clone());
        animation.play();
    }

    fn show(&self) {
        self.inner.fade_generation.start();
        self.cancel_fade();
        self.inner.progress.set_visible(true);
        self.inner.progress.set_opacity(1.0);
    }

    fn cancel_fade(&self) {
        if let Some(animation) = self.inner.fade.borrow_mut().take() {
            animation.skip();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_fraction_is_clamped_to_the_progress_range() {
        assert_eq!(clamped_fraction(-0.5), 0.0);
        assert_eq!(clamped_fraction(0.39), 0.39);
        assert_eq!(clamped_fraction(1.5), 1.0);
    }

    #[test]
    fn edge_css_is_two_pixels_with_the_approved_track_and_fill() {
        let css = super::super::scan_card_css::css();
        assert!(css.contains(".scan-edge-line trough"));
        assert!(css.contains("min-height: 2px"));
        assert!(css.contains("rgba(255, 255, 255, 0.10)"));
        assert!(css.contains("#2ec27e"));
    }

    #[test]
    fn starting_a_new_pulse_generation_stops_the_old_callback() {
        let generation = PulseGeneration::default();
        let first = generation.start();
        assert!(generation.is_current(first));

        let second = generation.start();
        assert!(!generation.is_current(first));
        assert!(generation.is_current(second));

        generation.cancel();
        assert!(!generation.is_current(second));
    }
}
