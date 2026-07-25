#![allow(dead_code)]

use std::cell::RefCell;

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
        self.store.remove_all();
        for row in rows {
            self.store.append(&ConcertObject::new(row));
        }
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
    use gtk4::gio::prelude::*;

    use super::*;

    fn row(id: i64) -> ConcertRow {
        ConcertRow {
            id,
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
}
