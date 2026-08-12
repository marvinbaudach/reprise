use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_view::search_scope::SearchScope;

type FocusCallback = Rc<dyn Fn() -> bool>;
type AbortCallback = Rc<dyn Fn()>;

#[derive(Clone)]
pub(in crate::ui) struct SearchPopover {
    popover: gtk4::Popover,
    entry: gtk4::SearchEntry,
    scope_label: gtk4::Label,
    focus_on_close: Rc<RefCell<Option<FocusCallback>>>,
    abort_on_escape: Rc<RefCell<Option<AbortCallback>>>,
}

#[derive(Clone)]
pub(in crate::ui) struct WeakSearchPopover {
    popover: gtk4::glib::WeakRef<gtk4::Popover>,
    entry: gtk4::glib::WeakRef<gtk4::SearchEntry>,
    scope_label: gtk4::glib::WeakRef<gtk4::Label>,
}

impl SearchPopover {
    pub(in crate::ui) fn new(lens: &gtk4::ToggleButton, entry: &gtk4::SearchEntry) -> Self {
        let scope_label = gtk4::Label::builder()
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .build();
        scope_label.add_css_class("reprise-search-popover-caption");
        let dismiss_label = gtk4::Label::builder()
            .label(crate::ui::strings::text(crate::ui::strings::ESC_TO_CLOSE))
            .halign(gtk4::Align::End)
            .build();
        dismiss_label.add_css_class("reprise-search-popover-caption");

        let caption = gtk4::Box::new(gtk4::Orientation::Horizontal, 7);
        caption.add_css_class("reprise-search-popover-caption-row");
        caption.append(&scope_label);
        caption.append(&dismiss_label);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 7);
        content.append(entry);
        content.append(&caption);

        let popover = gtk4::Popover::new();
        popover.add_css_class("reprise-search-popover");
        popover.set_child(Some(&content));
        popover.set_parent(lens);
        popover.set_autohide(true);
        popover.set_has_arrow(false);
        popover.set_position(gtk4::PositionType::Bottom);
        popover.set_halign(gtk4::Align::End);

        lens.connect_destroy({
            let popover = popover.clone();
            move |_| {
                if popover.parent().is_some() {
                    popover.unparent();
                }
            }
        });

        let focus_on_close: Rc<RefCell<Option<FocusCallback>>> = Rc::new(RefCell::new(None));
        let abort_on_escape: Rc<RefCell<Option<AbortCallback>>> = Rc::new(RefCell::new(None));
        // SEARCH-2c: closing returns focus to the list — on *every* close path.
        // This hangs on `closed` rather than on the explicit `close()` because
        // GTK's own autohide (a click outside) never runs our close helper: it
        // calls `popdown` itself. Wiring focus to the signal is what makes the
        // outside click behave like Escape instead of leaving the keyboard in a
        // popover that is no longer on screen.
        popover.connect_closed({
            let focus_on_close = Rc::clone(&focus_on_close);
            move |_| return_focus(&focus_on_close)
        });
        wire_entry_close(entry, &popover, &abort_on_escape);

        Self {
            popover,
            entry: entry.clone(),
            scope_label,
            focus_on_close,
            abort_on_escape,
        }
    }

    /// Test-only: production drives the popover through this type's own API,
    /// so handing out the raw widget would only invite a second close path.
    #[cfg(test)]
    pub(in crate::ui) fn widget(&self) -> &gtk4::Popover {
        &self.popover
    }

    pub(in crate::ui) fn entry(&self) -> &gtk4::SearchEntry {
        &self.entry
    }

    pub(in crate::ui) fn is_open(&self) -> bool {
        self.popover.is_visible()
    }

    pub(in crate::ui) fn open(&self) {
        self.downgrade().open();
    }

    pub(in crate::ui) fn close(&self) {
        self.downgrade().close();
    }

    /// Test-only: `SectionSearch` holds the weak handle and sets the scope
    /// through that.
    #[cfg(test)]
    pub(in crate::ui) fn set_scope(&self, scope: SearchScope) {
        self.downgrade().set_scope(scope);
    }

    #[cfg(test)]
    pub(in crate::ui) fn scope_text(&self) -> gtk4::glib::GString {
        self.scope_label.text()
    }

    #[cfg(test)]
    pub(in crate::ui) fn press_close_key(&self, key: gtk4::gdk::Key) -> gtk4::glib::Propagation {
        handle_search_key(
            key,
            &self.popover.downgrade(),
            &self.entry.downgrade(),
            &self.abort_on_escape,
        )
    }

    pub(in crate::ui) fn connect_open_changed(&self, f: impl Fn(bool) + 'static) {
        let callback = Rc::new(f);
        let opened = Rc::clone(&callback);
        self.popover.connect_show(move |_| opened(true));
        self.popover.connect_closed(move |_| callback(false));
    }

    pub(in crate::ui) fn set_focus_on_close(&self, callback: FocusCallback) {
        self.focus_on_close.replace(Some(callback));
    }

    pub(in crate::ui) fn set_abort_on_escape(&self, callback: AbortCallback) {
        self.abort_on_escape.replace(Some(callback));
    }

    pub(in crate::ui) fn downgrade(&self) -> WeakSearchPopover {
        WeakSearchPopover {
            popover: self.popover.downgrade(),
            entry: self.entry.downgrade(),
            scope_label: self.scope_label.downgrade(),
        }
    }
}

impl WeakSearchPopover {
    pub(in crate::ui) fn is_open(&self) -> bool {
        self.popover
            .upgrade()
            .is_some_and(|popover| popover.is_visible())
    }

    pub(in crate::ui) fn open(&self) {
        let (Some(popover), Some(entry)) = (self.popover.upgrade(), self.entry.upgrade()) else {
            return;
        };
        popover.popup();
        entry.grab_focus();
        entry.set_position(-1);
    }

    pub(in crate::ui) fn close(&self) {
        close_popover(&self.popover);
    }

    pub(in crate::ui) fn set_scope(&self, scope: SearchScope) {
        if let Some(label) = self.scope_label.upgrade() {
            label.set_label(&crate::ui::filter_bar_strings::searches_scope(scope));
        }
    }
}

fn wire_entry_close(
    entry: &gtk4::SearchEntry,
    popover: &gtk4::Popover,
    abort_on_escape: &Rc<RefCell<Option<AbortCallback>>>,
) {
    let keys = gtk4::EventControllerKey::new();
    keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let popover_weak = popover.downgrade();
    let entry_weak = entry.downgrade();
    let abort = Rc::clone(abort_on_escape);
    keys.connect_key_pressed(move |_, key, _, _| {
        handle_search_key(key, &popover_weak, &entry_weak, &abort)
    });
    entry.add_controller(keys);

    let popover_weak = popover.downgrade();
    let entry_weak = entry.downgrade();
    let abort = Rc::clone(abort_on_escape);
    entry.connect_stop_search(move |_| {
        abort_search(&entry_weak, &abort);
        close_popover(&popover_weak);
    });
}

fn handle_search_key(
    key: gtk4::gdk::Key,
    popover: &gtk4::glib::WeakRef<gtk4::Popover>,
    entry: &gtk4::glib::WeakRef<gtk4::SearchEntry>,
    abort_on_escape: &Rc<RefCell<Option<AbortCallback>>>,
) -> gtk4::glib::Propagation {
    match key {
        gtk4::gdk::Key::Escape => abort_search(entry, abort_on_escape),
        gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter => {}
        _ => return gtk4::glib::Propagation::Proceed,
    }
    close_popover(popover);
    gtk4::glib::Propagation::Stop
}

/// Escape uses the active section's clear path when the shell has installed
/// one. The direct entry clear is only a lifecycle-safe fallback for isolated
/// search widgets (including tests) whose coordinator no longer exists.
fn abort_search(
    entry: &gtk4::glib::WeakRef<gtk4::SearchEntry>,
    abort_on_escape: &Rc<RefCell<Option<AbortCallback>>>,
) {
    let callback = abort_on_escape.borrow().clone();
    if let Some(callback) = callback {
        callback();
    } else if let Some(entry) = entry.upgrade() {
        entry.set_text("");
    }
}

fn close_popover(popover: &gtk4::glib::WeakRef<gtk4::Popover>) {
    let Some(popover) = popover.upgrade() else {
        return;
    };
    if !popover.is_visible() {
        return;
    }
    // Focus is not returned here on purpose: `popdown` emits `closed`, and the
    // handler on that signal owns the focus contract for every path.
    popover.popdown();
}

/// The borrow is dropped before the callback runs: it can move focus, which
/// re-enters GTK, and a live `RefCell` borrow across that is how this codebase
/// has produced `BorrowMutError` panics before.
fn return_focus(focus_on_close: &Rc<RefCell<Option<FocusCallback>>>) {
    let callback = focus_on_close.borrow().clone();
    if let Some(callback) = callback {
        if !callback() {
            tracing::warn!("search close: could not move focus to the active content view");
        }
    }
}

#[cfg(test)]
#[path = "search_popover_tests.rs"]
mod tests;
