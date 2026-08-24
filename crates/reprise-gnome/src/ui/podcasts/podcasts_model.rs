use std::cell::RefCell;

use gtk4::prelude::*;
use gtk4::{gio, glib};
use reprise_core::podcasts::EpisodeRow;

mod imp {
    use super::*;
    use glib::subclass::prelude::*;

    #[derive(Default)]
    pub struct PodcastEpisodeObject {
        pub row: RefCell<Option<EpisodeRow>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PodcastEpisodeObject {
        const NAME: &'static str = "ReprisePodcastEpisodeObject";
        type Type = super::PodcastEpisodeObject;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for PodcastEpisodeObject {}
}

glib::wrapper! {
    pub struct PodcastEpisodeObject(ObjectSubclass<imp::PodcastEpisodeObject>);
}

impl PodcastEpisodeObject {
    pub(super) fn new(row: EpisodeRow) -> Self {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        let object: Self = glib::Object::new();
        object.imp().row.replace(Some(row));
        object
    }

    pub(super) fn row(&self) -> EpisodeRow {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        self.imp()
            .row
            .borrow()
            .as_ref()
            .expect("podcast episode object is initialized before publication")
            .clone()
    }
}

pub(super) struct PodcastsModel {
    store: gio::ListStore,
    selection: gtk4::SingleSelection,
    sort: RefCell<Option<gtk4::SortListModel>>,
}

impl PodcastsModel {
    pub(super) fn new() -> Self {
        let store = gio::ListStore::new::<PodcastEpisodeObject>();
        let selection = gtk4::SingleSelection::new(Some(store.clone()));
        Self {
            store,
            selection,
            sort: RefCell::new(None),
        }
    }

    pub(super) fn replace(&self, rows: Vec<EpisodeRow>) {
        crate::ui::list_store_delta::replace(
            &self.store,
            rows,
            |object| {
                object
                    .clone()
                    .downcast::<PodcastEpisodeObject>()
                    .expect("podcasts store contains only episode objects")
                    .row()
            },
            |row| row.id,
            |row| PodcastEpisodeObject::new(row).upcast(),
        );
    }

    pub(super) fn store(&self) -> &gio::ListStore {
        &self.store
    }

    pub(super) fn selection(&self) -> &gtk4::SingleSelection {
        &self.selection
    }

    pub(super) fn enable_sorting(&self, sorter: Option<gtk4::Sorter>) {
        let sort = gtk4::SortListModel::new(Some(self.store.clone()), sorter);
        self.selection.set_model(Some(&sort));
        self.sort.replace(Some(sort));
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use reprise_core::podcasts::PodcastKind;

    fn row(id: i64) -> EpisodeRow {
        EpisodeRow {
            id,
            subscription_id: 1,
            guid: format!("guid-{id}"),
            title: format!("Episode {id}"),
            show: "Show".into(),
            show_image_url: None,
            image_url: None,
            kind: PodcastKind::Rss,
            audio_url: format!("https://example.test/{id}.mp3"),
            page_url: None,
            published_at: None,
            duration_secs: None,
            downloaded_path: None,
            downloaded_bytes: None,
            played_at: None,
            position_ms: 0,
            first_seen_at: 1,
            is_new: false,
            media_category: None,
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn model_replaces_the_complete_episode_set() {
        gtk4::init().unwrap();
        let model = PodcastsModel::new();
        model.replace(vec![row(1)]);
        assert_eq!(model.store().n_items(), 1);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn one_episode_change_emits_one_delta_and_an_identical_refresh_emits_nothing() {
        gtk4::init().unwrap();
        let model = PodcastsModel::new();
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
        changed[0].title = "Updated episode".into();
        model.replace(changed.clone());

        assert_eq!(changes.borrow().as_slice(), [(0, 1, 1)]);
        assert_eq!(model.selection().selected(), 2);
        assert_eq!(model.store().item(2).unwrap(), retained);

        model.replace(changed);
        assert_eq!(changes.borrow().as_slice(), [(0, 1, 1)]);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn podcast_delta_keeps_selection_viewport_and_zero_binds_for_a_noop() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let model = PodcastsModel::new();
        let rows = (0..100).map(row).collect::<Vec<_>>();
        model.replace(rows.clone());
        model.selection().set_selected(75);
        let mut changed = rows;
        changed[0].title = "Updated episode".into();
        let identical = changed.clone();

        crate::ui::list_store_delta::display_test::assert_viewport_selection_and_noop_bind_count(
            model.selection().clone().upcast(),
            || model.replace(changed),
            || model.replace(identical),
            || model.selection().selected() == 75,
        );
    }
}
