use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_view::search_scope::SearchScope;

type FocusCallback = Rc<dyn Fn() -> bool>;

#[derive(Clone)]
pub(in crate::ui) struct SearchPopover {
    popover: gtk4::Popover,
    entry: gtk4::SearchEntry,
    scope_label: gtk4::Label,
    focus_on_close: Rc<RefCell<Option<FocusCallback>>>,
}

#[derive(Clone)]
pub(in crate::ui) struct WeakSearchPopover {
    popover: gtk4::glib::WeakRef<gtk4::Popover>,
    entry: gtk4::glib::WeakRef<gtk4::SearchEntry>,
    scope_label: gtk4::glib::WeakRef<gtk4::Label>,
    focus_on_close: Rc<RefCell<Option<FocusCallback>>>,
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

        let focus_on_close = Rc::new(RefCell::new(None));
        wire_entry_close(entry, &popover, &focus_on_close);

        Self {
            popover,
            entry: entry.clone(),
            scope_label,
            focus_on_close,
        }
    }

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

    pub(in crate::ui) fn set_scope(&self, scope: SearchScope) {
        self.downgrade().set_scope(scope);
    }

    #[cfg(test)]
    pub(in crate::ui) fn scope_text(&self) -> gtk4::glib::GString {
        self.scope_label.text()
    }

    #[cfg(test)]
    pub(in crate::ui) fn press_close_key(&self, key: gtk4::gdk::Key) -> gtk4::glib::Propagation {
        handle_close_key(key, &self.downgrade())
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

    pub(in crate::ui) fn downgrade(&self) -> WeakSearchPopover {
        WeakSearchPopover {
            popover: self.popover.downgrade(),
            entry: self.entry.downgrade(),
            scope_label: self.scope_label.downgrade(),
            focus_on_close: Rc::clone(&self.focus_on_close),
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
        close_and_focus(&self.popover, Rc::clone(&self.focus_on_close));
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
    focus_on_close: &Rc<RefCell<Option<FocusCallback>>>,
) {
    let keys = gtk4::EventControllerKey::new();
    keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let search = WeakSearchPopover {
        popover: popover.downgrade(),
        entry: entry.downgrade(),
        scope_label: gtk4::glib::WeakRef::new(),
        focus_on_close: Rc::clone(focus_on_close),
    };
    keys.connect_key_pressed(move |_, key, _, _| handle_close_key(key, &search));
    entry.add_controller(keys);

    let popover_weak = popover.downgrade();
    let focus = Rc::clone(focus_on_close);
    entry.connect_stop_search(move |_| {
        close_and_focus(&popover_weak, Rc::clone(&focus));
    });
}

fn handle_close_key(key: gtk4::gdk::Key, search: &WeakSearchPopover) -> gtk4::glib::Propagation {
    if !matches!(
        key,
        gtk4::gdk::Key::Escape | gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter
    ) {
        return gtk4::glib::Propagation::Proceed;
    }
    search.close();
    gtk4::glib::Propagation::Stop
}

fn close_and_focus(
    popover: &gtk4::glib::WeakRef<gtk4::Popover>,
    focus_on_close: Rc<RefCell<Option<FocusCallback>>>,
) {
    let Some(popover) = popover.upgrade() else {
        return;
    };
    if !popover.is_visible() {
        return;
    }
    popover.popdown();
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
