//! Autocomplete text entry for the tag editor. Wraps `adw::EntryRow`
//! with a `GtkPopover` dropdown showing library suggestions ranked by
//! track count (TAG-6). Popover is `can-focus = false` — focus stays in
//! the entry. Esc closes only the popup.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use reprise_core::queries::autocomplete::{
    query_autocomplete_suggestions, AutocompleteColumn, AutocompleteSuggestion, MAX_SUGGESTIONS,
    MIN_DROPDOWN_CHARS,
};

const DEBOUNCE_MS: u64 = 100;

/// A row in the autocomplete dropdown: either a ranked library value or the
/// trailing "use as new value" row, which is always present so a genuinely
/// new value is never blocked (TAG-6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SuggestionRow {
    Value(AutocompleteSuggestion),
    UseAsNew(String),
}

/// Builds the dropdown's row list from ranked suggestions (already ordered
/// prefix-before-substring by the core query) plus the literal typed text:
/// the "use as new" row is always last, regardless of how many suggestions
/// matched — including zero (TAG-6).
pub(crate) fn build_rows(
    suggestions: &[AutocompleteSuggestion],
    input: &str,
) -> Vec<SuggestionRow> {
    let mut rows: Vec<SuggestionRow> = suggestions
        .iter()
        .cloned()
        .map(SuggestionRow::Value)
        .collect();
    rows.push(SuggestionRow::UseAsNew(input.to_string()));
    rows
}

/// Extracts the literal value a row would commit if accepted (click or
/// Enter), by index into the row model. This replaces scraping the built
/// widget tree for a displayed string, which breaks once the "use as new"
/// row's displayed sentence differs from the value it commits.
pub(crate) fn row_value(rows: &[SuggestionRow], index: i32) -> Option<String> {
    let index = usize::try_from(index).ok()?;
    rows.get(index).map(|row| match row {
        SuggestionRow::Value(suggestion) => suggestion.value.clone(),
        SuggestionRow::UseAsNew(text) => text.clone(),
    })
}

/// Whether the dropdown may appear at all (TAG-6): gated purely on typed
/// length, never on whether any suggestions matched — the "use as new" row
/// means the dropdown is never actually empty once it is eligible to show.
pub(crate) fn should_show_dropdown(input: &str) -> bool {
    input.chars().count() >= MIN_DROPDOWN_CHARS
}

pub struct AutocompleteEntry {
    row: adw::EntryRow,
    popover: gtk4::Popover,
    listbox: gtk4::ListBox,
    section_header: gtk4::Label,
    conn: Rc<RefCell<Connection>>,
    column: AutocompleteColumn,
    debounce_source: Rc<RefCell<Option<glib::SourceId>>>,
    /// Suppresses the `changed` → query cycle while we programmatically
    /// set text from a suggestion click / accept.
    suppress_query: Rc<RefCell<bool>>,
    /// The rows currently backing `listbox`, kept in lockstep with it so
    /// accepting a row (click or Enter) can look up its literal value by
    /// index instead of re-parsing the widget tree.
    current_rows: Rc<RefCell<Vec<SuggestionRow>>>,
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

        let section_header = gtk4::Label::builder()
            .label(crate::ui::strings::text(
                crate::ui::strings::TAG_AUTOCOMPLETE_SECTION_HEADER,
            ))
            .xalign(0.0)
            .visible(false)
            .css_classes(["reprise-autocomplete-section-header"])
            .build();

        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.append(&section_header);
        container.append(&scrolled);

        let popover = gtk4::Popover::builder()
            .child(&container)
            .autohide(false)
            .can_focus(false)
            .has_arrow(false)
            .build();
        popover.set_parent(&row);
        popover.add_css_class("reprise-autocomplete-popover");

        let suppress_query = Rc::new(RefCell::new(false));
        let debounce_source: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let current_rows = Rc::new(RefCell::new(Vec::new()));

        let entry = Self {
            row,
            popover,
            listbox,
            section_header,
            conn,
            column,
            debounce_source,
            suppress_query,
            current_rows,
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
        let section_header = self.section_header.clone();
        let suppress = self.suppress_query.clone();
        let debounce = self.debounce_source.clone();
        let current_rows = self.current_rows.clone();

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
            let section_header = section_header.clone();
            let debounce_inner = debounce.clone();
            let current_rows = current_rows.clone();

            // timeout_add_local returns a SourceId we can cancel later
            let source = glib::timeout_add_local(Duration::from_millis(DEBOUNCE_MS), move || {
                *debounce_inner.borrow_mut() = None;

                if should_show_dropdown(&input) {
                    let suggestions = {
                        let conn = conn.borrow();
                        match query_autocomplete_suggestions(&conn, column, &input, MAX_SUGGESTIONS)
                        {
                            Ok(suggestions) => suggestions,
                            Err(error) => {
                                tracing::warn!(%error, "autocomplete query failed");
                                Vec::new()
                            }
                        }
                    };
                    let rows = build_rows(&suggestions, &input);
                    populate_listbox(&listbox, &rows, &input, column);
                    section_header.set_visible(!suggestions.is_empty());
                    *current_rows.borrow_mut() = rows;
                    // TAG-6: the first row is always pre-marked.
                    listbox.select_row(listbox.row_at_index(0).as_ref());
                    popover.popup();
                } else {
                    popover.popdown();
                    current_rows.borrow_mut().clear();
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
        let current_rows = self.current_rows.clone();

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
                gtk4::gdk::Key::Return => {
                    // The dropdown always pre-selects its first row on
                    // populate (TAG-6), so an open dropdown always has a
                    // selection here — there is no silent "no selection,
                    // take the first row" fallback.
                    if let Some(selected) = listbox.selected_row() {
                        if let Some(value) = row_value(&current_rows.borrow(), selected.index()) {
                            accept_text(&row, &value, &suppress);
                        }
                    }
                    popover.popdown();
                    glib::Propagation::Stop
                }
                gtk4::gdk::Key::Escape => {
                    // Esc cascade stage 1 (TAG-8, the remaining stages are
                    // Paket E's): closes only the popover, text untouched.
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
        let current_rows = self.current_rows.clone();

        self.listbox.connect_row_activated(move |_, list_row| {
            if let Some(value) = row_value(&current_rows.borrow(), list_row.index()) {
                accept_text(&row_entry, &value, &suppress);
            }
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

/// Sets `entry`'s text to `text` (a suggestion, or the literal typed value
/// for "use as new"), suppressing our own internal query cycle for the
/// duration so accepting a row doesn't immediately re-open the dropdown for
/// what was just accepted. External `connect_changed` listeners (pending
/// tracking in `tag_editor_dirty.rs`) are *not* suppressed — accepting a
/// value is a real, deliberate edit and must count as one.
fn accept_text(entry: &adw::EntryRow, text: &str, suppress: &Rc<RefCell<bool>>) {
    *suppress.borrow_mut() = true;
    entry.set_text(text);
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

/// Maps `column` to its "use as new …" sentence formatter. Kept here rather
/// than in `strings.rs` so the string catalog stays decoupled from the core
/// autocomplete-column enum.
fn use_as_new_text(column: AutocompleteColumn, value: &str) -> String {
    match column {
        AutocompleteColumn::Artist => crate::ui::strings::tag_autocomplete_use_as_new_artist(value),
        AutocompleteColumn::Album => crate::ui::strings::tag_autocomplete_use_as_new_album(value),
        AutocompleteColumn::AlbumArtist => {
            crate::ui::strings::tag_autocomplete_use_as_new_album_artist(value)
        }
        AutocompleteColumn::Genre => crate::ui::strings::tag_autocomplete_use_as_new_genre(value),
    }
}

fn populate_listbox(
    listbox: &gtk4::ListBox,
    rows: &[SuggestionRow],
    input: &str,
    column: AutocompleteColumn,
) {
    // Clear existing rows — remove accepts &Widget, no downcast needed
    while let Some(child) = listbox.first_child() {
        listbox.remove(&child);
    }

    let input_lower = input.to_lowercase();

    for row in rows {
        let list_row = match row {
            SuggestionRow::Value(suggestion) => value_row(suggestion, &input_lower),
            SuggestionRow::UseAsNew(text) => use_as_new_row(text, column),
        };
        listbox.append(&list_row);
    }
}

/// Builds a dropdown row for a ranked library value, with the matched
/// substring bolded via Pango and the track count shown alongside.
fn value_row(suggestion: &AutocompleteSuggestion, input_lower: &str) -> gtk4::ListBoxRow {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);

    let value_label = gtk4::Label::builder()
        .label(&suggestion.value)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(pango::EllipsizeMode::End)
        .build();
    if !input_lower.is_empty() {
        if let Some(attrs) = highlight_match(&suggestion.value, input_lower) {
            value_label.set_attributes(Some(&attrs));
        }
    }

    let count_label = gtk4::Label::builder()
        .label(crate::ui::strings::tag_autocomplete_track_count(
            suggestion.track_count,
        ))
        .css_classes(["dim-label"])
        .build();

    hbox.append(&value_label);
    hbox.append(&count_label);
    gtk4::ListBoxRow::builder().child(&hbox).build()
}

/// Builds the trailing "Use “X” as new …" row (TAG-6) — always present,
/// literally quoting the typed text, never blocked by any match state.
fn use_as_new_row(text: &str, column: AutocompleteColumn) -> gtk4::ListBoxRow {
    let label = gtk4::Label::builder()
        .label(use_as_new_text(column, text))
        .xalign(0.0)
        .ellipsize(pango::EllipsizeMode::End)
        .css_classes(["reprise-autocomplete-use-as-new"])
        .build();
    gtk4::ListBoxRow::builder().child(&label).build()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn suggestion(value: &str, track_count: i64) -> AutocompleteSuggestion {
        AutocompleteSuggestion {
            value: value.to_string(),
            track_count,
        }
    }

    #[test]
    fn tag_6_dropdown_needs_two_chars() {
        assert!(!should_show_dropdown(""));
        assert!(!should_show_dropdown("a"));
        assert!(should_show_dropdown("ab"));
        assert!(should_show_dropdown("abc"));
    }

    #[test]
    fn tag_6_use_as_new_row_always_last() {
        // Zero suggestions: the row is still there, and it's the only row.
        let empty = build_rows(&[], "Sui");
        assert_eq!(empty.len(), 1);
        assert!(matches!(&empty[0], SuggestionRow::UseAsNew(t) if t == "Sui"));

        // With suggestions: still last, suggestions keep the core's order.
        let suggestions = vec![
            suggestion("Suicide Silence", 5),
            suggestion("Suicidal Tendencies", 2),
        ];
        let rows = build_rows(&suggestions, "Sui");
        assert_eq!(rows.len(), 3);
        assert!(matches!(&rows[0], SuggestionRow::Value(s) if s.value == "Suicide Silence"));
        assert!(matches!(&rows[1], SuggestionRow::Value(s) if s.value == "Suicidal Tendencies"));
        assert!(matches!(&rows[2], SuggestionRow::UseAsNew(t) if t == "Sui"));
    }

    #[test]
    fn row_value_extracts_suggestion_value_not_display_text() {
        let rows = build_rows(&[suggestion("Suicide Silence", 5)], "Sui");
        assert_eq!(row_value(&rows, 0).as_deref(), Some("Suicide Silence"));
    }

    #[test]
    fn row_value_extracts_literal_typed_text_for_use_as_new() {
        let rows = build_rows(&[suggestion("Suicide Silence", 5)], "Sui");
        // Index 1 is the "use as new" row — its committed value is the
        // literal typed text, never the displayed sentence around it.
        assert_eq!(row_value(&rows, 1).as_deref(), Some("Sui"));
    }

    #[test]
    fn row_value_is_none_out_of_bounds() {
        let rows = build_rows(&[], "Sui");
        assert_eq!(row_value(&rows, 5), None);
        assert_eq!(row_value(&rows, -1), None);
    }
}
