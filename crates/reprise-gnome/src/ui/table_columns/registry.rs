//! A typed registry connecting one table's keys to its GTK columns.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::gio::prelude::*;
use reprise_core::db::Db;
use reprise_core::library::settings;
use reprise_view::columns::{ColumnKey, Layout, layout};

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
        Rc::new(Self {
            view: view.clone(),
            conn,
            keys,
            columns: columns.into_iter().collect(),
            syncing_order: Rc::new(Cell::new(false)),
            syncing_width: Rc::new(Cell::new(false)),
            label: RefCell::new(None),
            width_policy: RefCell::new(None),
            preferred_filler: Cell::new(None),
        })
    }

    pub(in crate::ui) fn apply(&self, layout: &Layout<K>) {
        let layout = layout::normalize(layout.order.clone(), layout.visible.clone());
        // Visibility is a property flip, never a removal: selection,
        // horizontal scroll and the active sort remain intact.
        for (key, column) in &self.columns {
            column.set_visible(layout.visible.contains(key));
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
        let stored = settings::get_setting(&self.conn, self.keys.layout)
            .map_err(|error| tracing::warn!(%error, key = self.keys.layout, "could not load stored column layout"))
            .ok()
            .flatten();
        let layout = stored
            .as_deref()
            .and_then(layout::parse::<K>)
            .unwrap_or_default();
        let canonical = layout::serialize(&layout);
        if stored.as_deref() != Some(&canonical) {
            self.persist_value(&canonical);
        }
        layout
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

    pub(super) fn persist(&self, layout: &Layout<K>) {
        self.persist_value(&layout::serialize(layout));
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
    use reprise_view::columns::ReleaseColumn;

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
