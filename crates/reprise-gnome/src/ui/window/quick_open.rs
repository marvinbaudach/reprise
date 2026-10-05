//! Adaptive quick-open dialog.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_view::quick_open::{QuickOpenAction, QuickOpenRow};

use super::quick_open_row::{self, PresentedRow};

type QueryCallback = Rc<dyn Fn(String, u64, bool)>;
type ActivateCallback = Rc<dyn Fn(QuickOpenRow, bool)>;

#[derive(Clone)]
struct Activation {
    rows: Rc<RefCell<Vec<PresentedRow>>>,
    callback: Rc<RefCell<Option<ActivateCallback>>>,
    dialog: adw::Dialog,
    restore_focus: Rc<Cell<bool>>,
    generation: Rc<Cell<u64>>,
    results_generation: Rc<Cell<u64>>,
}

impl Activation {
    fn row(&self, position: u32, play_next: bool) {
        if !results_are_fresh(self.generation.get(), self.results_generation.get()) {
            return;
        }
        let row = self
            .rows
            .borrow()
            .get(position as usize)
            .map(|presented| presented.row.clone());
        let callback = self.callback.borrow().clone();
        if let (Some(row), Some(callback)) = (row, callback) {
            self.restore_focus.set(restores_focus(&row));
            self.dialog.force_close();
            callback(row, play_next);
        }
    }
}

pub(super) struct QuickOpenPanel {
    dialog: adw::Dialog,
    entry: gtk4::SearchEntry,
    store: gio::ListStore,
    selection: gtk4::SingleSelection,
    stack: gtk4::Stack,
    status: adw::StatusPage,
    rows: Rc<RefCell<Vec<PresentedRow>>>,
    on_query: Rc<RefCell<Option<QueryCallback>>>,
    on_activate: Rc<RefCell<Option<ActivateCallback>>>,
    focus_guard: Rc<RefCell<Option<crate::ui::transient_focus::TransientFocusGuard>>>,
    restore_focus: Rc<Cell<bool>>,
    open: Rc<Cell<bool>>,
    generation: Rc<Cell<u64>>,
    results_generation: Rc<Cell<u64>>,
    suppress_query: Rc<Cell<bool>>,
}

impl QuickOpenPanel {
    pub(super) fn new() -> Self {
        let quick_open = crate::ui::strings::text(crate::ui::strings::QUICK_OPEN);
        let entry = gtk4::SearchEntry::builder()
            .placeholder_text(&quick_open)
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
        list.update_property(&[gtk4::accessible::Property::Label(&quick_open)]);
        list.set_single_click_activate(true);

        let scroller = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .min_content_height(300)
            .child(&list)
            .build();
        let status = adw::StatusPage::builder()
            .title(crate::ui::strings::text(
                crate::ui::strings::QUICK_OPEN_NO_RESULTS,
            ))
            .build();
        let stack = gtk4::Stack::new();
        stack.add_named(&scroller, Some("results"));
        stack.add_named(&status, Some("status"));
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.append(&entry);
        content.append(&stack);
        let dialog = adw::Dialog::builder()
            .child(&content)
            .content_width(560)
            .content_height(410)
            .presentation_mode(adw::DialogPresentationMode::Auto)
            .build();
        dialog.update_property(&[gtk4::accessible::Property::Label(&quick_open)]);

        let on_query: Rc<RefCell<Option<QueryCallback>>> = Rc::new(RefCell::new(None));
        let generation = Rc::new(Cell::new(0u64));
        let results_generation = Rc::new(Cell::new(0u64));
        let suppress_query = Rc::new(Cell::new(false));
        entry.connect_search_changed({
            let on_query = on_query.clone();
            let generation = generation.clone();
            let suppress_query = suppress_query.clone();
            move |entry| {
                if suppress_query.get() {
                    return;
                }
                let next = generation.get().wrapping_add(1);
                generation.set(next);
                if let Some(callback) = on_query.borrow().clone() {
                    callback(entry.text().to_string(), next, false);
                }
            }
        });
        let on_activate: Rc<RefCell<Option<ActivateCallback>>> = Rc::new(RefCell::new(None));
        let restore_focus = Rc::new(Cell::new(true));
        let activation = Activation {
            rows: rows.clone(),
            callback: on_activate.clone(),
            dialog: dialog.clone(),
            restore_focus: restore_focus.clone(),
            generation: generation.clone(),
            results_generation: results_generation.clone(),
        };
        list.connect_activate({
            let activation = activation.clone();
            move |_, position| {
                activation.row(position, false);
            }
        });
        entry.connect_activate({
            let selection = selection.clone();
            let activation = activation.clone();
            move |_| {
                activation.row(selection.selected(), false);
            }
        });
        wire_keys(&entry, &list, &selection, &activation);

        let focus_guard = Rc::new(RefCell::new(
            None::<crate::ui::transient_focus::TransientFocusGuard>,
        ));
        let open = Rc::new(Cell::new(false));
        dialog.connect_closed({
            let focus_guard = focus_guard.clone();
            let restore_focus = restore_focus.clone();
            let open = open.clone();
            let generation = generation.clone();
            let results_generation = results_generation.clone();
            let suppress_query = suppress_query.clone();
            let entry = entry.clone();
            let rows = rows.clone();
            let store = store.clone();
            move |_| {
                open.set(false);
                generation.set(generation.get().wrapping_add(1));
                results_generation.set(0);
                suppress_query.set(true);
                entry.set_text("");
                suppress_query.set(false);
                rows.borrow_mut().clear();
                store.remove_all();
                let guard = focus_guard.borrow_mut().take();
                if restore_focus.replace(true) {
                    if let Some(guard) = guard {
                        guard.restore();
                    }
                }
            }
        });

        Self {
            dialog,
            entry,
            store,
            selection,
            stack,
            status,
            rows,
            on_query,
            on_activate,
            focus_guard,
            restore_focus,
            open,
            generation,
            results_generation,
            suppress_query,
        }
    }

    pub(super) fn connect_query(&self, callback: impl Fn(String, u64, bool) + 'static) {
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
        self.focus_guard.replace(Some(
            crate::ui::transient_focus::TransientFocusGuard::capture(parent),
        ));
        self.suppress_query.set(true);
        self.entry.set_text("");
        self.suppress_query.set(false);
        let next = self.generation.get().wrapping_add(1);
        self.generation.set(next);
        self.results_generation.set(0);
        self.dialog.set_focus(Some(&self.entry));
        self.dialog.present(Some(parent));
        self.entry.grab_focus();
        if let Some(callback) = self.on_query.borrow().clone() {
            callback(String::new(), next, true);
        }
    }

    pub(super) fn accepts_generation(&self, generation: u64) -> bool {
        self.open.get() && self.generation.get() == generation
    }

    pub(super) fn set_results(&self, generation: u64, rows: Vec<QuickOpenRow>) {
        if !self.accepts_generation(generation) {
            return;
        }
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
            self.stack.set_visible_child_name("results");
        } else {
            self.status.set_title(&crate::ui::strings::text(
                crate::ui::strings::QUICK_OPEN_NO_RESULTS,
            ));
            self.status.set_description(None);
            self.stack.set_visible_child_name("status");
        }
        self.results_generation.set(generation);
    }

    pub(super) fn set_error(&self, generation: u64, detail: &str) {
        if !self.accepts_generation(generation) {
            return;
        }
        self.status.set_title(&crate::ui::strings::text(
            crate::ui::strings::QUICK_OPEN_SEARCH_FAILED,
        ));
        self.status.set_description(Some(detail));
        self.stack.set_visible_child_name("status");
        self.results_generation.set(0);
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

const fn results_are_fresh(current: u64, results: u64) -> bool {
    current == results
}

fn restores_focus(row: &QuickOpenRow) -> bool {
    matches!(
        row,
        QuickOpenRow::Item(item)
            if matches!(item.action, QuickOpenAction::PlayTrack { .. } | QuickOpenAction::PlayStation { .. })
    )
}

fn wire_keys(
    entry: &gtk4::SearchEntry,
    list: &gtk4::ListView,
    selection: &gtk4::SingleSelection,
    activation: &Activation,
) {
    for widget in [
        entry.clone().upcast::<gtk4::Widget>(),
        list.clone().upcast(),
    ] {
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let selection = selection.clone();
        let activation = activation.clone();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let modifiers = modifiers & ACCELERATOR_MODIFIERS;
            if key == gtk4::gdk::Key::Escape && modifiers.is_empty() {
                activation.dialog.force_close();
                return gtk4::glib::Propagation::Stop;
            }
            if matches!(key, gtk4::gdk::Key::Up | gtk4::gdk::Key::Down) && modifiers.is_empty() {
                move_selection(&selection, key, activation.rows.borrow().len());
                return gtk4::glib::Propagation::Stop;
            }
            if let Some(play_next) = activation_play_next(key, modifiers) {
                activation.row(selection.selected(), play_next);
                return gtk4::glib::Propagation::Stop;
            }
            gtk4::glib::Propagation::Proceed
        });
        widget.add_controller(keys);
    }
}

fn activation_play_next(key: gtk4::gdk::Key, modifiers: gtk4::gdk::ModifierType) -> Option<bool> {
    if !matches!(key, gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter) {
        return None;
    }
    let modifiers = modifiers & ACCELERATOR_MODIFIERS;
    let disallowed = gtk4::gdk::ModifierType::CONTROL_MASK
        | gtk4::gdk::ModifierType::SHIFT_MASK
        | gtk4::gdk::ModifierType::SUPER_MASK;
    (!modifiers.intersects(disallowed))
        .then(|| modifiers.contains(gtk4::gdk::ModifierType::ALT_MASK))
}

const ACCELERATOR_MODIFIERS: gtk4::gdk::ModifierType = gtk4::gdk::ModifierType::SHIFT_MASK
    .union(gtk4::gdk::ModifierType::CONTROL_MASK)
    .union(gtk4::gdk::ModifierType::ALT_MASK)
    .union(gtk4::gdk::ModifierType::SUPER_MASK)
    .union(gtk4::gdk::ModifierType::HYPER_MASK)
    .union(gtk4::gdk::ModifierType::META_MASK);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_17_enter_ignores_lock_modifiers_and_alt_enter_plays_next() {
        assert_eq!(
            activation_play_next(
                gtk4::gdk::Key::Return,
                gtk4::gdk::ModifierType::LOCK_MASK
                    | gtk4::gdk::ModifierType::from_bits_retain(1 << 4),
            ),
            Some(false)
        );
        assert_eq!(
            activation_play_next(
                gtk4::gdk::Key::Return,
                gtk4::gdk::ModifierType::ALT_MASK | gtk4::gdk::ModifierType::LOCK_MASK,
            ),
            Some(true)
        );
    }

    #[test]
    fn search_17_stale_results_cannot_activate() {
        assert!(!results_are_fresh(8, 7));
        assert!(results_are_fresh(8, 8));
    }
}
