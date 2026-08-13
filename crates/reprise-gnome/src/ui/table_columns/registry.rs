//! A typed registry connecting one table's keys to its GTK columns.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::gio::prelude::*;
use reprise_core::db::Db;
use reprise_core::library::settings;
use reprise_view::columns::{layout, ColumnKey, Layout, Pin};

use super::{ColumnDescriptor, EditorModel};

#[derive(Debug, Clone, Copy)]
pub(in crate::ui) struct TableKeys {
    pub layout: &'static str,
    pub widths: &'static str,
}

type Labeler<K> = Rc<dyn Fn(K) -> String>;
type WidthPolicy<K> = Rc<dyn Fn(K) -> i32>;

pub(in crate::ui) fn bind_columns_by_id<K: ColumnKey>(
    view: &gtk4::ColumnView,
) -> Vec<(K, gtk4::ColumnViewColumn)> {
    let model = view.columns();
    let columns = (0..model.n_items())
        .map(|index| {
            model
                .item(index)
                .and_downcast::<gtk4::ColumnViewColumn>()
                .unwrap_or_else(|| panic!("column view item {index} is not a ColumnViewColumn"))
        })
        .collect::<Vec<_>>();
    let ids = columns
        .iter()
        .map(|column| column.id().map(|id| id.to_string()))
        .collect::<Vec<_>>();
    let id_refs = ids.iter().map(Option::as_deref).collect::<Vec<_>>();
    let keys = bind_view_column_keys::<K>(&id_refs).unwrap_or_else(|error| {
        panic!(
            "invalid {} column binding: {error}",
            std::any::type_name::<K>()
        )
    });
    keys.into_iter().zip(columns).collect()
}

fn bind_view_column_keys<K: ColumnKey>(ids: &[Option<&str>]) -> Result<Vec<K>, String> {
    let leading = K::all()
        .iter()
        .copied()
        .filter(|key| key.pin() == Some(Pin::Leading))
        .collect::<Vec<_>>();
    let trailing = K::all()
        .iter()
        .copied()
        .filter(|key| key.pin() == Some(Pin::Trailing))
        .collect::<Vec<_>>();
    let first_named = ids.iter().position(Option::is_some).unwrap_or(ids.len());
    let after_last_named = ids
        .iter()
        .rposition(Option::is_some)
        .map_or(first_named, |index| index + 1);
    let mut keys = Vec::with_capacity(ids.len());
    let mut leading_index = 0;
    let mut trailing_index = 0;

    for (index, id) in ids.iter().enumerate() {
        let key = match id {
            Some(id) => {
                let key = K::parse(id)
                    .ok_or_else(|| format!("widget id `{id}` is not a declared column key"))?;
                if key.pin().is_some() {
                    return Err(format!(
                        "pinned column `{id}` must not expose an editable id"
                    ));
                }
                key
            }
            None if index < first_named => {
                let key = leading.get(leading_index).copied().ok_or_else(|| {
                    format!("unexpected unnamed leading column at physical index {index}")
                })?;
                leading_index += 1;
                key
            }
            None if index >= after_last_named => {
                let key = trailing.get(trailing_index).copied().ok_or_else(|| {
                    format!("unexpected unnamed trailing column at physical index {index}")
                })?;
                trailing_index += 1;
                key
            }
            None => {
                return Err(format!(
                    "non-pinned column at physical index {index} has no widget id"
                ));
            }
        };
        if keys.contains(&key) {
            return Err(format!("column `{}` is bound more than once", key.as_str()));
        }
        keys.push(key);
    }

    for key in K::all() {
        if !keys.contains(key) {
            return Err(format!("column `{}` has no widget binding", key.as_str()));
        }
    }
    Ok(keys)
}

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

fn parse_editor_key<K: ColumnKey>(id: &str, operation: &str, role: &str) -> Option<K> {
    let key = K::parse(id);
    if key.is_none() {
        tracing::warn!(
            operation,
            role,
            column_id = id,
            column_type = std::any::type_name::<K>(),
            "unknown column id"
        );
    }
    key
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
        let Some(key) = parse_editor_key::<K>(id, "set_visible", "column") else {
            return;
        };
        let next = layout::set_visible(&self.layout(), key, visible);
        self.apply(&next);
        self.persist(&next);
        tracing::debug!(
            column = key.as_str(),
            visible,
            "column header visibility changed"
        );
    }

    fn move_column(&self, id: &str, target: &str, after: bool) {
        let Some(key) = parse_editor_key::<K>(id, "move_column", "column") else {
            return;
        };
        let Some(target) = parse_editor_key::<K>(target, "move_column", "target") else {
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
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;

    #[derive(Clone, Default)]
    struct CapturedWarnings(Arc<Mutex<Vec<u8>>>);

    struct WarningWriter(Arc<Mutex<Vec<u8>>>);

    impl<'a> MakeWriter<'a> for CapturedWarnings {
        type Writer = WarningWriter;

        fn make_writer(&'a self) -> Self::Writer {
            WarningWriter(Arc::clone(&self.0))
        }
    }

    impl Write for WarningWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn capture_warnings(operation: impl FnOnce()) -> String {
        let output = CapturedWarnings::default();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_target(false)
            .with_max_level(tracing::Level::WARN)
            .with_writer(output.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, operation);
        let bytes = output.0.lock().unwrap().clone();
        String::from_utf8(bytes).unwrap()
    }

    fn capture_debug(operation: impl FnOnce()) -> String {
        let output = CapturedWarnings::default();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_target(false)
            .with_max_level(tracing::Level::DEBUG)
            .with_writer(output.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, operation);
        let bytes = output.0.lock().unwrap().clone();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn invalid_editor_column_ids_are_logged_with_the_rejected_role() {
        let logs = capture_warnings(|| {
            assert_eq!(
                parse_editor_key::<ReleaseColumn>("missing", "set_visible", "column"),
                None
            );
            assert_eq!(
                parse_editor_key::<ReleaseColumn>("also-missing", "move_column", "target"),
                None
            );
        });

        assert!(logs.contains("unknown column id"), "{logs}");
        assert!(logs.contains("operation=\"set_visible\""), "{logs}");
        assert!(logs.contains("column_id=\"missing\""), "{logs}");
        assert!(logs.contains("operation=\"move_column\""), "{logs}");
        assert!(logs.contains("role=\"target\""), "{logs}");
        assert!(logs.contains("column_id=\"also-missing\""), "{logs}");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn column_visibility_change_logs_the_column_and_new_value() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
        let artist = gtk4::ColumnViewColumn::builder()
            .title("Artist")
            .id(ReleaseColumn::Artist.as_str())
            .build();
        view.append_column(&artist);
        let registry = ColumnRegistry::new(
            &view,
            Rc::new(crate::test_db::open().unwrap()),
            TableKeys {
                layout: "test.visibility-log.layout",
                widths: "test.visibility-log.widths",
            },
            vec![(ReleaseColumn::Artist, artist)],
        );

        let logs = capture_debug(|| {
            EditorModel::set_visible(registry.as_ref(), ReleaseColumn::Artist.as_str(), false);
        });

        assert!(logs.contains("column header visibility changed"), "{logs}");
        assert!(logs.contains("column=\"artist\""), "{logs}");
        assert!(logs.contains("visible=false"), "{logs}");
    }

    #[test]
    fn column_bindings_follow_widget_ids_instead_of_enum_positions() {
        let keys = bind_view_column_keys::<ReleaseColumn>(&[
            None,
            Some("type"),
            Some("date"),
            Some("title"),
            Some("artist"),
            None,
            None,
        ])
        .expect("complete release binding");

        assert_eq!(
            keys,
            vec![
                ReleaseColumn::Cover,
                ReleaseColumn::Type,
                ReleaseColumn::Date,
                ReleaseColumn::Title,
                ReleaseColumn::Artist,
                ReleaseColumn::Status,
                ReleaseColumn::Buy,
            ]
        );
    }

    #[test]
    fn column_bindings_reject_an_unidentified_free_column() {
        let error = bind_view_column_keys::<ReleaseColumn>(&[
            None,
            Some("date"),
            None,
            Some("artist"),
            Some("type"),
            None,
            None,
        ])
        .expect_err("Title has no widget id");

        assert!(error.contains("non-pinned column"), "{error}");
    }

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

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_10_hiding_the_filler_moves_realized_space_to_the_next_column() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();

        let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
        let cover = gtk4::ColumnViewColumn::builder()
            .title("Cover")
            .fixed_width(40)
            .build();
        let title = gtk4::ColumnViewColumn::builder()
            .title("Title")
            .id(ColumnId::Title.as_str())
            .fixed_width(120)
            .expand(true)
            .build();
        let artist = gtk4::ColumnViewColumn::builder()
            .title("Artist")
            .id(ColumnId::Artist.as_str())
            .fixed_width(120)
            .build();
        view.append_column(&cover);
        view.append_column(&title);
        view.append_column(&artist);
        let registry = ColumnRegistry::new(
            &view,
            Rc::new(crate::test_db::open().unwrap()),
            TableKeys {
                layout: "test.filler-transfer.layout",
                widths: "test.filler-transfer.widths",
            },
            vec![
                (ColumnId::Cover, cover.clone()),
                (ColumnId::Title, title.clone()),
                (ColumnId::Artist, artist.clone()),
            ],
        );
        registry.configure(
            Rc::new(|key| key.as_str().to_owned()),
            Rc::new(|_| 120),
            ColumnId::Title,
        );
        registry.apply(&registry.layout());
        let window = gtk4::Window::builder()
            .default_width(600)
            .default_height(160)
            .child(&view)
            .build();
        window.present();
        crate::ui::source_context_surface::settle_layout();
        let before = crate::ui::table_column_widths::realised_widths(&view);
        let artist_index = registry
            .current_order()
            .iter()
            .position(|key| *key == ColumnId::Artist)
            .unwrap();
        let artist_before = before[artist_index];

        EditorModel::set_visible(registry.as_ref(), ColumnId::Title.as_str(), false);
        crate::ui::source_context_surface::settle_layout();

        let visible_free = [ColumnId::Title, ColumnId::Artist]
            .into_iter()
            .filter(|key| registry.is_visible(*key))
            .count();
        assert_eq!(visible_free, 1, "only Artist remains visible and free");
        assert!(
            registry.is_visible(ColumnId::Cover),
            "the pinned leading Cover remains visible"
        );
        let after = crate::ui::table_column_widths::realised_widths(&view);
        let artist_after = after[artist_index];
        assert!(
            artist_after > artist_before,
            "Artist must absorb the space released by Title: before={before:?}, after={after:?}"
        );
        assert!(artist.expands());
        assert!(!title.expands());
        window.close();
    }
}
