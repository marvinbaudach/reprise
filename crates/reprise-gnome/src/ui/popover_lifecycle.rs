use gtk4::prelude::*;

/// Defers unparenting until the current main-loop dispatch has completed.
/// Menu-item activation and `closed` can occur in the same signal chain;
/// removing the popover synchronously can detach its inherited action group
/// before the selected action is dispatched.
pub(super) fn unparent_after_actions(popover: &gtk4::Popover) {
    popover.connect_closed(|popover| {
        let popover = popover.downgrade();
        gtk4::glib::idle_add_local_once(move || {
            let Some(popover) = popover.upgrade() else {
                return;
            };
            if popover.parent().is_some() {
                popover.unparent();
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn closed_popover_stays_parented_until_pending_actions_finish() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        if gtk4::init().is_err() {
            return;
        }
        let parent = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let popover = gtk4::Popover::new();
        popover.set_parent(&parent);
        super::unparent_after_actions(&popover);

        popover.emit_by_name::<()>("closed", &[]);
        assert!(popover.parent().is_some());

        let context = gtk4::glib::MainContext::default();
        while context.pending() {
            context.iteration(false);
        }
        assert!(popover.parent().is_none());
    }
}
