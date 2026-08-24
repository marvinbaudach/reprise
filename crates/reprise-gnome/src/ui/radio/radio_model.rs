use std::cell::RefCell;

use gtk4::prelude::*;
use gtk4::{gio, glib};
use reprise_core::radio::StationRow;

mod imp {
    use glib::subclass::prelude::*;

    use super::*;

    #[derive(Default)]
    pub struct RadioObject {
        pub row: RefCell<Option<StationRow>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RadioObject {
        const NAME: &'static str = "RepriseRadioObject";
        type Type = super::RadioObject;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for RadioObject {}
}

glib::wrapper! {
    pub struct RadioObject(ObjectSubclass<imp::RadioObject>);
}

impl RadioObject {
    pub(super) fn new(row: StationRow) -> Self {
        use glib::subclass::prelude::ObjectSubclassIsExt;

        let object: Self = glib::Object::new();
        object.imp().row.replace(Some(row));
        object
    }

    pub(super) fn row(&self) -> StationRow {
        use glib::subclass::prelude::ObjectSubclassIsExt;

        self.imp()
            .row
            .borrow()
            .as_ref()
            .expect("RadioObject is initialized before publication")
            .clone()
    }
}

pub(super) struct RadioModel {
    store: gio::ListStore,
    selection: gtk4::SingleSelection,
}

impl RadioModel {
    pub(super) fn new() -> Self {
        let store = gio::ListStore::new::<RadioObject>();
        let selection = gtk4::SingleSelection::new(Some(store.clone()));
        Self { store, selection }
    }

    /// Publishes `rows`, but only if they differ from what the store already
    /// holds.
    ///
    /// The view re-renders on every external playback snapshot — the play
    /// itself, each phase change, and every new ICY stream title. The station
    /// list is identical across all of those, yet the old unconditional
    /// `remove_all()` + append made `GtkSingleSelection` autoselect row 0 (it
    /// saw the selected item removed) and reset the scroll offset while the
    /// store stood empty. Double-clicking a station moved the highlight to the
    /// station above it, and every song change did it again. The live parts of
    /// a row reach their cells through [`super::radio_live_cells`] instead, so
    /// no model signal is needed to move the playing marker.
    pub(super) fn replace(&self, rows: Vec<StationRow>) {
        if self.rows() == rows {
            return;
        }
        crate::ui::list_store_delta::replace(
            &self.store,
            rows,
            |object| {
                object
                    .clone()
                    .downcast::<RadioObject>()
                    .expect("radio store contains only station objects")
                    .row()
            },
            Clone::clone,
            |row| RadioObject::new(row).upcast(),
        );
    }

    /// The rows the store currently holds, in order.
    pub(super) fn rows(&self) -> Vec<StationRow> {
        use gtk4::prelude::{Cast, ListModelExt};

        (0..self.store.n_items())
            .filter_map(|position| {
                self.store
                    .item(position)
                    .and_then(|object| object.downcast::<RadioObject>().ok())
                    .map(|object| object.row())
            })
            .collect()
    }

    pub(super) fn store(&self) -> &gio::ListStore {
        &self.store
    }

    pub(super) fn selection(&self) -> &gtk4::SingleSelection {
        &self.selection
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gtk4::prelude::*;

    use super::*;

    fn row(id: i64) -> StationRow {
        StationRow {
            id,
            uuid: None,
            name: format!("Station {id}"),
            stream_url: format!("https://radio.example/{id}"),
            homepage: None,
            favicon_url: None,
            genre: None,
            codec: None,
            bitrate_kbps: None,
            country_code: None,
            votes: None,
            added_at: 1,
            removed_at: None,
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn radio_model_replaces_the_complete_small_favorites_set() {
        gtk4::init().unwrap();
        let model = RadioModel::new();
        model.replace(vec![row(1), row(2)]);
        assert_eq!(model.store().n_items(), 2);
        model.replace(vec![row(3)]);
        assert_eq!(model.store().n_items(), 1);
        assert_eq!(
            model
                .store()
                .item(0)
                .unwrap()
                .downcast::<RadioObject>()
                .unwrap()
                .row()
                .id,
            3
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn one_station_change_emits_one_delta_and_an_identical_refresh_emits_nothing() {
        gtk4::init().unwrap();
        let model = RadioModel::new();
        let original = vec![row(1), row(2), row(3)];
        model.replace(original.clone());
        model.selection().set_selected(2);
        let retained = model.store().item(2).unwrap();
        let changes = Rc::new(RefCell::new(Vec::new()));
        {
            let changes = changes.clone();
            model
                .store()
                .connect_items_changed(move |_, at, removed, added| {
                    changes.borrow_mut().push((at, removed, added));
                });
        }

        let mut changed = original.clone();
        changed[0].name = "Updated station".into();
        model.replace(changed.clone());

        assert_eq!(changes.borrow().as_slice(), [(0, 1, 1)]);
        assert_eq!(model.selection().selected(), 2);
        assert_eq!(model.store().item(2).unwrap(), retained);

        model.replace(changed);
        assert_eq!(changes.borrow().as_slice(), [(0, 1, 1)]);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn radio_delta_keeps_selection_viewport_and_zero_binds_for_a_noop() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let model = RadioModel::new();
        let rows = (0..100).map(row).collect::<Vec<_>>();
        model.replace(rows.clone());
        model.selection().set_selected(75);
        let mut changed = rows;
        changed[0].name = "Updated station".into();
        let identical = changed.clone();

        crate::ui::list_store_delta::display_test::assert_viewport_selection_and_noop_bind_count(
            model.selection().clone().upcast(),
            || model.replace(changed),
            || model.replace(identical),
            || model.selection().selected() == 75,
        );
    }
}
