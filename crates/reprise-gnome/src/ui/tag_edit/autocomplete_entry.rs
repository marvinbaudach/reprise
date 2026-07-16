//! Autocomplete text entry for the tag editor. Wraps `adw::EntryRow`
//! with a `GtkPopover` dropdown showing library suggestions ranked by
//! track count. Popover is `can-focus = false` — focus stays in the entry.
//! Tab accepts the top suggestion (same as ↵). Esc closes only the popup.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use reprise_core::queries::autocomplete::{
    query_autocomplete_suggestions, AutocompleteColumn, AutocompleteSuggestion,
};

const MAX_SUGGESTIONS: usize = 8;
const DEBOUNCE_MS: u64 = 100;

pub struct AutocompleteEntry {
    row: adw::EntryRow,
    popover: gtk4::Popover,
    listbox: gtk4::ListBox,
    conn: Rc<RefCell<Connection>>,
    column: AutocompleteColumn,
    debounce_source: Rc<RefCell<Option<glib::SourceId>>>,
    /// Suppresses the `changed` → query cycle while we programmatically
    /// set text from a suggestion click / Tab accept.
    suppress_query: Rc<RefCell<bool>>,
}

impl AutocompleteEntry {
    pub fn new(title: &str, column: AutocompleteColumn, conn: Rc<RefCell<Connection>>) -> Self {
        let row = adw::EntryRow::builder().title(title).build();
        let listbox = gtk4::ListBox::builder()
            .selection_mode(gtk4::SelectionMode::Single)
            .build();
        listbox.add_css_class("reprise-autocomplete-list");

        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&listbox)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .max_content_height(320)
            .propagate_natural_height(true)
            .build();

        let popover = gtk4::Popover::builder()
            .child(&scrolled)
            .autohide(false)
            .can_focus(false)
            .has_arrow(false)
            .build();
        popover.set_parent(&row);
        popover.add_css_class("reprise-autocomplete-popover");

        let suppress_query = Rc::new(RefCell::new(false));
        let debounce_source: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

        let entry = Self {
            row,
            popover,
            listbox,
            conn,
            column,
            debounce_source,
            suppress_query,
        };
        entry.wire_changed();
        entry.wire_key_navigation();
        entry.wire_row_activated();
        entry
    }

    pub fn row(&self) -> &adw::EntryRow {
        &self.row
    }

    pub fn set_text(&self, text: &str) {
        *self.suppress_query.borrow_mut() = true;
        self.row.set_text(text);
        *self.suppress_query.borrow_mut() = false;
    }

    pub fn text(&self) -> String {
        self.row.text().to_string()
    }

    pub fn connect_changed(&self, f: impl Fn() + 'static) {
        self.row.connect_changed(move |_| f());
    }

    fn wire_changed(&self) {
        let conn = self.conn.clone();
        let column = self.column;
        let listbox = self.listbox.clone();
        let popover = self.popover.clone();
        let suppress = self.suppress_query.clone();
        let debounce = self.debounce_source.clone();

        self.row.connect_changed(move |row| {
            if *suppress.borrow() {
                return;
            }
            // Cancel pending debounce
            if let Some(id) = debounce.borrow_mut().take() {
                id.remove();
            }
            let input = row.text().to_string();
            let conn = conn.clone();
            let listbox = listbox.clone();
            let popover = popover.clone();
            let debounce_inner = debounce.clone();

            // timeout_add_local returns a SourceId we can cancel later
            let source = glib::timeout_add_local(Duration::from_millis(DEBOUNCE_MS), move || {
                *debounce_inner.borrow_mut() = None;
                let suggestions = {
                    let conn = conn.borrow();
                    match query_autocomplete_suggestions(&conn, column, &input, MAX_SUGGESTIONS) {
                        Ok(suggestions) => suggestions,
                        Err(error) => {
                            tracing::warn!(%error, "autocomplete query failed");
                            Vec::new()
                        }
                    }
                };
                populate_listbox(&listbox, &suggestions, &input);
                if suggestions.is_empty() || input.is_empty() {
                    popover.popdown();
                } else {
                    popover.popup();
                }
                glib::ControlFlow::Break
            });
            *debounce.borrow_mut() = Some(source);
        });
    }

    fn wire_key_navigation(&self) {
        let listbox = self.listbox.clone();
        let popover = self.popover.clone();
        let row = self.row.clone();
        let suppress = self.suppress_query.clone();

        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);

        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            if !popover.is_visible() {
                return glib::Propagation::Proceed;
            }
            match keyval {
                gtk4::gdk::Key::Down => {
                    move_selection(&listbox, 1);
                    glib::Propagation::Stop
                }
                gtk4::gdk::Key::Up => {
                    move_selection(&listbox, -1);
                    glib::Propagation::Stop
                }
                gtk4::gdk::Key::Return | gtk4::gdk::Key::Tab => {
                    if let Some(selected) = listbox.selected_row() {
                        accept_row(&row, &selected, &suppress);
                        popover.popdown();
                        glib::Propagation::Stop
                    } else if let Some(first) = listbox.row_at_index(0) {
                        // Tab/Enter with no selection = accept first
                        accept_row(&row, &first, &suppress);
                        popover.popdown();
                        glib::Propagation::Stop
                    } else {
                        glib::Propagation::Proceed
                    }
                }
                gtk4::gdk::Key::Escape => {
                    popover.popdown();
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.row.add_controller(key_controller);
    }

    fn wire_row_activated(&self) {
        let row_entry = self.row.clone();
        let popover = self.popover.clone();
        let suppress = self.suppress_query.clone();

        self.listbox.connect_row_activated(move |_, list_row| {
            accept_row(&row_entry, list_row, &suppress);
            popover.popdown();
        });
    }
}

impl Drop for AutocompleteEntry {
    fn drop(&mut self) {
        if let Some(id) = self.debounce_source.borrow_mut().take() {
            id.remove();
        }
        self.popover.unparent();
    }
}

/// Extract the plain-text value from a list row built by `populate_listbox`.
///
/// Row structure: `ListBoxRow` → `Box` (hbox) → `Label` (value, first child)
fn accept_row(entry: &adw::EntryRow, list_row: &gtk4::ListBoxRow, suppress: &Rc<RefCell<bool>>) {
    let Some(label) = list_row
        .child()
        .and_then(|w| w.first_child())
        .and_then(|w| w.downcast::<gtk4::Label>().ok())
    else {
        return;
    };
    let text = label.text().to_string();
    *suppress.borrow_mut() = true;
    entry.set_text(&text);
    entry.set_position(-1); // cursor at end
    *suppress.borrow_mut() = false;
}

fn move_selection(listbox: &gtk4::ListBox, direction: i32) {
    let current = listbox.selected_row().map_or(-1, |r| r.index());
    let next = current + direction;
    if let Some(row) = listbox.row_at_index(next) {
        listbox.select_row(Some(&row));
    }
}

fn populate_listbox(listbox: &gtk4::ListBox, suggestions: &[AutocompleteSuggestion], input: &str) {
    // Clear existing rows — remove accepts &Widget, no downcast needed
    while let Some(child) = listbox.first_child() {
        listbox.remove(&child);
    }

    let input_lower = input.to_lowercase();

    for suggestion in suggestions {
        let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);

        // Value label with matched substring highlighted via Pango
        let value_label = gtk4::Label::builder()
            .label(&suggestion.value)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(pango::EllipsizeMode::End)
            .build();
        if !input_lower.is_empty() {
            if let Some(attrs) = highlight_match(&suggestion.value, &input_lower) {
                value_label.set_attributes(Some(&attrs));
            }
        }

        // Track count label
        let count_label = gtk4::Label::builder()
            .label(crate::ui::strings::tag_autocomplete_track_count(
                suggestion.track_count,
            ))
            .css_classes(["dim-label"])
            .build();

        hbox.append(&value_label);
        hbox.append(&count_label);
        let row = gtk4::ListBoxRow::builder().child(&hbox).build();
        listbox.append(&row);
    }
}

/// Returns Pango attributes that bold the matched substring within `text`.
///
/// Finds `input_lower` (already lowercased) inside `text` by lowercasing
/// `text` for comparison, then maps byte offsets back to the original.
/// The match start/end byte positions in `text_lower` are valid for Pango
/// because Pango uses UTF-8 byte indices.
fn highlight_match(text: &str, input_lower: &str) -> Option<pango::AttrList> {
    let text_lower = text.to_lowercase();
    let start_byte = text_lower.find(input_lower)?;
    // Walk the original text to find the same byte boundary.
    // For ASCII and most Latin text, start_byte in text_lower == start_byte in
    // text because to_lowercase() is length-preserving for those code points.
    // For the uncommon cases (e.g. 'İ' → 'i̇'), we fall back to a safe scan.
    let end_byte = start_byte + input_lower.len();

    // Verify the byte boundaries are valid char boundaries in the original.
    if !text.is_char_boundary(start_byte) || !text.is_char_boundary(end_byte) {
        return None;
    }

    let attrs = pango::AttrList::new();
    let mut bold = pango::AttrInt::new_weight(pango::Weight::Bold);
    bold.set_start_index(start_byte as u32);
    bold.set_end_index(end_byte as u32);
    attrs.insert(bold);
    Some(attrs)
}
