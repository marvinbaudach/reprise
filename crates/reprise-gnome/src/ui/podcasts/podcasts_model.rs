use std::cell::RefCell;

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
        self.store.remove_all();
        for row in rows {
            self.store.append(&PodcastEpisodeObject::new(row));
        }
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
    use super::*;
    use gtk4::gio::prelude::*;
    use reprise_core::podcasts::PodcastKind;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn model_replaces_the_complete_episode_set() {
        gtk4::init().unwrap();
        let row = EpisodeRow {
            id: 1,
            subscription_id: 1,
            guid: "g".into(),
            title: "Episode".into(),
            show: "Show".into(),
            show_image_url: None,
            kind: PodcastKind::Rss,
            audio_url: "https://example.test/e.mp3".into(),
            page_url: None,
            published_at: None,
            duration_secs: None,
            downloaded_path: None,
            played_at: None,
            position_ms: 0,
            first_seen_at: 1,
        };
        let model = PodcastsModel::new();
        model.replace(vec![row]);
        assert_eq!(model.store().n_items(), 1);
    }
}
