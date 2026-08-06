//! One visibility rule for everything a source row reveals on approach.
//!
//! Anything that appears on hover, focus, or selection goes through here.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

#[derive(Default)]
struct RevealState {
    hovered: Cell<bool>,
    focused: Cell<bool>,
    selected: Cell<bool>,
}

pub(in crate::ui) struct Reveal {
    state: Rc<RevealState>,
    target: gtk4::Widget,
    /// Focus is watched in two places and either one counts, so each needs its
    /// own memory — a single flag would let the row losing focus to its own
    /// child immediately hide the control that just gained it.
    focused_host: Rc<Cell<bool>>,
    focused_target: Rc<Cell<bool>>,
}

impl Reveal {
    pub(in crate::ui) fn install(
        host: &impl IsA<gtk4::Widget>,
        target: &impl IsA<gtk4::Widget>,
    ) -> Self {
        let reveal = Self {
            state: Rc::new(RevealState::default()),
            target: target.as_ref().clone(),
            focused_host: Rc::new(Cell::new(false)),
            focused_target: Rc::new(Cell::new(false)),
        };

        let motion = gtk4::EventControllerMotion::new();
        let enter_state = reveal.state.clone();
        let enter_target = target.as_ref().downgrade();
        motion.connect_enter(move |_, _, _| {
            enter_state.hovered.set(true);
            apply(&enter_state, &enter_target);
        });
        let leave_state = reveal.state.clone();
        let leave_target = target.as_ref().downgrade();
        motion.connect_leave(move |_| {
            leave_state.hovered.set(false);
            apply(&leave_state, &leave_target);
        });
        host.as_ref().add_controller(motion);

        // Both the row and the revealed control itself. A container's
        // `has-focus` stays false when the focus moves onto one of its
        // children — it does not bubble — so watching only the host leaves a
        // keyboard user tabbed onto an invisible, untargetable button. The
        // hover star this rule was generalised from wired the signal on the
        // target for exactly that reason; losing it in the generalisation is
        // how a shared rule ends up worse than the thing it replaced.
        let host_state = reveal.state.clone();
        let host_target = target.as_ref().downgrade();
        let host_focus = reveal.focused_host.clone();
        let host_focused_target = reveal.focused_target.clone();
        host.as_ref().connect_has_focus_notify(move |widget| {
            host_focus.set(widget.has_focus());
            host_state
                .focused
                .set(host_focus.get() || host_focused_target.get());
            apply(&host_state, &host_target);
        });

        let target_state = reveal.state.clone();
        let target_weak = target.as_ref().downgrade();
        let target_focus = reveal.focused_target.clone();
        let target_focused_host = reveal.focused_host.clone();
        target.as_ref().connect_has_focus_notify(move |widget| {
            target_focus.set(widget.has_focus());
            target_state
                .focused
                .set(target_focus.get() || target_focused_host.get());
            apply(&target_state, &target_weak);
        });

        apply(&reveal.state, &target.as_ref().downgrade());
        reveal
    }

    pub(in crate::ui) fn set_selected(&self, selected: bool) {
        self.state.selected.set(selected);
        apply(&self.state, &self.target.downgrade());
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

    /// `SRC-17`: tabbing straight onto the revealed control must reveal it.
    ///
    /// A container's `has-focus` stays false while the focus sits on one of
    /// its children, so watching only the row hands a keyboard user an
    /// invisible, untargetable button — reachable by Tab, impossible to see.
    /// The hover star this rule generalises from watched the target itself;
    /// this test is what keeps that from being generalised away again.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_17_focusing_the_control_itself_reveals_it() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();

        let window = gtk4::Window::new();
        let host = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let target = gtk4::Button::new();
        // Somewhere else to stand. Without it GTK hands the initial focus to
        // the only focusable widget there is — the target — and the row has no
        // idle state to start from.
        let elsewhere = gtk4::Button::new();
        host.append(&elsewhere);
        host.append(&target);
        window.set_child(Some(&host));
        let reveal = Reveal::install(&host, &target);
        window.present();
        // `has_focus()` is `is_focus() && window.is_active()`, and X grants the
        // activation asynchronously — without this wait the button takes the
        // focus, `has_focus()` still says no, and the test reads as a product
        // bug instead of a timing one.
        crate::ui::test_main_context::settle_until_active(&window);

        elsewhere.grab_focus();
        assert_eq!(target.opacity(), 0.0, "idle row");

        target.grab_focus();
        assert!(target.has_focus(), "the button really took the focus");
        assert_eq!(
            target.opacity(),
            1.0,
            "a focused control that stays invisible cannot be operated"
        );
        assert!(
            target.can_target(),
            "and it must be operable once it is visible"
        );

        // Losing focus puts it back, which is the half a single shared flag
        // would get wrong in the other direction.
        elsewhere.grab_focus();
        assert_eq!(target.opacity(), 0.0);

        drop(reveal);
    }
}
