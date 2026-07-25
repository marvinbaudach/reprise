//! FIL filter-reset actions kept outside the main TrackList orchestrator.

use reprise_core::queries::BrowseFilter;

use super::surface::TrackList;
use super::track_list_reload::set_filter_and_reload;

impl TrackList {
    /// FIL-1a/FIL-6: one action resets search and browse facets in a single
    /// reload. The caller additionally clears the headerbar entry text; the
    /// debounced search handler then early-returns because the filter is empty.
    pub fn clear_all_restrictions(&self) {
        let empty = BrowseFilter::default();
        *self.shared.browse_filter.borrow_mut() = empty.clone();
        self.shared.browse_bar.restore_filter(&empty);
        // FIL-7: Clear all also drops the AI-exclude filter (one reload below).
        self.shared.browse_bar.clear_exclude_ai();
        set_filter_and_reload(&self.shared, "");
    }

    pub fn set_on_search_cleared(&self, callback: impl Fn() + 'static) {
        self.shared.browse_bar.set_on_search_cleared(callback);
    }

    pub fn set_on_clear_all(&self, callback: impl Fn() + 'static) {
        self.shared.browse_bar.set_on_clear_all(callback);
    }

    pub fn set_on_scope_cleared(&self, callback: impl Fn() + 'static) {
        self.shared.browse_bar.set_on_scope_cleared(callback);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gtk4::gio::prelude::*;
    use gtk4::glib;
    use rusqlite::Connection;

    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn clear_all_restrictions_resets_search_and_browse_in_one_pass() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (path,title,artist,album,genre,added_at) VALUES
               ('/a.flac','Falling Apart','Caskets','X','Metal',0),
               ('/b.flac','Other','Dead by April','Y','Rock',0);",
        )
        .unwrap();
        let track_list = TrackList::new(
            Rc::new(RefCell::new(conn)),
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            super::super::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        );
        track_list.set_filter("falling");
        *track_list.shared.browse_filter.borrow_mut() = BrowseFilter {
            genre: Some("Metal".into()),
            ..BrowseFilter::default()
        };
        track_list.reload();

        track_list.clear_all_restrictions();
        let context = glib::MainContext::default();
        while context.pending() {
            context.iteration(false);
        }
        assert_eq!(track_list.shared.filter.borrow().as_str(), "");
        assert_eq!(
            *track_list.shared.browse_filter.borrow(),
            BrowseFilter::default()
        );
        assert_eq!(track_list.shared.browse_bar.result_count(), Some((2, 2)));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_1c_clear_all_keeps_the_genre_scope_and_counts_against_the_library() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (path,title,artist,album,genre,year,added_at) VALUES
               ('/a.flac','A','X','One','Metalcore',2026,0),
               ('/b.flac','B','Y','Two','Metalcore',2025,0),
               ('/c.flac','C','Z','Three','Jazz',2026,0);",
        )
        .unwrap();
        let track_list = TrackList::new(
            Rc::new(RefCell::new(conn)),
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            super::super::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        );
        assert!(
            track_list.restore_browser_place(&reprise_core::browser::BrowserPlace::from(
                reprise_core::view_source::ViewSource::Genre("Metalcore".into())
            ))
        );
        *track_list.shared.browse_filter.borrow_mut() = BrowseFilter {
            year: Some("2026".into()),
            ..BrowseFilter::default()
        };
        track_list.reload();

        track_list.clear_all_restrictions();
        let context = glib::MainContext::default();
        while context.pending() {
            context.iteration(false);
        }

        assert_eq!(
            track_list.current_source(),
            reprise_core::view_source::ViewSource::Genre("Metalcore".into())
        );
        assert_eq!(track_list.shared.model.n_items(), 2);
        assert_eq!(track_list.shared.browse_bar.result_count(), Some((2, 3)));
    }

    // UX FIL-7: with the experimental switch on, the "Hide AI music" filter
    // hides AI-flagged tracks and the filter row counts "X of Y" (FIL-2).
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_7_hide_ai_music_filter_hides_ai_tracks_and_counts() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (id,path,title,artist,album,added_at) VALUES
               (1,'/a.flac','A','X','Al',0),(2,'/b.flac','B','X','Al',0),
               (3,'/c.flac','C','X','Al',0),
               (4,'/d.flac','D (Instrumental)','X','Al',0),
               (5,'/e.flac','E (Instrumental)','X','Al',0);",
        )
        .unwrap();
        for id in [4, 5] {
            reprise_core::provenance::insert_provenance(
                &conn,
                id,
                &reprise_core::provenance::ProvenanceInput {
                    kind: reprise_core::provenance::KIND_VOCALS_REMOVED.to_string(),
                    ai: true,
                    source_track_id: None,
                    source_text: None,
                    source_mbid: None,
                    model: Some("m@1".into()),
                },
                0,
            )
            .unwrap();
        }
        // The AI-exclude filter only applies when the experimental switch is on.
        crate::ui::instrumental::set_experimental_enabled(&conn, true).unwrap();

        let track_list = TrackList::new(
            Rc::new(RefCell::new(conn)),
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            super::super::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        );
        let context = glib::MainContext::default();
        track_list.reload();
        while context.pending() {
            context.iteration(false);
        }
        assert_eq!(
            track_list.shared.model.n_items(),
            5,
            "AI tracks are visible by default (opt-in filter)"
        );

        track_list.shared.browse_bar.set_exclude_ai(true);
        track_list.reload();
        while context.pending() {
            context.iteration(false);
        }
        assert_eq!(
            track_list.shared.model.n_items(),
            3,
            "the two AI-flagged tracks are hidden"
        );
        assert_eq!(
            track_list.shared.browse_bar.result_count(),
            Some((3, 5)),
            "FIL-2: 3 of 5 tracks"
        );
    }
}
