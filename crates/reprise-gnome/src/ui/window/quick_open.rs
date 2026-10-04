//! Adaptive quick-open dialog.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_view::quick_open::QuickOpenRow;

use super::quick_open_row::{self, PresentedRow};

type QueryCallback = Rc<dyn Fn(String)>;
type ActivateCallback = Rc<dyn Fn(QuickOpenRow, bool)>;

pub(super) struct QuickOpenPanel {
    dialog: adw::Dialog,
    entry: gtk4::SearchEntry,
    store: gio::ListStore,
    selection: gtk4::SingleSelection,
    rows: Rc<RefCell<Vec<PresentedRow>>>,
    on_query: Rc<RefCell<Option<QueryCallback>>>,
    on_activate: Rc<RefCell<Option<ActivateCallback>>>,
    focus_guard: Rc<RefCell<Option<crate::ui::transient_focus::TransientFocusGuard>>>,
    direct_invoker: Rc<RefCell<gtk4::glib::WeakRef<gtk4::Widget>>>,
    parent_window: Rc<RefCell<gtk4::glib::WeakRef<gtk4::Window>>>,
    restore_focus: Rc<Cell<bool>>,
    open: Rc<Cell<bool>>,
}

impl QuickOpenPanel {
    pub(super) fn new() -> Self {
        let entry = gtk4::SearchEntry::builder()
            .placeholder_text(crate::i18n::gettext("Quick open"))
            .hexpand(true)
            .build();
        entry.set_search_delay(120);
        entry.set_margin_start(12);
        entry.set_margin_end(12);
        entry.set_margin_top(12);
        entry.set_margin_bottom(6);

        let rows = Rc::new(RefCell::new(Vec::new()));
        let store = gio::ListStore::new::<gtk4::StringObject>();
        let selection = gtk4::SingleSelection::new(Some(store.clone()));
        selection.set_autoselect(true);
        selection.set_can_unselect(false);
        let list = gtk4::ListView::new(
            Some(selection.clone()),
            Some(quick_open_row::factory(rows.clone())),
        );
        list.set_accessible_role(gtk4::AccessibleRole::ListBox);
        list.set_single_click_activate(true);

        let scroller = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .min_content_height(300)
            .child(&list)
            .build();
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.append(&entry);
        content.append(&scroller);
        let dialog = adw::Dialog::builder()
            .child(&content)
            .content_width(560)
            .content_height(410)
            .presentation_mode(adw::DialogPresentationMode::Auto)
            .build();

        let on_query: Rc<RefCell<Option<QueryCallback>>> = Rc::new(RefCell::new(None));
        entry.connect_search_changed({
            let on_query = on_query.clone();
            move |entry| {
                if let Some(callback) = on_query.borrow().clone() {
                    callback(entry.text().to_string());
                }
            }
        });
        let on_activate: Rc<RefCell<Option<ActivateCallback>>> = Rc::new(RefCell::new(None));
        let restore_focus = Rc::new(Cell::new(true));
        list.connect_activate({
            let rows = rows.clone();
            let on_activate = on_activate.clone();
            let dialog = dialog.clone();
            let restore_focus = restore_focus.clone();
            move |_, position| {
                activate_row(
                    &rows,
                    &on_activate,
                    &dialog,
                    &restore_focus,
                    position,
                    false,
                );
            }
        });
        entry.connect_activate({
            let rows = rows.clone();
            let selection = selection.clone();
            let on_activate = on_activate.clone();
            let dialog = dialog.clone();
            let restore_focus = restore_focus.clone();
            move |_| {
                activate_row(
                    &rows,
                    &on_activate,
                    &dialog,
                    &restore_focus,
                    selection.selected(),
                    false,
                );
            }
        });
        wire_keys(
            &entry,
            &list,
            &selection,
            &rows,
            &on_activate,
            &dialog,
            &restore_focus,
        );

        let focus_guard = Rc::new(RefCell::new(
            None::<crate::ui::transient_focus::TransientFocusGuard>,
        ));
        let open = Rc::new(Cell::new(false));
        let direct_invoker = Rc::new(RefCell::new(gtk4::glib::WeakRef::new()));
        let parent_window = Rc::new(RefCell::new(gtk4::glib::WeakRef::new()));
        dialog.connect_closed({
            let focus_guard = focus_guard.clone();
            let direct_invoker = direct_invoker.clone();
            let parent_window = parent_window.clone();
            let restore_focus = restore_focus.clone();
            let open = open.clone();
            move |_| {
                open.set(false);
                let guard = focus_guard.borrow_mut().take();
                if !restore_focus.replace(true) {
                    return;
                }
                if let Some(guard) = guard {
                    guard.restore();
                }
                let invoker = direct_invoker
                    .borrow()
                    .upgrade()
                    .filter(|widget| !inside_recycled_view(widget));
                let window = parent_window.borrow().upgrade();
                gtk4::glib::idle_add_local_once(move || {
                    if let (Some(invoker), Some(window)) = (invoker, window) {
                        gtk4::prelude::GtkWindowExt::set_focus(&window, Some(&invoker));
                    }
                });
            }
        });

        Self {
            dialog,
            entry,
            store,
            selection,
            rows,
            on_query,
            on_activate,
            focus_guard,
            direct_invoker,
            parent_window,
            restore_focus,
            open,
        }
    }

    pub(super) fn connect_query(&self, callback: impl Fn(String) + 'static) {
        self.on_query.replace(Some(Rc::new(callback)));
    }

    pub(super) fn connect_activate(&self, callback: impl Fn(QuickOpenRow, bool) + 'static) {
        self.on_activate.replace(Some(Rc::new(callback)));
    }

    pub(super) fn present(&self, parent: &adw::ApplicationWindow) {
        if self.open.replace(true) {
            return;
        }
        self.restore_focus.set(true);
        let focus = gtk4::prelude::GtkWindowExt::focus(parent)
            .unwrap_or_else(|| parent.clone().upcast::<gtk4::Widget>());
        *self.direct_invoker.borrow_mut() = focus.downgrade();
        *self.parent_window.borrow_mut() = parent.clone().upcast::<gtk4::Window>().downgrade();
        self.focus_guard.replace(Some(
            crate::ui::transient_focus::TransientFocusGuard::capture(parent),
        ));
        self.dialog.set_focus(Some(&self.entry));
        self.dialog.present(Some(parent));
        self.entry.grab_focus();
        if self.entry.text().is_empty() {
            if let Some(callback) = self.on_query.borrow().clone() {
                callback(String::new());
            }
        } else {
            self.entry.set_text("");
        }
    }

    pub(super) fn set_results(&self, rows: Vec<QuickOpenRow>) {
        let mut previous = None;
        let presented = rows
            .into_iter()
            .map(|row| {
                let kind = quick_open_row::row_kind(&row);
                let starts_group = previous != Some(kind);
                previous = Some(kind);
                PresentedRow { row, starts_group }
            })
            .collect::<Vec<_>>();
        let count = presented.len();
        *self.rows.borrow_mut() = presented;
        self.store.remove_all();
        for index in 0..count {
            self.store
                .append(&gtk4::StringObject::new(&index.to_string()));
        }
        if self.store.n_items() > 0 {
            self.selection.set_selected(0);
        }
    }

    #[cfg(test)]
    pub(super) fn entry(&self) -> &gtk4::SearchEntry {
        &self.entry
    }

    #[cfg(test)]
    pub(super) fn entry_contains_focus(&self) -> bool {
        self.entry
            .root()
            .and_then(|root| gtk4::prelude::RootExt::focus(&root))
            .is_some_and(|focus| {
                focus == self.entry.clone().upcast::<gtk4::Widget>()
                    || focus.is_ancestor(&self.entry)
            })
    }

    #[cfg(test)]
    pub(super) fn result_count(&self) -> usize {
        self.rows.borrow().len()
    }

    #[cfg(test)]
    pub(super) fn is_visible(&self) -> bool {
        self.open.get()
    }

    #[cfg(test)]
    pub(super) fn press_escape(&self) {
        self.dialog.force_close();
    }
}

fn inside_recycled_view(widget: &gtk4::Widget) -> bool {
    let mut current = Some(widget.clone());
    while let Some(widget) = current {
        if widget.is::<gtk4::ListView>()
            || widget.is::<gtk4::GridView>()
            || widget.is::<gtk4::ColumnView>()
        {
            return true;
        }
        current = widget.parent();
    }
    false
}

fn activate_row(
    rows: &RefCell<Vec<PresentedRow>>,
    callback: &RefCell<Option<ActivateCallback>>,
    dialog: &adw::Dialog,
    restore_focus: &Cell<bool>,
    position: u32,
    play_next: bool,
) {
    let row = rows
        .borrow()
        .get(position as usize)
        .map(|presented| presented.row.clone());
    let callback = callback.borrow().clone();
    if let (Some(row), Some(callback)) = (row, callback) {
        restore_focus.set(false);
        dialog.force_close();
        callback(row, play_next);
    }
}

fn wire_keys(
    entry: &gtk4::SearchEntry,
    list: &gtk4::ListView,
    selection: &gtk4::SingleSelection,
    rows: &Rc<RefCell<Vec<PresentedRow>>>,
    callback: &Rc<RefCell<Option<ActivateCallback>>>,
    dialog: &adw::Dialog,
    restore_focus: &Rc<Cell<bool>>,
) {
    for widget in [
        entry.clone().upcast::<gtk4::Widget>(),
        list.clone().upcast(),
    ] {
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let selection = selection.clone();
        let rows = rows.clone();
        let callback = callback.clone();
        let dialog = dialog.clone();
        let restore_focus = restore_focus.clone();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            if key == gtk4::gdk::Key::Escape && modifiers.is_empty() {
                dialog.force_close();
                return gtk4::glib::Propagation::Stop;
            }
            if matches!(key, gtk4::gdk::Key::Up | gtk4::gdk::Key::Down) && modifiers.is_empty() {
                move_selection(&selection, key, rows.borrow().len());
                return gtk4::glib::Propagation::Stop;
            }
            let disallowed = gtk4::gdk::ModifierType::CONTROL_MASK
                | gtk4::gdk::ModifierType::SHIFT_MASK
                | gtk4::gdk::ModifierType::SUPER_MASK;
            if matches!(key, gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter)
                && !modifiers.intersects(disallowed)
            {
                activate_row(
                    &rows,
                    &callback,
                    &dialog,
                    &restore_focus,
                    selection.selected(),
                    modifiers.contains(gtk4::gdk::ModifierType::ALT_MASK),
                );
                return gtk4::glib::Propagation::Stop;
            }
            gtk4::glib::Propagation::Proceed
        });
        widget.add_controller(keys);
    }
}

fn move_selection(selection: &gtk4::SingleSelection, key: gtk4::gdk::Key, count: usize) {
    if count == 0 {
        return;
    }
    let current = selection.selected().min((count - 1) as u32);
    let next = if key == gtk4::gdk::Key::Up {
        current.saturating_sub(1)
    } else {
        (current + 1).min((count - 1) as u32)
    };
    selection.set_selected(next);
}
