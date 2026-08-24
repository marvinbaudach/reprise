#![allow(dead_code)]

use std::cell::RefCell;

use gtk4::prelude::*;
use gtk4::{gio, glib};
use reprise_core::concerts::ConcertRow;

mod imp {
    use glib::subclass::prelude::*;

    use super::*;

    #[derive(Default)]
    pub struct ConcertObject {
        pub row: RefCell<Option<ConcertRow>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ConcertObject {
        const NAME: &'static str = "RepriseConcertObject";
        type Type = super::ConcertObject;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for ConcertObject {}
}

glib::wrapper! {
    pub struct ConcertObject(ObjectSubclass<imp::ConcertObject>);
}

impl ConcertObject {
    pub(super) fn new(row: ConcertRow) -> Self {
        use glib::subclass::prelude::ObjectSubclassIsExt;

        let object: Self = glib::Object::new();
        object.imp().row.replace(Some(row));
        object
    }

    pub(super) fn row(&self) -> ConcertRow {
        use glib::subclass::prelude::ObjectSubclassIsExt;

        self.imp()
            .row
            .borrow()
            .as_ref()
            .expect("ConcertObject is initialized before publication")
            .clone()
    }
}

pub(super) struct ConcertsModel {
    store: gio::ListStore,
    selection: gtk4::SingleSelection,
}

impl ConcertsModel {
    pub(super) fn new() -> Self {
        let store = gio::ListStore::new::<ConcertObject>();
        let selection = gtk4::SingleSelection::new(Some(store.clone()));
        Self { store, selection }
    }

    pub(super) fn replace(&self, rows: Vec<ConcertRow>) {
        crate::ui::list_store_delta::replace(
            &self.store,
            rows,
            |object| {
                object
                    .clone()
                    .downcast::<ConcertObject>()
                    .expect("concerts store contains only concert objects")
                    .row()
            },
            |row| row.id,
            |row| ConcertObject::new(row).upcast(),
        );
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

    fn row(id: i64) -> ConcertRow {
        ConcertRow {
            id,
            availability: reprise_core::concerts::TicketAvailability::Unknown,
            date_key: "2099-01-01".into(),
            starts_at: "2099-01-01T19:00:00".into(),
            artist_name: format!("Artist {id}"),
            venue: "Venue".into(),
            city: "City".into(),
            region: None,
            country: None,
            latitude: None,
            longitude: None,
            distance_km: None,
            ticket_url: None,
            ticket_source: None,
            event_url: None,
            provider: "fixture".into(),
            is_similar: false,
            similar_to: None,
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn model_replaces_the_complete_small_result_set() {
        gtk4::init().unwrap();
        let model = ConcertsModel::new();
        model.replace(vec![row(1), row(2)]);
        assert_eq!(model.store().n_items(), 2);
        let first = model
            .store()
            .item(0)
            .unwrap()
            .downcast::<ConcertObject>()
            .unwrap();
        assert_eq!(first.row().id, 1);

        model.replace(vec![row(3)]);
        assert_eq!(model.store().n_items(), 1);
        assert_eq!(
            model
                .store()
                .item(0)
                .unwrap()
                .downcast::<ConcertObject>()
                .unwrap()
                .row()
                .id,
            3
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn model_uses_single_selection_for_a_single_row_primary_action() {
        gtk4::init().unwrap();
        let model = ConcertsModel::new();
        assert_eq!(model.selection().model().unwrap(), model.store().clone());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn one_concert_change_emits_one_delta_and_an_identical_refresh_emits_nothing() {
        gtk4::init().unwrap();
        let model = ConcertsModel::new();
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
        changed[0].venue = "Updated venue".into();
        model.replace(changed.clone());

        assert_eq!(changes.borrow().as_slice(), [(0, 1, 1)]);
        assert_eq!(model.selection().selected(), 2);
        assert_eq!(model.store().item(2).unwrap(), retained);

        model.replace(changed);
        assert_eq!(changes.borrow().as_slice(), [(0, 1, 1)]);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn concert_delta_keeps_selection_viewport_and_zero_binds_for_a_noop() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let model = ConcertsModel::new();
        let rows = (0..100).map(row).collect::<Vec<_>>();
        model.replace(rows.clone());
        model.selection().set_selected(75);
        let mut changed = rows;
        changed[0].venue = "Updated venue".into();
        let identical = changed.clone();

        crate::ui::list_store_delta::display_test::assert_viewport_selection_and_noop_bind_count(
            model.selection().clone().upcast(),
            || model.replace(changed),
            || model.replace(identical),
            || model.selection().selected() == 75,
        );
    }
}
