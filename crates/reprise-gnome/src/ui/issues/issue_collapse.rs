//! Lazy, paginated issue-row expansion.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

const INITIAL_ROWS: u32 = 2;
const PAGE_SIZE: u32 = 50;

fn visible_end(total: u32, expansions: u32) -> u32 {
    let expanded = expansions.saturating_mul(PAGE_SIZE);
    INITIAL_ROWS.saturating_add(expanded).min(total)
}

fn expansion_pill_label(remaining: u32) -> String {
    let next_page = remaining.min(PAGE_SIZE);
    crate::ui::strings::issue_show_more(next_page)
}

struct Shared {
    total: u32,
    visible: Cell<u32>,
    listbox: glib::WeakRef<gtk4::ListBox>,
    footer: glib::WeakRef<gtk4::ListBoxRow>,
    button: glib::WeakRef<gtk4::Button>,
    build_row: Rc<dyn Fn(u32) -> gtk4::Widget>,
}

/// A list that materializes two rows initially and at most fifty per click.
pub(in crate::ui) struct CollapsedList {
    listbox: gtk4::ListBox,
    shared: Rc<Shared>,
}

impl CollapsedList {
    pub(in crate::ui) fn new(total: u32, build_row: Rc<dyn Fn(u32) -> gtk4::Widget>) -> Self {
        let listbox = gtk4::ListBox::new();
        listbox.set_selection_mode(gtk4::SelectionMode::Multiple);
        listbox.set_activate_on_single_click(false);
        listbox.add_css_class("issue-card-list");
        Self::attach_to(&listbox, total, build_row)
    }

    /// Populates an existing card body while retaining the same lazy paging.
    pub(in crate::ui) fn attach_to(
        listbox: &gtk4::ListBox,
        total: u32,
        build_row: Rc<dyn Fn(u32) -> gtk4::Widget>,
    ) -> Self {
        listbox.set_selection_mode(gtk4::SelectionMode::Multiple);
        listbox.set_activate_on_single_click(false);

        let button = gtk4::Button::new();
        button.add_css_class("flat");
        button.add_css_class("pill");
        button.add_css_class("issue-row-pill");
        button.set_halign(gtk4::Align::Center);

        let footer = gtk4::ListBoxRow::new();
        footer.set_selectable(false);
        footer.set_activatable(false);
        footer.add_css_class("issue-collapse-footer");
        footer.set_child(Some(&button));

        let shared = Rc::new(Shared {
            total,
            visible: Cell::new(0),
            listbox: listbox.downgrade(),
            footer: footer.downgrade(),
            button: button.downgrade(),
            build_row,
        });

        append_through(&shared, visible_end(total, 0));
        update_footer(&shared);

        let callback_shared = shared.clone();
        button.connect_clicked(move |_| {
            let shared = &callback_shared;
            let next = shared
                .visible
                .get()
                .saturating_add(PAGE_SIZE)
                .min(shared.total);
            append_through(shared, next);
            update_footer(shared);
        });

        Self {
            listbox: listbox.clone(),
            shared,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::ListBox {
        &self.listbox
    }

    pub(in crate::ui) fn visible_count(&self) -> u32 {
        self.shared.visible.get()
    }
}

fn append_through(shared: &Shared, end: u32) {
    let Some(listbox) = shared.listbox.upgrade() else {
        return;
    };
    let Some(footer) = shared.footer.upgrade() else {
        return;
    };
    if footer.parent().is_some() {
        listbox.remove(&footer);
    }

    for index in shared.visible.get()..end {
        let row = (shared.build_row)(index);
        listbox.append(&row);
    }
    shared.visible.set(end);
}

fn update_footer(shared: &Shared) {
    let remaining = shared.total.saturating_sub(shared.visible.get());
    if remaining == 0 {
        return;
    }

    let Some(listbox) = shared.listbox.upgrade() else {
        return;
    };
    let Some(footer) = shared.footer.upgrade() else {
        return;
    };
    let Some(button) = shared.button.upgrade() else {
        return;
    };

    button.set_label(&expansion_pill_label(remaining));
    listbox.append(&footer);
}

#[cfg(test)]
mod tests {
    use super::{expansion_pill_label, visible_end};

    #[test]
    fn issue_collapse_clamps_small_lists_after_each_expansion() {
        assert_eq!(
            [visible_end(1, 0), visible_end(1, 1), visible_end(1, 2)],
            [1, 1, 1]
        );
        assert_eq!(
            [visible_end(2, 0), visible_end(2, 1), visible_end(2, 2)],
            [2, 2, 2]
        );
        assert_eq!(
            [visible_end(3, 0), visible_end(3, 1), visible_end(3, 2)],
            [2, 3, 3]
        );
    }

    #[test]
    fn issue_collapse_reveals_fifty_rows_per_expansion() {
        assert_eq!(
            [
                visible_end(120, 0),
                visible_end(120, 1),
                visible_end(120, 2),
            ],
            [2, 52, 102]
        );
    }

    #[test]
    fn issue_collapse_pill_label_caps_the_next_page_and_pluralizes() {
        assert_eq!(expansion_pill_label(1), "Show 1 more");
        assert_eq!(expansion_pill_label(2), "Show 2 more");
        assert_eq!(expansion_pill_label(118), "Show 50 more");
    }
}
