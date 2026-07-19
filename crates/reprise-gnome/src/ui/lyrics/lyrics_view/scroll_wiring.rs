//! GTK event-controller and animation adapter for [`LyricsView`].

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::AnimationExt;

use super::{centered_scroll_value, LyricsView};
use crate::ui::lyrics::lyrics_scroll::{content_margins, PauseHandle, USER_PAUSE_MS};

impl LyricsView {
    pub(super) fn wire_scroll_input(self: &Rc<Self>) {
        // input-parity: ACC-8 keyboard=native-scroll
        let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
        scroll.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let view = Rc::downgrade(self);
        scroll.connect_scroll(move |_, _, _| {
            if let Some(view) = view.upgrade() {
                view.handle_user_scroll();
            }
            gtk4::glib::Propagation::Proceed
        });
        self.scrolled.add_controller(scroll);

        // input-parity: ACC-8 keyboard=native-scroll
        let drag = gtk4::GestureDrag::new();
        drag.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let view = Rc::downgrade(self);
        drag.connect_drag_begin(move |_, _, _| {
            if let Some(view) = view.upgrade() {
                view.handle_user_scroll();
            }
        });
        let view = Rc::downgrade(self);
        drag.connect_drag_update(move |_, _, _| {
            if let Some(view) = view.upgrade() {
                view.handle_user_scroll();
            }
        });
        self.scrolled.add_controller(drag);
    }

    pub(super) fn handle_user_scroll(self: &Rc<Self>) {
        self.cancel_pause_timer();
        self.cancel_scroll_animation();
        let handle = self
            .scroll_state
            .borrow_mut()
            .user_scroll(self.scroll_timer.now_ms());
        self.schedule_pause(handle, USER_PAUSE_MS);
    }

    fn schedule_pause(self: &Rc<Self>, handle: PauseHandle, delay_ms: u64) {
        let view = Rc::downgrade(self);
        let timer = self.scroll_timer.schedule(
            delay_ms,
            Box::new(move || {
                if let Some(view) = view.upgrade() {
                    view.pause_timer_elapsed(handle);
                }
            }),
        );
        *self.pause_timer.borrow_mut() = Some(timer);
    }

    fn pause_timer_elapsed(self: &Rc<Self>, handle: PauseHandle) {
        self.pause_timer.borrow_mut().take();
        let now_ms = self.scroll_timer.now_ms();
        if !self.scroll_state.borrow_mut().timer_elapsed(handle, now_ms) {
            let remaining = self.scroll_state.borrow().remaining_pause_ms(now_ms);
            if remaining > 0 {
                self.schedule_pause(handle, remaining);
            }
            return;
        }
        if let Some(label) = self.active_label() {
            self.scroll_to_label(&label, true);
        } else {
            self.scroll_state.borrow_mut().return_finished();
        }
    }

    pub(super) fn cancel_pause_timer(&self) {
        if let Some(timer) = self.pause_timer.borrow_mut().take() {
            timer.cancel();
        }
    }

    pub(super) fn activate_line(self: &Rc<Self>, index: usize) {
        let line = self.lines.borrow().get(index).cloned();
        let Some(line) = line else {
            return;
        };
        let Some(timestamp_ms) = line.timestamp_ms else {
            return;
        };
        self.cancel_pause_timer();
        self.cancel_scroll_animation();
        self.scroll_state.borrow_mut().external_seek();
        self.scroll_to_label(&line.label, true);
        let callback = self.on_seek.borrow().clone();
        if let Some(callback) = callback {
            callback(timestamp_ms);
        }
    }

    pub(super) fn active_label(&self) -> Option<gtk4::Label> {
        self.active_line.get().and_then(|index| {
            self.lines
                .borrow()
                .get(index)
                .map(|line| line.label.clone())
        })
    }

    pub(super) fn scroll_to_label(self: &Rc<Self>, label: &gtk4::Label, animated: bool) {
        if !self.scroll_state.borrow().should_follow_active_line() {
            return;
        }
        let label = label.clone();
        let view = Rc::downgrade(self);
        gtk4::glib::idle_add_local_once(move || {
            let Some(view) = view.upgrade() else {
                return;
            };
            if !view.scroll_state.borrow().should_follow_active_line() {
                return;
            }
            let (top, bottom) = content_margins(view.scrolled.height(), label.height());
            view.content.set_margin_top(top);
            view.content.set_margin_bottom(bottom);
            let view = Rc::downgrade(&view);
            gtk4::glib::idle_add_local_once(move || {
                if let Some(view) = view.upgrade() {
                    if view.scroll_state.borrow().should_follow_active_line() {
                        view.begin_center_scroll(&label, animated);
                    }
                }
            });
        });
    }

    fn begin_center_scroll(self: &Rc<Self>, label: &gtk4::Label, animated: bool) {
        let adjustment = self.scrolled.vadjustment();
        let target = {
            let Some(point) =
                label.compute_point(&self.scrolled, &gtk4::graphene::Point::new(0.0, 0.0))
            else {
                return;
            };
            centered_scroll_value(
                adjustment.value() + f64::from(point.y()),
                f64::from(label.height()),
                adjustment.page_size(),
                adjustment.upper(),
            )
        };
        if !animated || !crate::ui::motion::animations_enabled() {
            self.cancel_scroll_animation();
            adjustment.set_value(target);
            self.scroll_state.borrow_mut().return_finished();
            return;
        }
        if (adjustment.value() - target).abs() < f64::EPSILON {
            self.scroll_state.borrow_mut().return_finished();
            return;
        }

        self.cancel_scroll_animation();
        let generation = self.scroll_animation_generation.get().wrapping_add(1);
        self.scroll_animation_generation.set(generation);
        let animation_target = adw::CallbackAnimationTarget::new({
            let adjustment = adjustment.clone();
            move |value| adjustment.set_value(value)
        });
        let animation = crate::ui::motion::timed(
            &self.scrolled,
            adjustment.value(),
            target,
            crate::ui::motion::STANDARD,
            animation_target,
        );
        let view = Rc::downgrade(self);
        animation.connect_done(move |_| {
            let Some(view) = view.upgrade() else {
                return;
            };
            if view.scroll_animation_generation.get() != generation {
                return;
            }
            view.scroll_animation.borrow_mut().take();
            view.scroll_state.borrow_mut().return_finished();
        });
        *self.scroll_animation.borrow_mut() = Some(animation.clone());
        animation.play();
    }

    pub(super) fn cancel_scroll_animation(&self) {
        self.scroll_animation_generation
            .set(self.scroll_animation_generation.get().wrapping_add(1));
        let animation = self.scroll_animation.borrow_mut().take();
        if let Some(animation) = animation {
            animation.pause();
        }
    }
}
