//! Shared focus lifecycle for modal dialogs and popovers.

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

#[derive(Clone)]
pub(super) struct TransientFocusGuard {
    invoker: glib::WeakRef<gtk4::Widget>,
    fallback: glib::WeakRef<gtk4::Widget>,
}

impl TransientFocusGuard {
    /// Captures the currently focused widget before a transient is opened.
    /// The parent is retained weakly as a stable fallback when the original
    /// row or button disappears while the transient is open.
    pub(super) fn capture<W: IsA<gtk4::Widget>>(parent: &W) -> Self {
        let fallback = parent.clone().upcast::<gtk4::Widget>();
        let invoker = fallback
            .root()
            .and_then(|root| root.focus())
            .unwrap_or_else(|| fallback.clone());
        Self {
            invoker: invoker.downgrade(),
            fallback: fallback.downgrade(),
        }
    }

    /// Applies initial focus after libadwaita has mapped the dialog and
    /// restores the captured invoker after its close animation completes.
    pub(super) fn bind_dialog<W: IsA<gtk4::Widget>>(&self, dialog: &adw::Dialog, initial: &W) {
        dialog.set_focus(Some(initial));
        let initial = initial.clone().upcast::<gtk4::Widget>().downgrade();
        dialog.connect_map(move |dialog| {
            if let Some(initial) = initial.upgrade() {
                dialog.set_focus(Some(&initial));
            }
        });
        let guard = self.clone();
        dialog.connect_closed(move |_| guard.restore());
    }

    pub(super) fn bind_closable_dialog<W: IsA<gtk4::Widget>>(
        &self,
        dialog: &adw::Dialog,
        initial: &W,
    ) {
        self.bind_dialog(dialog, initial);
        wire_close_shortcut(dialog);
    }

    pub(super) fn restore_on_dialog_close(&self, dialog: &adw::Dialog) {
        let guard = self.clone();
        dialog.connect_closed(move |_| guard.restore());
    }

    pub(super) fn close_on_control_w(&self, dialog: &adw::Dialog) {
        wire_close_shortcut(dialog);
    }

    /// Popovers use the same contract as dialogs. Native popover focus
    /// containment remains responsible for Tab and Escape handling.
    pub(super) fn bind_popover<W: IsA<gtk4::Widget>>(&self, popover: &gtk4::Popover, initial: &W) {
        let initial = initial.clone().upcast::<gtk4::Widget>().downgrade();
        popover.connect_map(move |_| {
            if let Some(initial) = initial.upgrade() {
                initial.grab_focus();
            }
        });
        let guard = self.clone();
        popover.connect_closed(move |_| guard.restore());
    }

    pub(super) fn restore_on_popover_close(&self, popover: &gtk4::Popover) {
        let guard = self.clone();
        popover.connect_closed(move |_| guard.restore());
    }

    /// Schedules restoration outside the current response/close signal so
    /// GTK can finish removing the transient from the focus chain first.
    pub(super) fn restore(&self) {
        let guard = self.clone();
        glib::idle_add_local_once(move || {
            if try_focus(&guard.invoker) {
                return;
            }
            if let Some(fallback) = guard.fallback.upgrade() {
                if !fallback.grab_focus() {
                    fallback.child_focus(gtk4::DirectionType::TabForward);
                }
            }
        });
    }
}

fn try_focus(target: &glib::WeakRef<gtk4::Widget>) -> bool {
    target.upgrade().is_some_and(|target| {
        if !target.is_visible() || !target.is_sensitive() {
            return false;
        }
        let Some(root) = target.root() else {
            return false;
        };
        let Ok(window) = root.downcast::<gtk4::Window>() else {
            return false;
        };
        gtk4::prelude::GtkWindowExt::set_focus(&window, Some(&target));
        gtk4::prelude::GtkWindowExt::focus(&window).is_some_and(|focus| focus == target)
    })
}

pub(super) fn is_close_shortcut(key: gtk4::gdk::Key, modifiers: gtk4::gdk::ModifierType) -> bool {
    modifiers.contains(gtk4::gdk::ModifierType::CONTROL_MASK)
        && matches!(key, gtk4::gdk::Key::w | gtk4::gdk::Key::W)
}

fn wire_close_shortcut(dialog: &adw::Dialog) {
    let keys = gtk4::EventControllerKey::new();
    let dialog_weak = dialog.downgrade();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if !is_close_shortcut(key, modifiers) {
            return glib::Propagation::Proceed;
        }
        if let Some(dialog) = dialog_weak.upgrade() {
            dialog.close();
        }
        glib::Propagation::Stop
    });
    dialog.add_controller(keys);
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;
    use libadwaita::prelude::*;

    #[test]
    fn control_w_is_the_only_transient_close_shortcut() {
        use gtk4::gdk::{Key, ModifierType};

        assert!(super::is_close_shortcut(Key::w, ModifierType::CONTROL_MASK));
        assert!(super::is_close_shortcut(Key::W, ModifierType::CONTROL_MASK));
        assert!(!super::is_close_shortcut(Key::w, ModifierType::empty()));
        assert!(!super::is_close_shortcut(
            Key::q,
            ModifierType::CONTROL_MASK
        ));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn guard_focuses_the_initial_widget_and_restores_the_invoker() {
        gtk4::init().unwrap();
        let window = gtk4::Window::new();
        let invoker = gtk4::Button::with_label("Open");
        window.set_child(Some(&invoker));
        window.present();
        assert!(invoker.grab_focus());

        let initial = gtk4::Entry::new();
        let dialog = libadwaita::Dialog::builder().child(&initial).build();
        let guard = super::TransientFocusGuard::capture(&window);
        guard.bind_dialog(&dialog, &initial);
        dialog.present(Some(&window));
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert_eq!(
            libadwaita::prelude::AdwDialogExt::focus(&dialog),
            Some(initial.clone().upcast())
        );

        dialog.close();
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert_eq!(
            gtk4::prelude::GtkWindowExt::focus(&window),
            Some(invoker.clone().upcast())
        );
        window.close();
    }
}
