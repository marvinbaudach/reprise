//! Autocomplete text entry for the tag editor. Wraps `adw::EntryRow`
//! with a `GtkPopover` dropdown showing library suggestions ranked by
//! track count (TAG-6), plus an inline ghost completion (TAG-7): the best
//! prefix match rendered dimmed in a second borderless popover anchored to
//! the entry, accepted with Tab. Both popovers are `can-focus = false` —
//! focus stays in the entry. Esc closes only the suggestion dropdown.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::EntryRowExt;
use rusqlite::Connection;

use reprise_core::queries::autocomplete::{
    query_autocomplete_suggestions, query_ghost_completion, AutocompleteColumn,
    AutocompleteSuggestion, MAX_SUGGESTIONS, MIN_DROPDOWN_CHARS,
};

const DEBOUNCE_MS: u64 = 100;

/// TAG-7 kill switch. `false`: no ghost text, no Tab badge, Tab is a pure
/// focus change, and the dropdown (TAG-6) is completely unaffected — the
/// fallback the rule text explicitly sanctions ("kein halb kaputtes Ghost
/// im Release"). The mechanism below (query wiring, Tab semantics, badge
/// visibility) is fully implemented and covered by tests either way; what
/// is *not* verifiable headless is the popover's pixel alignment "behind
/// the cursor" — this file was authored without a display to eyeball that
/// against. Flip to `true` only after a sighted pass confirms the ghost
/// popover reads correctly next to real typed text.
const GHOST_ENABLED: bool = false;

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

/// The remaining characters `completion` adds after `typed`, or `None` if
/// `completion` is not a case-insensitive extension of `typed` (nothing to
/// ghost) or is identical to it (nothing left to complete).
pub(crate) fn ghost_suffix(typed: &str, completion: &str) -> Option<String> {
    if typed.is_empty() {
        return None;
    }
    let typed_lower = typed.to_lowercase();
    let completion_lower = completion.to_lowercase();
    if !completion_lower.starts_with(&typed_lower) {
        return None;
    }
    // Byte-length parity between `typed` and its lowercased form holds for
    // ASCII/most Latin text (see `highlight_match`'s comment for the same
    // caveat); guard the boundary defensively rather than assume it.
    if !completion.is_char_boundary(typed.len()) {
        return None;
    }
    let suffix = &completion[typed.len()..];
    if suffix.is_empty() {
        return None;
    }
    Some(suffix.to_string())
}

/// TAG-7's display gate: the ghost (and, by extension, the Tab badge) is
/// visible only when the feature is enabled and there is a real, non-empty
/// completion left to show. Pure so both states of the kill switch are
/// testable without a running display.
pub(crate) fn ghost_display(
    enabled: bool,
    typed: &str,
    completion: Option<&str>,
) -> Option<String> {
    if !enabled {
        return None;
    }
    ghost_suffix(typed, completion?)
}

/// What Tab does (TAG-7): narrowed to accepting a *visible* ghost only —
/// with no ghost shown, Tab is a pure focus change. There is no silent
/// first-row dropdown accept via Tab anymore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TabAction {
    AcceptGhost(String),
    MoveFocus,
}

pub(crate) fn tab_action(ghost: Option<&str>) -> TabAction {
    match ghost {
        Some(text) if !text.is_empty() => TabAction::AcceptGhost(text.to_string()),
        _ => TabAction::MoveFocus,
    }
}

pub struct AutocompleteEntry {
    row: adw::EntryRow,
    popover: gtk4::Popover,
    listbox: gtk4::ListBox,
    section_header: gtk4::Label,
    /// Borderless popover anchored to `row` (not the grid cell — TAG-7)
    /// showing the dimmed ghost suffix. Separate from `popover` since the
    /// ghost can be visible below `MIN_DROPDOWN_CHARS`, where the
    /// suggestion dropdown never appears at all.
    ghost_popover: gtk4::Popover,
    ghost_label: gtk4::Label,
    /// "Tab" hint shown in the entry's suffix slot only while a ghost is
    /// visible (TAG-7); added via `EntryRow::add_suffix`, so it lives
    /// inside the row itself rather than needing a separate parent.
    tab_badge: gtk4::Label,
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
    /// The full completion string currently ghosted, if any (TAG-7). This
    /// is pure display state — it is never read as a pending value; only
    /// an explicit Tab accept ever writes it into the entry's real text.
    current_ghost: Rc<RefCell<Option<String>>>,
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
        container.add_css_class("reprise-autocomplete-menu");
        container.append(&section_header);
        container.append(&scrolled);

        let popover = gtk4::Popover::builder()
            .child(&container)
            .autohide(false)
            .can_focus(false)
            .has_arrow(false)
            .build();
        popover.set_parent(&row);
        popover.set_position(gtk4::PositionType::Bottom);
        popover.set_halign(gtk4::Align::Start);
        popover.set_offset(0, -1);
        popover.add_css_class("reprise-autocomplete-popover");

        let ghost_label = gtk4::Label::builder()
            .xalign(0.0)
            .css_classes(["reprise-autocomplete-ghost-label"])
            .build();
        let ghost_popover = gtk4::Popover::builder()
            .child(&ghost_label)
            .autohide(false)
            .can_focus(false)
            .has_arrow(false)
            .build();
        ghost_popover.set_parent(&row);
        ghost_popover.add_css_class("reprise-autocomplete-ghost-popover");

        let tab_badge = gtk4::Label::builder()
            .label(crate::ui::strings::text(
                crate::ui::strings::TAG_AUTOCOMPLETE_GHOST_TAB_HINT,
            ))
            .visible(false)
            .css_classes(["reprise-autocomplete-ghost-badge"])
            .build();
        row.add_suffix(&tab_badge);

        let suppress_query = Rc::new(RefCell::new(false));
        let debounce_source: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let current_rows = Rc::new(RefCell::new(Vec::new()));
        let current_ghost = Rc::new(RefCell::new(None));

        let entry = Self {
            row,
            popover,
            listbox,
            section_header,
            ghost_popover,
            ghost_label,
            tab_badge,
            conn,
            column,
            debounce_source,
            suppress_query,
            current_rows,
            current_ghost,
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
        let ghost_popover = self.ghost_popover.clone();
        let ghost_label = self.ghost_label.clone();
        let tab_badge = self.tab_badge.clone();
        let current_ghost = self.current_ghost.clone();

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
            let ghost_popover = ghost_popover.clone();
            let ghost_label = ghost_label.clone();
            let tab_badge = tab_badge.clone();
            let current_ghost = current_ghost.clone();

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
                    // Match the dropdown to the field's width. Labels declare
                    // a tiny natural width and ellipsize, so long copy cannot
                    // expand the popover beyond this anchor width.
                    if let Some(anchor) = popover.parent() {
                        let width = anchor.width();
                        if width > 0 {
                            popover.set_size_request(width, -1);
                            if let Some(content) = popover.child() {
                                content.set_size_request(width, -1);
                            }
                        }
                    }
                    popover.popup();
                } else {
                    popover.popdown();
                    current_rows.borrow_mut().clear();
                }

                // TAG-7: the ghost is queried independently of the dropdown
                // gate above — it can show below MIN_DROPDOWN_CHARS, where
                // the dropdown never appears.
                let ghost_completion = {
                    let conn = conn.borrow();
                    query_ghost_completion(&conn, column, &input)
                };
                apply_ghost(
                    &ghost_popover,
                    &ghost_label,
                    &tab_badge,
                    &current_ghost,
                    &input,
                    ghost_completion.as_deref(),
                );

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
        let ghost_popover = self.ghost_popover.clone();
        let tab_badge = self.tab_badge.clone();
        let current_ghost = self.current_ghost.clone();

        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);

        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gtk4::gdk::Key::Tab {
                // TAG-7: Tab is judged purely on ghost visibility, entirely
                // independent of whether the suggestion dropdown happens to
                // be open — the ghost can be showing below
                // MIN_DROPDOWN_CHARS, where the dropdown never appears.
                let ghost = current_ghost.borrow().clone();
                return match tab_action(ghost.as_deref()) {
                    TabAction::AcceptGhost(full) => {
                        accept_text(&row, &full, &suppress);
                        popover.popdown();
                        ghost_popover.popdown();
                        tab_badge.set_visible(false);
                        *current_ghost.borrow_mut() = None;
                        glib::Propagation::Stop
                    }
                    TabAction::MoveFocus => glib::Propagation::Proceed,
                };
            }

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
        self.ghost_popover.unparent();
    }
}

/// Updates the ghost popover, its label text, and the Tab badge from the
/// current typed text and the core's best prefix completion (TAG-7). Ghost
/// state is purely for display: `current_ghost` records the full
/// completion string so Tab-accept knows what to write, but nothing here
/// ever touches the entry's real text or fires `changed` — only an
/// explicit accept does that.
fn apply_ghost(
    ghost_popover: &gtk4::Popover,
    ghost_label: &gtk4::Label,
    tab_badge: &gtk4::Label,
    current_ghost: &Rc<RefCell<Option<String>>>,
    typed: &str,
    completion: Option<&str>,
) {
    match ghost_display(GHOST_ENABLED, typed, completion) {
        Some(suffix) => {
            ghost_label.set_label(&suffix);
            ghost_popover.popup();
            tab_badge.set_visible(true);
            *current_ghost.borrow_mut() = completion.map(str::to_string);
        }
        None => {
            ghost_popover.popdown();
            tab_badge.set_visible(false);
            *current_ghost.borrow_mut() = None;
        }
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

/// Builds a dropdown row with the match accented and the track count alongside.
fn value_row(suggestion: &AutocompleteSuggestion, input_lower: &str) -> gtk4::ListBoxRow {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);

    let value = highlighted_value(&suggestion.value, input_lower);
    value.set_hexpand(true);

    let count_label = gtk4::Label::builder()
        .label(crate::ui::strings::tag_autocomplete_track_count(
            suggestion.track_count,
        ))
        .css_classes(["dim-label"])
        .build();
    let enter_hint = gtk4::Label::builder()
        .label("↵")
        .css_classes(["reprise-autocomplete-enter-hint"])
        .build();
    enter_hint.set_accessible_role(gtk4::AccessibleRole::Presentation);

    hbox.append(&value);
    hbox.append(&count_label);
    hbox.append(&enter_hint);
    gtk4::ListBoxRow::builder().child(&hbox).build()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MatchSegments<'a> {
    before: &'a str,
    matched: &'a str,
    after: &'a str,
}

fn match_segments<'a>(text: &'a str, input_lower: &str) -> Option<MatchSegments<'a>> {
    if input_lower.is_empty() {
        return None;
    }
    let text_lower = text.to_lowercase();
    let start = text_lower.find(input_lower)?;
    let end = start + input_lower.len();
    if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return None;
    }
    Some(MatchSegments {
        before: &text[..start],
        matched: &text[start..end],
        after: &text[end..],
    })
}

fn highlighted_value(text: &str, input_lower: &str) -> gtk4::Box {
    let value = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    let segments = match_segments(text, input_lower);
    let parts = segments.map_or([(text, false), ("", false), ("", false)], |segments| {
        [
            (segments.before, false),
            (segments.matched, true),
            (segments.after, false),
        ]
    });
    for (index, (part, matched)) in parts.into_iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let label = gtk4::Label::builder()
            .label(part)
            .xalign(0.0)
            .css_classes(["reprise-autocomplete-value"])
            .build();
        if matched {
            label.add_css_class("reprise-autocomplete-match");
        }
        if !matched {
            label.set_ellipsize(pango::EllipsizeMode::End);
            label.set_max_width_chars(1);
        }
        if index == 2 || segments.is_none() {
            label.set_hexpand(true);
        }
        value.append(&label);
    }
    value
}

/// Builds the trailing "Use “X” as new …" row (TAG-6) — always present,
/// literally quoting the typed text, never blocked by any match state.
fn use_as_new_row(text: &str, column: AutocompleteColumn) -> gtk4::ListBoxRow {
    let label = gtk4::Label::builder()
        .label(use_as_new_text(column, text))
        .xalign(0.0)
        .hexpand(true)
        .max_width_chars(1)
        .ellipsize(pango::EllipsizeMode::End)
        .css_classes(["reprise-autocomplete-use-as-new"])
        .build();
    let row = gtk4::ListBoxRow::builder().child(&label).build();
    row.add_css_class("reprise-autocomplete-use-as-new-row");
    row
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

    #[test]
    fn tag_6_match_segments_preserve_prefix_match_and_suffix() {
        assert_eq!(
            match_segments("Radio Cognac", "cog"),
            Some(MatchSegments {
                before: "Radio ",
                matched: "Cog",
                after: "nac",
            })
        );
        assert_eq!(match_segments("Cogitations", "cog").unwrap().matched, "Cog");
        assert_eq!(match_segments("Perpetual Rain", "cog"), None);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn tag_6_dropdown_is_anchored_to_the_field_start() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let entry = AutocompleteEntry::new(
            "Artist",
            AutocompleteColumn::Artist,
            Rc::new(RefCell::new(Connection::open_in_memory().unwrap())),
        );
        assert_eq!(entry.popover.position(), gtk4::PositionType::Bottom);
        assert_eq!(entry.popover.halign(), gtk4::Align::Start);
        assert!(entry
            .popover
            .child()
            .unwrap()
            .has_css_class("reprise-autocomplete-menu"));
    }

    #[test]
    fn ghost_suffix_case_insensitive_prefix() {
        assert_eq!(
            ghost_suffix("Cog", "Cognac").as_deref(),
            Some("nac"),
            "the completion keeps its own casing beyond the typed prefix"
        );
        assert_eq!(ghost_suffix("cog", "Cognac").as_deref(), Some("nac"));
    }

    #[test]
    fn ghost_suffix_none_when_not_a_prefix() {
        // "ognac" is a substring of "Cognac" but never a prefix completion
        // of what's typed — the ghost must not offer it (TAG-7).
        assert_eq!(ghost_suffix("ognac", "Cognac"), None);
    }

    #[test]
    fn ghost_suffix_none_when_nothing_left_to_complete() {
        assert_eq!(ghost_suffix("Cognac", "Cognac"), None);
        assert_eq!(ghost_suffix("", "Cognac"), None);
    }

    #[test]
    fn tag_7a_tab_accepts_only_visible_ghost() {
        match tab_action(Some("nac")) {
            TabAction::AcceptGhost(text) => assert_eq!(text, "nac"),
            TabAction::MoveFocus => panic!("expected AcceptGhost with a visible ghost"),
        }
    }

    #[test]
    fn tag_7a_tab_moves_focus_without_ghost() {
        assert_eq!(tab_action(None), TabAction::MoveFocus);
    }

    #[test]
    fn tag_7a_ghost_disabled_hides_badge() {
        // Disabled: no ghost, regardless of a real completion being available
        // — this is the badge's visibility gate too, since the badge only
        // shows when `ghost_display` yields `Some`.
        assert_eq!(ghost_display(false, "Cog", Some("Cognac")), None);
        // Enabled: the same input does produce a ghost.
        assert_eq!(
            ghost_display(true, "Cog", Some("Cognac")).as_deref(),
            Some("nac")
        );
    }

    #[test]
    fn ghost_display_none_without_a_completion() {
        assert_eq!(ghost_display(true, "Cog", None), None);
    }
}
