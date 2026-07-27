//! Small, non-windowed model for the bounded release history.

#![allow(dead_code)]

use std::cell::RefCell;

use gtk4::{gio, glib};
use reprise_core::artist_news_history::HistoryEntry;

mod imp {
    use glib::subclass::prelude::*;

    use super::*;

    #[derive(Default)]
    pub struct ReleaseObject {
        pub entry: RefCell<Option<HistoryEntry>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ReleaseObject {
        const NAME: &'static str = "RepriseReleaseObject";
        type Type = super::ReleaseObject;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for ReleaseObject {}
}

glib::wrapper! {
    pub struct ReleaseObject(ObjectSubclass<imp::ReleaseObject>);
}

impl ReleaseObject {
    pub(super) fn new(entry: HistoryEntry) -> Self {
        use glib::subclass::prelude::ObjectSubclassIsExt;

        let object: Self = glib::Object::new();
        object.imp().entry.replace(Some(entry));
        object
    }

    pub(super) fn entry(&self) -> HistoryEntry {
        use glib::subclass::prelude::ObjectSubclassIsExt;

        self.imp()
            .entry
            .borrow()
            .as_ref()
            .expect("ReleaseObject is initialized before publication")
            .clone()
    }
}

pub(super) struct ReleasesModel {
    store: gio::ListStore,
    selection: gtk4::SingleSelection,
}

impl ReleasesModel {
    pub(super) fn new() -> Self {
        let store = gio::ListStore::new::<ReleaseObject>();
        let selection = gtk4::SingleSelection::new(Some(store.clone()));
        Self { store, selection }
    }

    pub(super) fn replace(&self, rows: Vec<HistoryEntry>) {
        self.store.remove_all();
        for row in rows {
            self.store.append(&ReleaseObject::new(row));
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
    use reprise_core::artist_news::LibraryPresence;

    use super::*;

    fn entry(id: &str) -> HistoryEntry {
        HistoryEntry {
            release_group_mbid: id.to_string(),
            artist_name: "Artist".to_string(),
            title: format!("Release {id}"),
            release_type: "Album".to_string(),
            first_release_date: "2026-01-01".to_string(),
            first_seen: Some(1),
            seen_at: None,
            hidden: false,
            hidden_at: None,
            presence: LibraryPresence::Absent,
            announce_url: None,
            track_count: None,
            local_track_count: 0,
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn release_model_replaces_the_bounded_history_snapshot() {
        gtk4::init().unwrap();
        let model = ReleasesModel::new();
        model.replace(vec![entry("one"), entry("two")]);
        assert_eq!(model.store().n_items(), 2);
        let first = model
            .store()
            .item(0)
            .unwrap()
            .downcast::<ReleaseObject>()
            .unwrap();
        assert_eq!(first.entry().release_group_mbid, "one");

        model.replace(vec![entry("three")]);
        assert_eq!(model.store().n_items(), 1);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn release_model_uses_single_selection_for_one_primary_action() {
        gtk4::init().unwrap();
        let model = ReleasesModel::new();
        assert_eq!(model.selection().model().unwrap(), model.store().clone());
    }
}
