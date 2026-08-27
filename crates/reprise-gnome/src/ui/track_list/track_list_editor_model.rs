//! Playlist-aware adapter for the music table's shared customization surface.

use std::rc::Rc;

use reprise_core::view_source::ViewSource;

use super::column_layout::ColumnRegistry;
use super::track_list_sort::{self, SortState, PLAYLIST_ORDER_SORT_FIELD};
use super::{Shared, TrackList};
use crate::ui::strings;
use crate::ui::table_columns::{ColumnDescriptor, EditorModel};

struct TrackListEditorModel {
    registry: ColumnRegistry,
    shared: Rc<Shared>,
}

pub(super) fn model(track_list: &Rc<TrackList>) -> Rc<dyn EditorModel> {
    Rc::new(TrackListEditorModel {
        registry: super::column_layout::registry(track_list),
        shared: track_list.shared.clone(),
    })
}

impl EditorModel for TrackListEditorModel {
    fn title(&self) -> String {
        EditorModel::title(self.registry.as_ref())
    }

    fn columns(&self) -> Vec<ColumnDescriptor> {
        EditorModel::columns(self.registry.as_ref())
    }

    fn sortable_columns(&self) -> Vec<ColumnDescriptor> {
        let source = self.shared.source.borrow().clone();
        sortable_columns_for_source(
            &source,
            EditorModel::sortable_columns(self.registry.as_ref()),
        )
    }

    fn sort(&self) -> Option<(String, gtk4::SortType)> {
        let source = self.shared.source.borrow().clone();
        let sort = self.shared.sort.borrow().clone();
        sort_for_source(&source, &sort, EditorModel::sort(self.registry.as_ref()))
    }

    fn set_sort(&self, id: &str, order: gtk4::SortType) {
        let source = self.shared.source.borrow().clone();
        if is_playlist_order(&source, id) {
            track_list_sort::restore_playlist_order(&self.shared, order);
        } else {
            EditorModel::set_sort(self.registry.as_ref(), id, order);
        }
    }

    fn is_visible(&self, id: &str) -> bool {
        EditorModel::is_visible(self.registry.as_ref(), id)
    }

    fn set_visible(&self, id: &str, visible: bool) {
        EditorModel::set_visible(self.registry.as_ref(), id, visible);
    }

    fn move_column(&self, id: &str, target: &str, after: bool) {
        EditorModel::move_column(self.registry.as_ref(), id, target, after);
    }

    fn reset(&self) {
        EditorModel::reset(self.registry.as_ref());
    }
}

fn sortable_columns_for_source(
    source: &ViewSource,
    mut columns: Vec<ColumnDescriptor>,
) -> Vec<ColumnDescriptor> {
    if matches!(source, ViewSource::Playlist(_)) {
        columns.insert(
            0,
            ColumnDescriptor {
                id: PLAYLIST_ORDER_SORT_FIELD.to_owned(),
                label: strings::text(strings::PLAYLIST_ORDER),
            },
        );
    }
    columns
}

fn sort_for_source(
    source: &ViewSource,
    sort: &SortState,
    column_sort: Option<(String, gtk4::SortType)>,
) -> Option<(String, gtk4::SortType)> {
    if !is_playlist_order(source, &sort.field) {
        return column_sort;
    }
    Some((
        PLAYLIST_ORDER_SORT_FIELD.to_owned(),
        if sort.dir == "desc" {
            gtk4::SortType::Descending
        } else {
            gtk4::SortType::Ascending
        },
    ))
}

fn is_playlist_order(source: &ViewSource, id: &str) -> bool {
    matches!(source, ViewSource::Playlist(_)) && id == PLAYLIST_ORDER_SORT_FIELD
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date_column() -> ColumnDescriptor {
        ColumnDescriptor {
            id: "date".to_owned(),
            label: "Date".to_owned(),
        }
    }

    #[test]
    fn playlists_prepend_manual_order_to_the_sortable_descriptor_list() {
        let playlist = sortable_columns_for_source(&ViewSource::Playlist(7), vec![date_column()]);
        assert_eq!(
            playlist
                .iter()
                .map(|column| column.id.as_str())
                .collect::<Vec<_>>(),
            [PLAYLIST_ORDER_SORT_FIELD, "date"]
        );
        assert_eq!(playlist[0].label, strings::text(strings::PLAYLIST_ORDER));

        let library = sortable_columns_for_source(&ViewSource::Library, vec![date_column()]);
        assert_eq!(library.len(), 1);
        assert_eq!(library[0].id, "date");
    }

    #[test]
    fn playlist_order_sentinel_round_trips_without_a_gtk_view() {
        let source = ViewSource::Playlist(7);
        let restored = SortState {
            field: PLAYLIST_ORDER_SORT_FIELD.to_owned(),
            dir: "desc".to_owned(),
        };

        assert!(is_playlist_order(&source, PLAYLIST_ORDER_SORT_FIELD));
        assert_eq!(
            sort_for_source(
                &source,
                &restored,
                Some(("artist".to_owned(), gtk4::SortType::Ascending))
            ),
            Some((
                PLAYLIST_ORDER_SORT_FIELD.to_owned(),
                gtk4::SortType::Descending
            ))
        );
        assert!(!is_playlist_order(
            &ViewSource::Library,
            PLAYLIST_ORDER_SORT_FIELD
        ));
    }
}
