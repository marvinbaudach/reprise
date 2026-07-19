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
        set_filter_and_reload(&self.shared, "");
    }

    pub fn set_on_search_cleared(&self, callback: impl Fn() + 'static) {
        self.shared.browse_bar.set_on_search_cleared(callback);
    }

    pub fn set_on_clear_all(&self, callback: impl Fn() + 'static) {
        self.shared.browse_bar.set_on_clear_all(callback);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

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
}
