//! Small, non-windowed model for the bounded release history.

#![allow(dead_code)]

use std::cell::RefCell;

use gtk4::prelude::*;
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
    selection: gtk4::MultiSelection,
}

impl ReleasesModel {
    pub(super) fn new() -> Self {
        let store = gio::ListStore::new::<ReleaseObject>();
        let selection = glib::Object::builder::<gtk4::MultiSelection>()
            .property("model", &store)
            .build();
        Self { store, selection }
    }

    pub(super) fn replace(&self, rows: Vec<HistoryEntry>) {
        let selected_mbids = self.selected_mbids();
        crate::ui::list_store_delta::replace(
            &self.store,
            rows,
            |object| {
                object
                    .clone()
                    .downcast::<ReleaseObject>()
                    .expect("releases store contains only release objects")
                    .entry()
            },
            |entry| entry.release_group_mbid.clone(),
            |entry| ReleaseObject::new(entry).upcast(),
        );
        self.select_mbids(&selected_mbids);
    }

    pub(super) fn store(&self) -> &gio::ListStore {
        &self.store
    }

    pub(super) fn selection(&self) -> &gtk4::MultiSelection {
        &self.selection
    }

    pub(super) fn position_of(&self, mbid: &str) -> Option<u32> {
        (0..self.store.n_items()).find(|position| {
            self.store
                .item(*position)
                .and_downcast::<ReleaseObject>()
                .is_some_and(|object| object.entry().release_group_mbid == mbid)
        })
    }

    pub(super) fn selected_mbids(&self) -> Vec<String> {
        (0..self.store.n_items())
            .filter(|position| self.selection.is_selected(*position))
            .filter_map(|position| self.store.item(position).and_downcast::<ReleaseObject>())
            .map(|object| object.entry().release_group_mbid)
            .collect()
    }

    pub(super) fn select_mbids(&self, mbids: &[String]) {
        let selected = self.selection.selection().copy();
        selected.remove_all();
        for mbid in mbids {
            if let Some(position) = self.position_of(mbid) {
                selected.add(position);
            }
        }
        let mask = self.selection.selection().copy();
        mask.remove_all();
        mask.add_range(0, self.store.n_items());
        self.selection.set_selection(&selected, &mask);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

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
    fn release_model_replaces_the_bounded_history_snapshot() {
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
    fn release_model_binds_multi_selection_to_the_release_store() {
        let model = ReleasesModel::new();
        assert_eq!(model.selection().model().unwrap(), model.store().clone());
    }

    #[test]
    fn replacing_rows_restores_exactly_the_selected_mbids_that_survive() {
        let model = ReleasesModel::new();
        model.replace(vec![entry("one"), entry("two"), entry("three")]);
        model.select_mbids(&["one".to_owned(), "two".to_owned(), "three".to_owned()]);

        model.replace(vec![entry("three"), entry("one"), entry("four")]);

        assert_eq!(model.selected_mbids(), ["three", "one"]);
    }

    #[test]
    fn restoring_multiple_mbids_never_emits_a_single_row_selection() {
        let model = ReleasesModel::new();
        model.replace(vec![entry("one"), entry("two"), entry("three")]);
        let observed_sizes = Rc::new(RefCell::new(Vec::new()));
        {
            let observed_sizes = observed_sizes.clone();
            model
                .selection()
                .connect_selection_changed(move |selection, _, _| {
                    observed_sizes
                        .borrow_mut()
                        .push(selection.selection().size());
                });
        }

        model.select_mbids(&["one".to_owned(), "two".to_owned(), "three".to_owned()]);

        assert_eq!(
            observed_sizes.borrow().as_slice(),
            [3],
            "restore must emit the complete selection once"
        );
    }

    #[test]
    fn one_release_change_emits_one_delta_and_an_identical_refresh_emits_nothing() {
        let model = ReleasesModel::new();
        let original = vec![entry("one"), entry("two"), entry("three")];
        model.replace(original.clone());
        model.select_mbids(&["three".to_owned()]);
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
        changed[0].title = "Updated release".into();
        model.replace(changed.clone());

        assert_eq!(changes.borrow().as_slice(), [(0, 1, 1)]);
        assert_eq!(model.selected_mbids(), ["three"]);
        assert_eq!(model.store().item(2).unwrap(), retained);

        model.replace(changed);
        assert_eq!(changes.borrow().as_slice(), [(0, 1, 1)]);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn release_delta_keeps_selection_viewport_and_zero_binds_for_a_noop() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let model = ReleasesModel::new();
        let rows = (0..100)
            .map(|position| entry(&format!("release-{position}")))
            .collect::<Vec<_>>();
        model.replace(rows.clone());
        model.select_mbids(&["release-75".into()]);
        let mut changed = rows;
        changed[0].title = "Updated release".into();
        let identical = changed.clone();

        crate::ui::list_store_delta::display_test::assert_viewport_selection_and_noop_bind_count(
            model.selection().clone().upcast(),
            || model.replace(changed),
            || model.replace(identical),
            || model.selected_mbids() == ["release-75"],
        );
    }
}
