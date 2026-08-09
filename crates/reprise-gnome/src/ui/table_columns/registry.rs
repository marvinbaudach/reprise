//! A typed registry connecting one table's keys to its GTK columns.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::gio::prelude::*;
use reprise_core::db::Db;
use reprise_core::library::settings;
use reprise_view::columns::{layout, ColumnKey, Layout};

use super::{ColumnDescriptor, EditorModel};

#[derive(Debug, Clone, Copy)]
pub(in crate::ui) struct TableKeys {
    pub layout: &'static str,
    pub widths: &'static str,
}

type Labeler<K> = Rc<dyn Fn(K) -> String>;
type WidthPolicy<K> = Rc<dyn Fn(K) -> i32>;

pub(in crate::ui) struct ColumnRegistry<K: ColumnKey> {
    pub(super) view: gtk4::ColumnView,
    pub(super) conn: Rc<Db>,
    pub(super) keys: TableKeys,
    pub(super) columns: HashMap<K, gtk4::ColumnViewColumn>,
    /// Suppresses the columns-model listener while `apply` rebuilds the list
    /// programmatically. Only a genuine header drag may persist from there.
    pub(super) syncing_order: Rc<Cell<bool>>,
    /// Suppresses width listeners while defaults and stored widths are being
    /// installed, so setup is never mistaken for a user's header drag.
    pub(super) syncing_width: Rc<Cell<bool>>,
    current_layout: RefCell<Layout<K>>,
    #[cfg(test)]
    layout_settings_reads: Cell<usize>,
    label: RefCell<Option<Labeler<K>>>,
    width_policy: RefCell<Option<WidthPolicy<K>>>,
    preferred_filler: Cell<Option<K>>,
}

impl<K: ColumnKey> ColumnRegistry<K> {
    pub(in crate::ui) fn new(
        view: &gtk4::ColumnView,
        conn: Rc<Db>,
        keys: TableKeys,
        columns: Vec<(K, gtk4::ColumnViewColumn)>,
    ) -> Rc<Self> {
        let stored = settings::get_setting(&conn, keys.layout)
            .map_err(|error| tracing::warn!(%error, key = keys.layout, "could not load stored column layout"))
            .ok()
            .flatten();
        let layout = stored
            .as_deref()
            .and_then(layout::parse::<K>)
            .unwrap_or_default();
        let canonical = layout::serialize(&layout);
        let registry = Rc::new(Self {
            view: view.clone(),
            conn,
            keys,
            columns: columns.into_iter().collect(),
            syncing_order: Rc::new(Cell::new(false)),
            syncing_width: Rc::new(Cell::new(false)),
            current_layout: RefCell::new(layout),
            #[cfg(test)]
            layout_settings_reads: Cell::new(1),
            label: RefCell::new(None),
            width_policy: RefCell::new(None),
            preferred_filler: Cell::new(None),
        });
        if stored.as_deref() != Some(&canonical) {
            registry.persist_value(&canonical);
        }
        registry
    }

    pub(in crate::ui) fn apply(&self, layout: &Layout<K>) {
        let layout = layout::normalize(layout.order.clone(), layout.visible.clone());
        self.current_layout.replace(layout.clone());
        let sort_fallback = self.sort_fallback(&layout);
        // Visibility is a property flip, never a removal: selection,
        // horizontal scroll and the active sort widget remain intact.
        for (key, column) in &self.columns {
            column.set_visible(layout.visible.contains(key));
        }
        match sort_fallback {
            SortFallback::Keep => {}
            SortFallback::Use(key) => {
                if let Some(column) = self.columns.get(&key) {
                    self.view
                        .sort_by_column(Some(column), gtk4::SortType::Ascending);
                }
            }
            SortFallback::Clear => self
                .view
                .sort_by_column(None::<&gtk4::ColumnViewColumn>, gtk4::SortType::Ascending),
        }
        if !self.syncing_width.get() {
            let filler = self
                .preferred_filler
                .get()
                .and_then(|preferred| filler_for(&layout, preferred));
            for (key, column) in &self.columns {
                column.set_expand(Some(*key) == filler);
            }
        }

        // Only rebuild the list when order changed. Rebuilding on a plain
        // visibility flip resets the horizontal scroll offset.
        let desired: Vec<K> = layout
            .order
            .iter()
            .copied()
            .filter(|key| self.columns.contains_key(key))
            .collect();
        if self.current_order() == desired {
            return;
        }
        self.syncing_order.set(true);
        for column in self.columns.values() {
            self.view.remove_column(column);
        }
        for key in &desired {
            if let Some(column) = self.columns.get(key) {
                self.view.append_column(column);
            }
        }
        self.syncing_order.set(false);
    }

    pub(in crate::ui) fn column(&self, key: K) -> Option<&gtk4::ColumnViewColumn> {
        self.columns.get(&key)
    }

    pub(in crate::ui) fn is_visible(&self, key: K) -> bool {
        self.columns
            .get(&key)
            .is_some_and(gtk4::ColumnViewColumn::is_visible)
    }

    #[cfg(test)]
    pub(in crate::ui) fn view(&self) -> &gtk4::ColumnView {
        &self.view
    }

    pub(in crate::ui) fn reset(&self) {
        self.syncing_width.set(true);
        if let Some(width) = self.width_policy.borrow().as_ref() {
            for (key, column) in &self.columns {
                column.set_fixed_width(width(*key));
            }
        }
        self.syncing_width.set(false);
        let layout = Layout::<K>::default();
        self.apply(&layout);
        self.persist(&layout);
    }

    pub(in crate::ui) fn layout(&self) -> Layout<K> {
        self.current_layout.borrow().clone()
    }

    #[cfg(test)]
    pub(in crate::ui) fn layout_settings_read_count(&self) -> usize {
        self.layout_settings_reads.get()
    }

    pub(super) fn configure(&self, label: Labeler<K>, width: WidthPolicy<K>, filler: K) {
        *self.label.borrow_mut() = Some(label);
        *self.width_policy.borrow_mut() = Some(width);
        self.preferred_filler.set(Some(filler));
    }

    pub(super) fn width_policy(&self) -> Option<WidthPolicy<K>> {
        self.width_policy.borrow().clone()
    }

    pub(in crate::ui) fn current_order(&self) -> Vec<K> {
        let model = self.view.columns();
        (0..model.n_items())
            .filter_map(|index| {
                let column = model
                    .item(index)?
                    .downcast::<gtk4::ColumnViewColumn>()
                    .ok()?;
                self.columns
                    .iter()
                    .find(|(_, candidate)| **candidate == column)
                    .map(|(key, _)| *key)
            })
            .collect()
    }

    fn sort_fallback(&self, layout: &Layout<K>) -> SortFallback<K> {
        let primary = self
            .view
            .sorter()
            .and_downcast::<gtk4::ColumnViewSorter>()
            .and_then(|sorter| sorter.primary_sort_column())
            .and_then(|column| {
                self.columns
                    .iter()
                    .find(|(_, candidate)| **candidate == column)
                    .map(|(key, _)| *key)
            });
        sort_fallback(layout, primary, |key| {
            self.columns
                .get(&key)
                .is_some_and(|column| column.sorter().is_some())
        })
    }

    pub(super) fn persist(&self, layout: &Layout<K>) {
        let layout = layout::normalize(layout.order.clone(), layout.visible.clone());
        self.current_layout.replace(layout.clone());
        self.persist_value(&layout::serialize(&layout));
    }

    pub(super) fn persist_value(&self, serialized: &str) {
        if let Err(error) = settings::set_setting(&self.conn, self.keys.layout, serialized) {
            tracing::warn!(
                %error,
                key = self.keys.layout,
                message = %crate::ui::strings::text(crate::ui::strings::COLUMN_LAYOUT_SAVE_FAILED),
                "could not persist column layout"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortFallback<K> {
    Keep,
    Use(K),
    Clear,
}

/// Chooses the replacement for a primary sort column that the new layout
/// hides. Pinned columns are excluded even when they carry GTK's dummy sorter
/// for header geometry; they do not represent user-visible sort fields.
fn sort_fallback<K: ColumnKey>(
    layout: &Layout<K>,
    primary: Option<K>,
    sortable: impl Fn(K) -> bool,
) -> SortFallback<K> {
    let Some(primary) = primary else {
        return SortFallback::Keep;
    };
    if layout.visible.contains(&primary) {
        return SortFallback::Keep;
    }
    layout
        .order
        .iter()
        .copied()
        .find(|key| key.pin().is_none() && layout.visible.contains(key) && sortable(*key))
        .map_or(SortFallback::Clear, SortFallback::Use)
}

impl<K: ColumnKey> EditorModel for ColumnRegistry<K> {
    fn title(&self) -> String {
        crate::ui::strings::text(crate::ui::strings::EDIT_COLUMN_LAYOUT)
    }

    fn columns(&self) -> Vec<ColumnDescriptor> {
        let layout = self.layout();
        let label = self.label.borrow();
        layout
            .order
            .into_iter()
            .filter(|key| key.pin().is_none())
            .map(|key| ColumnDescriptor {
                id: key.as_str().to_owned(),
                label: label
                    .as_ref()
                    .map_or_else(|| key.as_str().to_owned(), |label| label(key)),
            })
            .collect()
    }

    fn is_visible(&self, id: &str) -> bool {
        K::parse(id).is_some_and(|key| self.layout().visible.contains(&key))
    }

    fn set_visible(&self, id: &str, visible: bool) {
        let Some(key) = K::parse(id) else {
            return;
        };
        let next = layout::set_visible(&self.layout(), key, visible);
        self.apply(&next);
        self.persist(&next);
    }

    fn move_column(&self, id: &str, target: &str, after: bool) {
        let (Some(key), Some(target)) = (K::parse(id), K::parse(target)) else {
            return;
        };
        let current = self.layout();
        let next = if after {
            layout::move_after(&current, key, target)
        } else {
            layout::move_before(&current, key, target)
        };
        self.apply(&next);
        self.persist(&next);
    }

    fn reset(&self) {
        ColumnRegistry::reset(self);
    }
}

/// The column that absorbs leftover width: the preferred filler while it is
/// visible, otherwise the first visible free column in the user's order.
pub(in crate::ui) fn filler_for<K: ColumnKey>(layout: &Layout<K>, preferred: K) -> Option<K> {
    if layout.visible.contains(&preferred) {
        return Some(preferred);
    }
    layout
        .order
        .iter()
        .copied()
        .find(|key| key.pin().is_none() && layout.visible.contains(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::prelude::*;
    use reprise_view::columns::{ColumnId, ReleaseColumn};

    #[test]
    fn hiding_primary_sort_chooses_first_visible_sortable_free_column() {
        let layout = reprise_view::columns::layout::set_visible(
            &reprise_view::columns::Layout::<ReleaseColumn>::default(),
            ReleaseColumn::Title,
            false,
        );

        assert_eq!(
            sort_fallback(&layout, Some(ReleaseColumn::Title), |_| true),
            SortFallback::Use(ReleaseColumn::Date)
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_10_hiding_the_sorted_column_keeps_a_visible_sort_indicator() {
        fn sortable_column(key: ColumnId) -> gtk4::ColumnViewColumn {
            let column = gtk4::ColumnViewColumn::builder()
                .title(key.as_str())
                .id(key.as_str())
                .build();
            column.set_sorter(Some(&gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal)));
            column
        }

        fn count_primary_indicators(widget: &gtk4::Widget) -> usize {
            let own = usize::from(
                widget.css_name() == "sort-indicator"
                    && widget.has_css_class("reprise-primary-sort-indicator"),
            );
            let mut total = own;
            let mut child = widget.first_child();
            while let Some(current) = child {
                total += count_primary_indicators(&current);
                child = current.next_sibling();
            }
            total
        }

        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
        let title = sortable_column(ColumnId::Title);
        let artist = sortable_column(ColumnId::Artist);
        view.append_column(&title);
        view.append_column(&artist);
        let store = gtk4::gio::ListStore::new::<gtk4::glib::Object>();
        let sorted = gtk4::SortListModel::new(Some(store), view.sorter());
        view.set_model(Some(&gtk4::NoSelection::new(Some(sorted))));
        crate::ui::track_list::track_list_header_style::mark(&view);
        let registry = ColumnRegistry::new(
            &view,
            Rc::new(crate::test_db::open().unwrap()),
            TableKeys {
                layout: settings::COLUMN_LAYOUT_KEY,
                widths: settings::COLUMN_WIDTHS_KEY,
            },
            vec![
                (ColumnId::Title, title.clone()),
                (ColumnId::Artist, artist.clone()),
            ],
        );
        registry.apply(&registry.layout());
        view.sort_by_column(Some(&artist), gtk4::SortType::Descending);
        let window = gtk4::Window::builder()
            .default_width(500)
            .default_height(160)
            .child(&view)
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        EditorModel::set_visible(registry.as_ref(), ColumnId::Artist.as_str(), false);
        while gtk4::glib::MainContext::default().iteration(false) {}

        let sorter = view
            .sorter()
            .and_downcast::<gtk4::ColumnViewSorter>()
            .expect("ColumnView owns its aggregate sorter");
        assert_eq!(sorter.primary_sort_column().as_ref(), Some(&title));
        assert_eq!(sorter.primary_sort_order(), gtk4::SortType::Ascending);
        assert_eq!(
            count_primary_indicators(view.upcast_ref()),
            1,
            "the visible fallback sort must retain exactly one header indicator"
        );
        window.close();
    }

    /// STYLE-10: the filler role is not welded to one column. Hiding the
    /// filler moves it to the first visible free column, or the table stops
    /// absorbing its own slack — which is the gap the music library has had
    /// since Title became hideable.
    #[test]
    fn style_10_the_filler_moves_when_it_is_hidden() {
        let layout = reprise_view::columns::layout::set_visible(
            &reprise_view::columns::Layout::<ReleaseColumn>::default(),
            ReleaseColumn::Title,
            false,
        );
        assert_eq!(
            filler_for(&layout, ReleaseColumn::Title),
            Some(ReleaseColumn::Date),
            "with Title hidden, Date is the first visible free column"
        );
        assert_eq!(
            filler_for(
                &reprise_view::columns::Layout::<ReleaseColumn>::default(),
                ReleaseColumn::Title
            ),
            Some(ReleaseColumn::Title),
            "a visible preferred filler keeps the role"
        );
    }
}
