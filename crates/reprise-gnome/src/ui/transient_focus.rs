//! Shared focus lifecycle for modal dialogs and popovers.

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

#[derive(Clone)]
pub(super) struct TransientFocusGuard {
    invoker: glib::WeakRef<gtk4::Widget>,
    fallback: glib::WeakRef<gtk4::Widget>,
    row: glib::WeakRef<gtk4::Widget>,
    rows_at_capture: u32,
}

impl TransientFocusGuard {
    /// Captures the currently focused widget before a transient is opened.
    /// The parent is retained weakly as a stable fallback when the original
    /// row or button disappears while the transient is open.
    pub(super) fn capture<W: IsA<gtk4::Widget>>(parent: &W) -> Self {
        let fallback = parent.clone().upcast::<gtk4::Widget>();
        let focused = fallback.root().and_then(|root| root.focus());
        let row = focused.as_ref().and_then(enclosing_row);
        let invoker = focused
            .as_ref()
            .map_or_else(|| fallback.clone(), stable_focus_target);
        let rows_at_capture = row.as_ref().and_then(|_| row_count(&invoker)).unwrap_or(0);
        Self {
            invoker: invoker.downgrade(),
            fallback: fallback.downgrade(),
            row: row.map_or_else(glib::WeakRef::new, |row| row.downgrade()),
            rows_at_capture,
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
    ///
    /// Restoring focus must not move what the user is looking at, which is
    /// why a list view is focused through [`focus_visible_row`] rather than
    /// through plain `set_focus`.
    pub(super) fn restore(&self) {
        let guard = self.clone();
        glib::idle_add_local_once(move || {
            if let (Some(row), Some(invoker)) = (guard.row.upgrade(), guard.invoker.upgrade()) {
                if row.is_visible() && row_count(&invoker) == Some(guard.rows_at_capture) {
                    row.grab_focus();
                    return;
                }
            }
            let landed_on_a_row = guard
                .invoker
                .upgrade()
                .filter(|invoker| invoker.is_visible() && invoker.is_sensitive())
                .is_some_and(|invoker| focus_visible_row(&invoker));
            if landed_on_a_row || try_focus(&guard.invoker) {
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

fn is_row_widget(widget: &gtk4::Widget) -> bool {
    matches!(
        widget.type_().name(),
        "GtkColumnViewRowWidget" | "GtkListItemWidget"
    )
}

fn enclosing_row(widget: &gtk4::Widget) -> Option<gtk4::Widget> {
    let mut node = Some(widget.clone());
    while let Some(current) = node {
        if is_row_widget(&current) {
            return Some(current);
        }
        node = current.parent();
    }
    None
}

#[cfg(test)]
pub(in crate::ui) fn is_row_widget_for_test(widget: &gtk4::Widget) -> bool {
    is_row_widget(widget)
}

/// The widget worth remembering across a transient's lifetime, given the one
/// that currently has focus.
///
/// Rows of a `GtkColumnView`/`GtkListView`/`GtkGridView` are **recycled**:
/// GTK rebinds the very same widget to a different row whenever the model
/// changes. A `WeakRef` to the focused row therefore stays alive while
/// silently coming to mean a different track — and focusing it again scrolls
/// the list to wherever that widget now sits. The Tag Editor is exactly this
/// case: its save rebuilds the model (`items_changed(0, old, new)`) while the
/// dialog is still closing, so restoring focus afterwards regularly threw the
/// library to the top and selected whatever row the recycled widget had been
/// handed (TAG-1: a save moves neither scroll nor view).
///
/// The list itself is not recycled and is therefore the stable fallback to
/// remember. [`TransientFocusGuard`] separately retains the row only while
/// the model's cardinality is unchanged, so a cardinality-changing re-query
/// cannot restore a recycled widget after rebinding it to another track.
fn stable_focus_target(focused: &gtk4::Widget) -> gtk4::Widget {
    let mut list = None;
    let mut node = Some(focused.clone());
    while let Some(current) = node {
        if current.is::<gtk4::ColumnView>()
            || current.is::<gtk4::ListView>()
            || current.is::<gtk4::GridView>()
        {
            // Keep walking: a `GtkColumnView` reaches its rows through an
            // internal `GtkListView`, and the public view is the better
            // anchor of the two.
            list = Some(current.clone());
        }
        node = current.parent();
    }
    list.unwrap_or_else(|| focused.clone())
}

/// How many rows `view` holds, if it is one of the list views.
fn row_count(view: &gtk4::Widget) -> Option<u32> {
    if let Some(view) = view.downcast_ref::<gtk4::ColumnView>() {
        return view.model().map(|model| model.n_items());
    }
    if let Some(view) = view.downcast_ref::<gtk4::ListView>() {
        return view.model().map(|model| model.n_items());
    }
    view.downcast_ref::<gtk4::GridView>()?
        .model()
        .map(|model| model.n_items())
}

/// Moves keyboard focus onto the row crossing `view`'s vertical middle and
/// reports whether that worked.
///
/// Plain `set_focus` on a list view is not enough, and is in fact the whole
/// problem: the view hands focus to *its* focus row, and a view whose model
/// was replaced while the transient was open (`items_changed(0, old, new)`,
/// which is what every re-query emits) has that row reset to the top. GTK
/// then dutifully reveals row zero and the library jumps — TAG-1's "a save
/// moves neither scroll nor view", violated by the focus restore rather than
/// by the save.
///
/// Walking the realized widget tree avoids estimating a row from average
/// height, whose error grows with the scroll offset. A row crossing the
/// viewport's middle is already fully visible, so focusing it needs neither a
/// `scroll_to` anchor nor an adjustment hold.
fn focus_visible_row(view: &gtk4::Widget) -> bool {
    if row_count(view).is_none_or(|rows| rows == 0) {
        return false;
    }

    let middle = view.height() as f32 / 2.0;
    let mut pending = vec![view.clone()];
    while let Some(widget) = pending.pop() {
        if is_row_widget(&widget)
            && widget.compute_bounds(view).is_some_and(|bounds| {
                bounds.y() <= middle && bounds.y() + bounds.height() >= middle
            })
        {
            return widget.grab_focus();
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
    false
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
        // A container hands focus on to a child — a list view focuses its
        // focus row — so the window reports that child, not the target. That
        // is a success, not a miss: treating it as one would drop through to
        // the fallback and focus something unrelated.
        gtk4::prelude::GtkWindowExt::focus(&window)
            .is_some_and(|focus| focus == target || focus.is_ancestor(&target))
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
