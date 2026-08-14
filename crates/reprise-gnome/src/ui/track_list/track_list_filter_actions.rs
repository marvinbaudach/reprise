//! FIL filter-reset actions kept outside the main TrackList orchestrator.

use reprise_core::queries::BrowseFilter;

use super::surface::TrackList;
use super::track_list_reload::{reload_with_viewport, viewport_after_clearing};

impl TrackList {
    pub(in crate::ui) fn set_committed_search_query(&self, query: &str) {
        self.shared.browse_bar.set_committed_query(query);
    }

    /// FIL-1a/FIL-6: one action resets search and browse facets and performs
    /// the single model reload. The caller still applies the empty header-bar
    /// query so its commit half removes the chip; `set_filter_and_reload`
    /// recognizes that the filter is already empty and does no model work.
    pub fn clear_all_restrictions(&self) {
        let had_query = !self.shared.filter.borrow().is_empty();
        let started_in_search = self.shared.pre_search.get().playback_started;
        let empty = BrowseFilter::default();
        *self.shared.browse_filter.borrow_mut() = empty.clone();
        self.shared.browse_bar.restore_filter(&empty);
        // FIL-7: Clear all also drops the AI-exclude filter (one reload below).
        self.shared.browse_bar.clear_exclude_ai();
        *self.shared.filter.borrow_mut() = String::new();
        reload_with_viewport(
            &self.shared,
            viewport_after_clearing(had_query, started_in_search),
        );
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
    use std::cell::Cell;
    use std::rc::Rc;

    use gtk4::gio::prelude::*;
    use gtk4::glib;
    use gtk4::prelude::*;
    use reprise_view::search_scope::SearchScope;

    use super::*;

    fn contains_label(widget: &gtk4::Widget, needle: &str) -> bool {
        if widget
            .downcast_ref::<gtk4::Label>()
            .is_some_and(|label| label.label().contains(needle))
        {
            return true;
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            if contains_label(&current, needle) {
                return true;
            }
            child = current.next_sibling();
        }
        false
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_2a_clear_all_reloads_the_track_list_once() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let entry = gtk4::SearchEntry::new();
        entry.set_search_delay(0);
        let toggle = gtk4::ToggleButton::new();
        let popover = crate::ui::window::search_popover::SearchPopover::new(&toggle, &entry);
        let search =
            crate::ui::window::section_search::SectionSearch::new(&entry, &popover, &toggle);
        let reloads = Rc::new(Cell::new(0));
        let reloads_for_callback = reloads.clone();
        let track_list = Rc::new(TrackList::new(
            Rc::new(crate::test_db::open().unwrap()),
            Box::new(|_, _, _, _| {}),
            move |_, _, _, _| reloads_for_callback.set(reloads_for_callback.get() + 1),
            super::super::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        ));
        crate::ui::window::section_search_wiring::install_tracks(&search, &track_list);
        search.activate_source(&reprise_core::view_source::ViewSource::Library, "Music");

        track_list.set_filter("falling");
        track_list.set_committed_search_query("falling");
        entry.set_text("falling");
        let browse = BrowseFilter {
            genre: Some("Metal".into()),
            ..BrowseFilter::default()
        };
        *track_list.shared.browse_filter.borrow_mut() = browse.clone();
        track_list.shared.browse_bar.restore_filter(&browse);
        reloads.set(0);

        search.clear_all();

        assert_eq!(reloads.get(), 1, "Clear all must rebuild the model once");
        assert_eq!(track_list.shared.filter.borrow().as_str(), "");
        assert_eq!(
            *track_list.shared.browse_filter.borrow(),
            BrowseFilter::default()
        );
        assert_eq!(entry.text(), "");
        assert!(
            !contains_label(
                track_list.shared.browse_bar.widget().upcast_ref(),
                "falling"
            ),
            "the committed search chip must be gone"
        );

        // Missing files shares the same clear-facets handler, but its query
        // has a separate sink. Exercise that registration once so the Tracks
        // no-op cannot strand the Missing query in the header entry.
        search.activate_source(
            &reprise_core::view_source::ViewSource::Missing,
            "Missing files",
        );
        search.set_query(SearchScope::Missing, "missing");
        assert_eq!(entry.text(), "missing");
        search.clear_all();
        assert_eq!(entry.text(), "");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn clear_all_restrictions_resets_search_and_browse_in_one_pass() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = crate::test_db::open().unwrap();
        crate::test_db::connection(&conn)
            .execute_batch(
                "INSERT INTO tracks (path,title,artist,album,genre,added_at) VALUES
                   ('/a.flac','Falling Apart','Caskets','X','Metal',0),
                   ('/b.flac','Other','Dead by April','Y','Rock',0);",
            )
            .unwrap();
        let track_list = TrackList::new(
            Rc::new(conn),
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
    fn fil_1c_clear_all_keeps_the_genre_place_and_counts_against_it() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = crate::test_db::open().unwrap();
        crate::test_db::connection(&conn)
            .execute_batch(
                "INSERT INTO tracks (path,title,artist,album,genre,year,added_at) VALUES
                   ('/a.flac','A','X','One','Metalcore',2026,0),
                   ('/b.flac','B','Y','Two','Metalcore',2025,0),
                   ('/c.flac','C','Z','Three','Jazz',2026,0);",
            )
            .unwrap();
        let track_list = TrackList::new(
            Rc::new(conn),
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
        // FIL-2 (revised 2026-07-31): the place is the counting base, so the
        // genre reports its own two tracks rather than "2 of 3" against the
        // library — and with the year filter cleared nothing restricts at all.
        assert_eq!(track_list.shared.browse_bar.result_count(), Some((2, 2)));
    }

    // UX FIL-7: the "Hide AI music" filter hides AI-flagged tracks and the
    // filter row counts "X of Y" (FIL-2).
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_7_hide_ai_music_filter_hides_ai_tracks_and_counts() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = crate::test_db::open().unwrap();
        crate::test_db::connection(&conn)
            .execute_batch(
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
        let track_list = TrackList::new(
            Rc::new(conn),
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
            "FIL-2a: 3 of 5 tracks"
        );
    }
}
