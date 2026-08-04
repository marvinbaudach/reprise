//! One visibility rule for everything a source row reveals on approach.
//!
//! Anything that appears on hover, focus, or selection goes through here.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;

#[derive(Default)]
struct RevealState {
    hovered: Cell<bool>,
    focused: Cell<bool>,
    selected: Cell<bool>,
}

type HoverCallback = Rc<dyn Fn(bool)>;

pub(in crate::ui) struct Reveal {
    state: Rc<RevealState>,
    target: gtk4::Widget,
    on_hover: Rc<RefCell<Option<HoverCallback>>>,
}

impl Reveal {
    pub(in crate::ui) fn install(
        host: &impl IsA<gtk4::Widget>,
        target: &impl IsA<gtk4::Widget>,
    ) -> Self {
        let reveal = Self {
            state: Rc::new(RevealState::default()),
            target: target.as_ref().clone(),
            on_hover: Rc::new(RefCell::new(None)),
        };

        let motion = gtk4::EventControllerMotion::new();
        let enter_state = reveal.state.clone();
        let enter_target = target.as_ref().downgrade();
        let enter_hover = reveal.on_hover.clone();
        motion.connect_enter(move |_, _, _| {
            enter_state.hovered.set(true);
            apply(&enter_state, &enter_target);
            notify(&enter_hover, true);
        });
        let leave_state = reveal.state.clone();
        let leave_target = target.as_ref().downgrade();
        let leave_hover = reveal.on_hover.clone();
        motion.connect_leave(move |_| {
            leave_state.hovered.set(false);
            apply(&leave_state, &leave_target);
            notify(&leave_hover, false);
        });
        host.as_ref().add_controller(motion);

        let focus_state = reveal.state.clone();
        let focus_target = target.as_ref().downgrade();
        target.as_ref().connect_has_focus_notify(move |widget| {
            focus_state.focused.set(widget.has_focus());
            apply(&focus_state, &focus_target);
        });

        apply(&reveal.state, &target.as_ref().downgrade());
        reveal
    }

    pub(in crate::ui) fn set_selected(&self, selected: bool) {
        self.state.selected.set(selected);
        apply(&self.state, &self.target.downgrade());
    }

    /// Lets a caller mirror the hover state elsewhere — the media column needs
    /// it for the selection checkbox, and a second motion controller on the
    /// same row is exactly the duplication this module exists to prevent.
    pub(in crate::ui) fn on_hover(&self, callback: impl Fn(bool) + 'static) {
        self.on_hover.replace(Some(Rc::new(callback)));
    }
}

fn apply(state: &RevealState, target: &gtk4::glib::WeakRef<gtk4::Widget>) {
    let Some(target) = target.upgrade() else {
        return;
    };
    let shown = state.hovered.get() || state.focused.get() || state.selected.get();
    target.set_opacity(if shown { 1.0 } else { 0.0 });
    target.set_can_target(shown);
}

fn notify(callback: &RefCell<Option<HoverCallback>>, hovered: bool) {
    let callback = callback.borrow().clone();
    if let Some(callback) = callback {
        callback(hovered);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SRC-17`: the button is invisible but its space is not. A row whose ⋮
    /// pops in and out by `visible` would shove its own title sideways under
    /// the pointer.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_17_revealing_keeps_the_space_and_only_changes_opacity() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();

        let host = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let target = gtk4::Button::new();
        host.append(&target);
        let reveal = Reveal::install(&host, &target);

        assert_eq!(target.opacity(), 0.0);
        assert!(
            target.is_visible(),
            "hidden by opacity, never by visibility"
        );

        reveal.set_selected(true);
        assert_eq!(target.opacity(), 1.0);

        reveal.set_selected(false);
        assert_eq!(target.opacity(), 0.0);
    }
}
