//! Reversible rendering of real Preferences rows as search results.
//!
//! Settings search does not duplicate controls. A prepared result captures a
//! row's exact list origin and presentation, then a moved result owns the
//! temporary wrapper and knows how to restore that row byte-for-byte.

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use super::preferences_search_index::{IndexedRow, SearchDocument};
use crate::ui::preferences_window::PageId;

struct RowOrigin {
    parent: gtk4::ListBox,
    index: i32,
    title: String,
    subtitle: Option<String>,
    used_markup: bool,
    was_visible: bool,
    expanders: Vec<adw::ExpanderRow>,
}

pub(super) struct PreparedResult {
    indexed: IndexedRow,
    origin: RowOrigin,
}

impl PreparedResult {
    pub(super) fn capture(indexed: IndexedRow) -> Option<Self> {
        let origin = capture_origin(&indexed.row)?;
        Some(Self { indexed, origin })
    }

    pub(super) fn indexed(&self) -> &IndexedRow {
        &self.indexed
    }

    pub(super) fn render(
        self,
        query: &str,
        on_open: impl Fn(PageId, adw::PreferencesRow) + 'static,
    ) -> MovedResult {
        let row = self.indexed.row;
        let palette = crate::ui::search_highlight::accent_palette(&row);
        self.origin.parent.remove(&row);
        row.set_visible(true);
        apply_highlight(
            &row,
            &self.origin.title,
            self.origin.subtitle.as_deref(),
            query,
            &palette,
        );

        let path_label = gtk4::Label::new(None);
        path_label.set_use_markup(true);
        path_label.set_markup(&path_markup(&self.indexed.document, query, &palette));
        path_label.add_css_class("caption");
        path_label.add_css_class("dim-label");
        path_label.set_xalign(0.0);
        let path_button = gtk4::Button::new();
        path_button.add_css_class("flat");
        path_button.add_css_class("reprise-settings-result-path");
        path_button.set_halign(gtk4::Align::Start);
        path_button.set_child(Some(&path_label));
        let result_list = gtk4::ListBox::new();
        result_list.add_css_class("boxed-list");
        result_list.set_selection_mode(gtk4::SelectionMode::None);
        result_list.append(&row);
        let wrapper = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
        wrapper.append(&path_button);
        wrapper.append(&result_list);

        let page = self.indexed.document.page;
        let target = row.clone();
        path_button.connect_clicked(move |_| on_open(page, target.clone()));

        MovedResult {
            row,
            origin: self.origin,
            #[cfg(test)]
            document: self.indexed.document,
            result_list,
            wrapper,
            #[cfg(test)]
            path_button,
        }
    }
}

pub(super) struct MovedResult {
    row: adw::PreferencesRow,
    origin: RowOrigin,
    #[cfg(test)]
    document: SearchDocument,
    result_list: gtk4::ListBox,
    wrapper: gtk4::Box,
    #[cfg(test)]
    path_button: gtk4::Button,
}

impl MovedResult {
    pub(super) fn widget(&self) -> &gtk4::Box {
        &self.wrapper
    }

    pub(super) fn matches(&self, row: &adw::PreferencesRow) -> bool {
        self.row == *row
    }

    pub(super) fn expanders(&self) -> Vec<adw::ExpanderRow> {
        self.origin.expanders.clone()
    }

    pub(super) fn restore(self) {
        self.result_list.remove(&self.row);
        self.origin.parent.insert(&self.row, self.origin.index);
        self.row.set_use_markup(self.origin.used_markup);
        self.row.set_title(&self.origin.title);
        self.row.set_visible(self.origin.was_visible);
        if let Some(parent) = self
            .wrapper
            .parent()
            .and_then(|parent| parent.downcast::<gtk4::Box>().ok())
        {
            parent.remove(&self.wrapper);
        }
        if let Some(subtitle) = self.origin.subtitle {
            set_row_subtitle(&self.row, "");
            set_row_subtitle(&self.row, &subtitle);
        }
    }

    #[cfg(test)]
    pub(super) fn origin(&self) -> TestOrigin {
        TestOrigin {
            parent: self.origin.parent.clone(),
            index: self.origin.index,
            subtitle: self.origin.subtitle.clone(),
        }
    }

    #[cfg(test)]
    pub(super) fn path(&self) -> String {
        self.document.path()
    }

    #[cfg(test)]
    pub(super) fn path_button(&self) -> gtk4::Button {
        self.path_button.clone()
    }
}

fn capture_origin(row: &adw::PreferencesRow) -> Option<RowOrigin> {
    let parent = row
        .parent()
        .and_then(|parent| parent.downcast::<gtk4::ListBox>().ok())?;
    Some(RowOrigin {
        parent,
        index: row.index(),
        title: row.title().to_string(),
        subtitle: row_subtitle(row),
        used_markup: row.uses_markup(),
        was_visible: row.is_visible(),
        expanders: ancestor_expanders(row),
    })
}

fn ancestor_expanders(row: &adw::PreferencesRow) -> Vec<adw::ExpanderRow> {
    let mut expanders = Vec::new();
    let mut ancestor = row.parent();
    while let Some(widget) = ancestor {
        if let Ok(expander) = widget.clone().downcast::<adw::ExpanderRow>() {
            expanders.push(expander);
        }
        ancestor = widget.parent();
    }
    expanders
}

fn apply_highlight(
    row: &adw::PreferencesRow,
    title: &str,
    subtitle: Option<&str>,
    query: &str,
    palette: &crate::ui::search_highlight::HighlightPalette,
) {
    row.set_use_markup(true);
    let title = crate::ui::search_highlight::highlight_markup(title, query, Some(palette))
        .unwrap_or_else(|| gtk4::glib::markup_escape_text(title).to_string());
    row.set_title(&title);
    let Some(subtitle) = subtitle else {
        return;
    };
    let subtitle = crate::ui::search_highlight::highlight_markup(subtitle, query, Some(palette))
        .unwrap_or_else(|| gtk4::glib::markup_escape_text(subtitle).to_string());
    set_row_subtitle(row, &subtitle);
}

fn row_subtitle(row: &adw::PreferencesRow) -> Option<String> {
    if let Ok(action) = row.clone().downcast::<adw::ActionRow>() {
        return action.subtitle().map(|subtitle| subtitle.to_string());
    }
    row.clone()
        .downcast::<adw::ExpanderRow>()
        .ok()
        .map(|row| row.subtitle())
        .map(|subtitle| subtitle.to_string())
        .filter(|subtitle| !subtitle.is_empty())
}

fn set_row_subtitle(row: &adw::PreferencesRow, subtitle: &str) {
    if let Ok(action) = row.clone().downcast::<adw::ActionRow>() {
        action.set_subtitle(subtitle);
    } else if let Ok(expander) = row.clone().downcast::<adw::ExpanderRow>() {
        expander.set_subtitle(subtitle);
    }
}

fn path_markup(
    document: &SearchDocument,
    query: &str,
    palette: &crate::ui::search_highlight::HighlightPalette,
) -> String {
    let page = document.page.title();
    let page = crate::ui::search_highlight::highlight_markup(&page, query, Some(palette))
        .unwrap_or_else(|| gtk4::glib::markup_escape_text(&page).to_string());
    if document.group().trim().is_empty() {
        page
    } else {
        format!(
            "{page} › {}",
            gtk4::glib::markup_escape_text(document.group())
        )
    }
}

#[cfg(test)]
pub(super) struct TestOrigin {
    pub(super) parent: gtk4::ListBox,
    pub(super) index: i32,
    pub(super) subtitle: Option<String>,
}
