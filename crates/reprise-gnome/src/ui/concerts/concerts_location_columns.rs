use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_view::columns::{ColumnKey, ConcertColumn};

use super::concerts_columns::SortColumns;
use crate::ui::table_columns::registry::ColumnRegistry;
use crate::ui::table_columns::{ColumnDescriptor, EditorModel};

struct LocationAwareEditorModel {
    registry: Rc<ColumnRegistry<ConcertColumn>>,
    has_location: Rc<Cell<bool>>,
}

impl EditorModel for LocationAwareEditorModel {
    fn title(&self) -> String {
        EditorModel::title(self.registry.as_ref())
    }

    fn columns(&self) -> Vec<ColumnDescriptor> {
        EditorModel::columns(self.registry.as_ref())
            .into_iter()
            .filter(|column| {
                self.has_location.get() || column.id != ConcertColumn::Distance.as_str()
            })
            .collect()
    }

    fn sortable_columns(&self) -> Vec<ColumnDescriptor> {
        EditorModel::sortable_columns(self.registry.as_ref())
    }

    fn sort(&self) -> Option<(String, gtk4::SortType)> {
        EditorModel::sort(self.registry.as_ref())
    }

    fn set_sort(&self, id: &str, order: gtk4::SortType) {
        EditorModel::set_sort(self.registry.as_ref(), id, order);
    }

    fn is_visible(&self, id: &str) -> bool {
        if id == ConcertColumn::Distance.as_str() && !self.has_location.get() {
            return false;
        }
        EditorModel::is_visible(self.registry.as_ref(), id)
    }

    fn set_visible(&self, id: &str, visible: bool) {
        if id == ConcertColumn::Distance.as_str() && !self.has_location.get() {
            return;
        }
        EditorModel::set_visible(self.registry.as_ref(), id, visible);
    }

    fn move_column(&self, id: &str, target: &str, after: bool) {
        EditorModel::move_column(self.registry.as_ref(), id, target, after);
    }

    fn reset(&self) {
        EditorModel::reset(self.registry.as_ref());
    }
}

pub(super) struct LocationColumns {
    registry: Rc<ColumnRegistry<ConcertColumn>>,
    view: gtk4::ColumnView,
    columns: SortColumns,
    has_location: Rc<Cell<bool>>,
    distance_sorter: Option<gtk4::Sorter>,
    venue_expand_before_hide: Cell<Option<bool>>,
    distance_sort_before_hide: Cell<Option<gtk4::SortType>>,
}

impl LocationColumns {
    pub(super) fn new(
        registry: Rc<ColumnRegistry<ConcertColumn>>,
        view: &gtk4::ColumnView,
        columns: SortColumns,
    ) -> (Self, Rc<dyn EditorModel>) {
        let has_location = Rc::new(Cell::new(false));
        let editor: Rc<dyn EditorModel> = Rc::new(LocationAwareEditorModel {
            registry: registry.clone(),
            has_location: has_location.clone(),
        });
        let distance_sorter = columns.distance.sorter();
        (
            Self {
                registry,
                view: view.clone(),
                columns,
                has_location,
                distance_sorter,
                venue_expand_before_hide: Cell::new(None),
                distance_sort_before_hide: Cell::new(None),
            },
            editor,
        )
    }

    pub(super) fn apply(&self, has_location: bool) {
        self.has_location.set(has_location);
        if has_location {
            let user_wants_distance = self
                .registry
                .layout()
                .visible
                .contains(&ConcertColumn::Distance);
            self.columns.distance.set_visible(user_wants_distance);
            self.columns
                .distance
                .set_sorter(self.distance_sorter.as_ref());
            if let Some(expanded) = self.venue_expand_before_hide.take() {
                self.columns.venue.set_expand(expanded);
            }
            if let Some(order) = self.distance_sort_before_hide.take() {
                self.view
                    .sort_by_column(Some(&self.columns.distance), order);
            }
            return;
        }

        if self.venue_expand_before_hide.get().is_none() {
            self.venue_expand_before_hide
                .set(Some(self.columns.venue.expands()));
        }
        let primary_distance = self
            .view
            .sorter()
            .and_downcast::<gtk4::ColumnViewSorter>()
            .and_then(|sorter| {
                (sorter.primary_sort_column().as_ref() == Some(&self.columns.distance))
                    .then_some(sorter.primary_sort_order())
            });
        if let Some(order) = primary_distance {
            self.distance_sort_before_hide.set(Some(order));
            self.view
                .sort_by_column(Some(&self.columns.date), gtk4::SortType::Ascending);
        }
        self.columns.distance.set_sorter(None::<&gtk4::Sorter>);
        self.columns.distance.set_visible(false);
        self.columns.venue.set_expand(true);
    }

    pub(super) fn sort_by_date(&self) {
        self.view
            .sort_by_column(Some(&self.columns.date), gtk4::SortType::Ascending);
    }

    #[cfg(test)]
    pub(super) fn distance_visible(&self) -> bool {
        self.columns.distance.is_visible()
    }

    #[cfg(test)]
    pub(super) fn distance_sortable(&self) -> bool {
        self.columns.distance.sorter().is_some()
    }

    #[cfg(test)]
    pub(super) fn venue_expands(&self) -> bool {
        self.columns.venue.expands()
    }

    #[cfg(test)]
    pub(super) fn venue_column_id(&self) -> Option<String> {
        self.columns.venue.id().map(|id| id.to_string())
    }

    #[cfg(test)]
    pub(super) fn sort_by_distance(&self, order: gtk4::SortType) {
        self.view
            .sort_by_column(Some(&self.columns.distance), order);
    }

    #[cfg(test)]
    pub(super) fn primary_sort(&self) -> (Option<String>, gtk4::SortType) {
        self.view
            .sorter()
            .and_downcast::<gtk4::ColumnViewSorter>()
            .map_or((None, gtk4::SortType::Ascending), |sorter| {
                (
                    sorter
                        .primary_sort_column()
                        .and_then(|column| column.id())
                        .map(|id| id.to_string()),
                    sorter.primary_sort_order(),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use reprise_view::columns::{ColumnKey, Layout};

    use super::*;

    #[test]
    fn automatic_distance_visibility_never_changes_the_user_layout() {
        let layout = Layout::<ConcertColumn>::default();
        assert!(layout.visible.contains(&ConcertColumn::Distance));
        assert_eq!(ConcertColumn::Distance.as_str(), "distance");
    }
}
