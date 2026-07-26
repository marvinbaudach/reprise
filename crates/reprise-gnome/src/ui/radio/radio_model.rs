use std::cell::RefCell;

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

    pub(super) fn replace(&self, rows: Vec<StationRow>) {
        self.store.remove_all();
        for row in rows {
            self.store.append(&RadioObject::new(row));
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
}
