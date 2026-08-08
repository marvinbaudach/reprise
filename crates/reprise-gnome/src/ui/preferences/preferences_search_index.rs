use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::preferences_window::{PageId, PAGE_ORDER};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SearchDocument {
    pub(super) page: PageId,
    group: String,
    pub(super) title: String,
    subtitle: String,
}

impl SearchDocument {
    fn new(page: PageId, group: &str, title: &str, subtitle: &str) -> Self {
        Self {
            page,
            group: group.to_owned(),
            title: title.to_owned(),
            subtitle: subtitle.to_owned(),
        }
    }

    #[cfg(test)]
    pub(super) fn path(&self) -> String {
        let page = self.page.title();
        if self.group.trim().is_empty() {
            page
        } else {
            format!("{page} › {}", self.group)
        }
    }

    pub(super) fn group(&self) -> &str {
        &self.group
    }

    #[cfg(test)]
    pub(super) fn matches(&self, query: &str) -> bool {
        self.matches_fields(&self.title, &self.subtitle, query)
    }

    fn matches_fields(&self, title: &str, subtitle: &str, query: &str) -> bool {
        let page = self.page.title();
        reprise_view::search_scope::matches_query(title, query)
            || reprise_view::search_scope::matches_query(subtitle, query)
            || reprise_view::search_scope::matches_query(&page, query)
    }
}

#[derive(Clone)]
pub(super) struct IndexedRow {
    pub(super) document: SearchDocument,
    pub(super) row: adw::PreferencesRow,
}

impl IndexedRow {
    pub(super) fn matches(&self, query: &str) -> bool {
        let subtitle = row_subtitle(&self.row).unwrap_or_default();
        self.document
            .matches_fields(&self.row.title(), &subtitle, query)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct PageHitCounts {
    per_page: [usize; PAGE_ORDER.len()],
}

impl PageHitCounts {
    #[cfg(test)]
    pub(super) fn matching(documents: &[SearchDocument], query: &str) -> Self {
        let mut counts = Self::default();
        for document in documents.iter().filter(|document| document.matches(query)) {
            let Some(index) = PAGE_ORDER.iter().position(|page| *page == document.page) else {
                continue;
            };
            counts.per_page[index] += 1;
        }
        counts
    }

    pub(super) fn from_rows<'a>(rows: impl IntoIterator<Item = &'a IndexedRow>) -> Self {
        let mut counts = Self::default();
        for row in rows {
            let Some(index) = PAGE_ORDER
                .iter()
                .position(|page| *page == row.document.page)
            else {
                continue;
            };
            counts.per_page[index] += 1;
        }
        counts
    }

    pub(super) fn total(&self) -> usize {
        self.per_page.iter().sum()
    }

    pub(super) fn for_page(&self, page: PageId) -> usize {
        PAGE_ORDER
            .iter()
            .position(|candidate| *candidate == page)
            .map_or(0, |index| self.per_page[index])
    }
}

pub(super) fn collect_rows(widget: &gtk4::Widget, page: PageId, index: &mut Vec<IndexedRow>) {
    if let Ok(row) = widget.clone().downcast::<adw::PreferencesRow>() {
        let subtitle = row_subtitle(&row).unwrap_or_default();
        let group = row
            .ancestor(adw::PreferencesGroup::static_type())
            .and_downcast::<adw::PreferencesGroup>()
            .map_or_else(String::new, |group| group.title().to_string());
        index.push(IndexedRow {
            document: SearchDocument::new(page, &group, &row.title(), &subtitle),
            row,
        });
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        collect_rows(&current, page, index);
        child = current.next_sibling();
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_13_settings_index_composes_the_page_and_group_path() {
        let grouped = SearchDocument::new(
            PageId::Plugins,
            "Online content",
            "Cover downloads",
            "coverartarchive.org",
        );
        let ungrouped = SearchDocument::new(PageId::Playback, "", "Crossfade", "");

        assert_eq!(grouped.path(), "Plugins › Online content");
        assert_eq!(ungrouped.path(), "Playback");
    }

    #[test]
    fn set_13_settings_index_matches_title_subtitle_and_page_name() {
        let document = SearchDocument::new(
            PageId::Plugins,
            "Online content",
            "Cover downloads",
            "coverartarchive.org",
        );

        assert!(document.matches("downloads"));
        assert!(document.matches("cover"));
        assert!(document.matches("plugins"));
        assert!(!document.matches("crossfade"));
    }

    #[test]
    fn set_13_settings_index_counts_hits_per_page() {
        let documents = [
            SearchDocument::new(PageId::Playback, "Audio", "Crossfade", "Smooth changes"),
            SearchDocument::new(PageId::Playback, "Audio", "Gapless", "No gaps"),
            SearchDocument::new(
                PageId::Plugins,
                "Online content",
                "Radio",
                "Online stations",
            ),
            SearchDocument::new(PageId::Library, "Folders", "Music folder", "/music"),
        ];

        let counts = PageHitCounts::matching(&documents, "playback");

        assert_eq!(counts.total(), 2);
        assert_eq!(counts.for_page(PageId::Playback), 2);
        assert_eq!(counts.for_page(PageId::Plugins), 0);
        assert_eq!(counts.for_page(PageId::Library), 0);
    }
}
