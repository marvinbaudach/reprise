//! Moves a list's viewport to a target instead of teleporting it.
//!
//! The repository had no scroll animation at all: every centring of the loaded
//! track was a `set_value`, so the list jumped on every track change. This is
//! the same value written over `motion::STANDARD` — and it yields the moment
//! anything else touches the adjustment, because the viewport is the most
//! contested property in this widget (see `adjustment_hold.rs`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::AnimationExt;

use crate::ui::motion;

const MAX_GLIDE_PAGES: f64 = 3.0;
/// Same tolerance `adjustment_hold.rs` uses to decide a value is "still ours".
const VALUE_EPSILON: f64 = 0.5;

pub(in crate::ui) fn should_glide(distance: f64, page_size: f64) -> bool {
    page_size > 0.0 && distance.abs() <= MAX_GLIDE_PAGES * page_size
}

pub(in crate::ui) fn foreign_write(written: f64, observed: f64) -> bool {
    (written - observed).abs() > VALUE_EPSILON
}

pub(in crate::ui) struct ScrollGlide {
    inner: Rc<ScrollGlideInner>,
}

struct ScrollGlideInner {
    widget: gtk4::glib::WeakRef<gtk4::Widget>,
    animation: RefCell<Option<adw::TimedAnimation>>,
    last_written: Cell<f64>,
    generation: Cell<u64>,
}

impl ScrollGlide {
    pub(in crate::ui) fn new(widget: &impl IsA<gtk4::Widget>) -> Self {
        let weak = gtk4::glib::WeakRef::new();
        weak.set(Some(widget.upcast_ref::<gtk4::Widget>()));
        Self {
            inner: Rc::new(ScrollGlideInner {
                widget: weak,
                animation: RefCell::new(None),
                last_written: Cell::new(0.0),
                generation: Cell::new(0),
            }),
        }
    }

    /// Where a glide in flight is heading, or `None` when the viewport is at
    /// rest. A reload asks before capturing what to preserve: mid-glide, the
    /// value on screen is a waypoint, and preserving *that* would strand the
    /// follow halfway (see `track_list_reload::capture_reload_anchor`).
    pub(in crate::ui) fn destination(&self) -> Option<f64> {
        self.inner
            .animation
            .borrow()
            .as_ref()
            .map(adw::TimedAnimation::value_to)
    }

    pub(in crate::ui) fn glide_to(&self, adjustment: &gtk4::Adjustment, target: f64) {
        let current = adjustment.value();
        let distance = target - current;
        let Some(widget) = self.inner.widget.upgrade() else {
            cancel_animation(&self.inner);
            crate::ui::scroll_probe::probe("glide.no_widget", adjustment, target);
            adjustment.set_value(target);
            self.inner.last_written.set(adjustment.value());
            return;
        };
        if !motion::animations_enabled() || !should_glide(distance, adjustment.page_size()) {
            cancel_animation(&self.inner);
            crate::ui::scroll_probe::probe("glide.instant", adjustment, target);
            adjustment.set_value(target);
            self.inner.last_written.set(adjustment.value());
            return;
        }

        let generation = next_generation(&self.inner);
        self.inner.last_written.set(current);
        let inner = self.inner.clone();
        let adjustment_for_target = adjustment.clone();
        let animation_target = adw::CallbackAnimationTarget::new(move |value| {
            if inner.generation.get() != generation {
                return;
            }
            if foreign_write(inner.last_written.get(), adjustment_for_target.value()) {
                abort_generation(&inner, generation);
                return;
            }
            // Whole pixels only. The view floors the scroll offset to an
            // integer, but it does so *between* frames — reading the value
            // straight back after writing still returns the fraction we asked
            // for, so remembering either one made the next frame see a
            // difference of up to a pixel and abort. Measured: we wrote
            // 1568.748 and the following frame observed 1568.000, which is
            // past the 0.5 epsilon. Writing integers ourselves makes the
            // view's own flooring a no-op, and the epsilon then separates our
            // writes from a real third party cleanly. It also keeps the
            // viewport off half-pixels, which is where text renders soft.
            let value = value.round();
            crate::ui::scroll_probe::probe("glide.frame", &adjustment_for_target, value);
            adjustment_for_target.set_value(value);
            if inner.generation.get() == generation {
                inner.last_written.set(value);
            }
        });
        let animation = motion::timed(&widget, current, target, motion::STANDARD, animation_target);
        let inner = self.inner.clone();
        let adjustment_for_done = adjustment.clone();
        animation.connect_done(move |_| {
            if inner.generation.get() != generation {
                return;
            }
            if foreign_write(inner.last_written.get(), adjustment_for_done.value()) {
                abort_generation(&inner, generation);
                return;
            }
            crate::ui::scroll_probe::probe("glide.done", &adjustment_for_done, target);
            adjustment_for_done.set_value(target);
            if inner.generation.get() != generation {
                return;
            }
            inner.last_written.set(target);
            inner.animation.borrow_mut().take();
        });
        motion::replace_animation(&self.inner.animation, animation.clone());
        animation.play();
    }
}

fn next_generation(inner: &ScrollGlideInner) -> u64 {
    let generation = inner.generation.get().wrapping_add(1);
    inner.generation.set(generation);
    generation
}

fn cancel_animation(inner: &ScrollGlideInner) {
    next_generation(inner);
    let animation = inner.animation.borrow_mut().take();
    if let Some(animation) = animation {
        animation.skip();
    }
}

fn abort_generation(inner: &ScrollGlideInner, generation: u64) {
    if inner.generation.get() != generation {
        return;
    }
    cancel_animation(inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nav_10b_a_far_target_jumps_instead_of_gliding() {
        // Three viewport heights. Beyond that a glide is either absurdly slow
        // or a blur, and at launch the list is at 0 with the loaded track far
        // away — which is exactly how START-3 keeps its instant placement.
        let page = 600.0;
        assert!(should_glide(0.0, page));
        assert!(should_glide(page, page));
        assert!(should_glide(3.0 * page - 1.0, page));
        assert!(!should_glide(3.0 * page + 1.0, page));
        assert!(!should_glide(50_000.0, page));
        // A degenerate page size must not divide by zero into a glide.
        assert!(!should_glide(10.0, 0.0));
    }

    #[test]
    fn nav_10b_a_foreign_write_ends_the_glide() {
        // Anything that writes the adjustment beats the glide: the user
        // scrolling, `AdjustmentHold` restoring a value across a GTK handover,
        // or GTK resetting to zero after a model replacement. One rule covers
        // all three, and it is the caller's value that survives.
        assert!(!foreign_write(100.0, 100.4));
        assert!(foreign_write(100.0, 100.6));
        assert!(foreign_write(100.0, 0.0));
    }
}
