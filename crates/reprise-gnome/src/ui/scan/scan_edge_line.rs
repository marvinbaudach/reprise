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

    /// Brings the line up over the same Micro fade [`Self::hide`] leaves by.
    /// Every progress tick calls this, so a line that is already up stays
    /// untouched instead of restarting its fade.
    fn show(&self) {
        self.inner.fade_generation.start();
        self.inner.progress.set_visible(true);
        let from = self.inner.progress.opacity();
        if from >= 1.0 && !self.is_fading() {
            return;
        }
        if !crate::ui::motion::animations_enabled() {
            self.cancel_fade();
            self.inner.progress.set_opacity(1.0);
            return;
        }
        let target = adw::PropertyAnimationTarget::new(&self.inner.progress, "opacity");
        let animation = crate::ui::motion::timed(
            &self.inner.progress,
            from,
            1.0,
            crate::ui::motion::MICRO,
            target,
        );
        crate::ui::motion::replace_animation(&self.inner.fade, animation.clone());
        animation.play();
    }

    fn is_fading(&self) -> bool {
        self.inner
            .fade
            .borrow()
            .as_ref()
            .is_some_and(|animation| animation.state() == adw::AnimationState::Playing)
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

    /// Runs the main loop for `ms` so frame-clock driven animation advances.
    fn wait_ms(ms: u64) {
        let main_loop = glib::MainLoop::new(None, false);
        let quit = main_loop.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(ms), move || quit.quit());
        main_loop.run();
    }

    /// Pumps the main loop until `condition` holds or the deadline passes. The
    /// fade rides the frame clock, which needs a moment to spin up on a cold
    /// headless display — a fixed sleep of one Micro duration is not reliable.
    fn wait_until(condition: impl Fn() -> bool) -> bool {
        const SLICE_MS: u64 = 25;
        const DEADLINE_MS: u64 = 2_000;
        let mut waited = 0;
        while waited < DEADLINE_MS {
            if condition() {
                return true;
            }
            wait_ms(SLICE_MS);
            waited += SLICE_MS;
        }
        condition()
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fb_9_the_edge_line_arrives_by_fade_like_it_leaves() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let settings = gtk4::Settings::default().unwrap();
        let previous = settings.is_gtk_enable_animations();
        settings.set_gtk_enable_animations(true);

        let line = ScanEdgeLine::new();
        let window = gtk4::Window::builder()
            .default_width(420)
            .default_height(120)
            .child(line.widget())
            .build();
        window.present();

        line.set_fraction(0.39);
        // Up right away, but not yet opaque: the line arrives over the same
        // Micro fade it leaves by (FB-9), instead of snapping in.
        assert!(line.inner.progress.is_visible());
        assert!(
            line.inner.progress.opacity() < 1.0,
            "the edge line snapped to full opacity instead of fading in"
        );

        let progress = line.inner.progress.clone();
        assert!(
            wait_until(move || progress.opacity() >= 1.0),
            "the fade never reached full opacity"
        );

        // Every progress tick calls show(); once the line is up, that must not
        // restart the fade (MOT-6: no animation churn on a background update).
        line.set_fraction(0.51);
        assert_eq!(line.inner.progress.opacity(), 1.0);

        window.close();
        settings.set_gtk_enable_animations(previous);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mot_7_disabled_animations_show_the_edge_line_hard() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let settings = gtk4::Settings::default().unwrap();
        let previous = settings.is_gtk_enable_animations();
        settings.set_gtk_enable_animations(false);

        let line = ScanEdgeLine::new();
        line.set_fraction(0.39);
        assert!(line.inner.progress.is_visible());
        assert_eq!(line.inner.progress.opacity(), 1.0);

        settings.set_gtk_enable_animations(previous);
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
