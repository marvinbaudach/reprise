//! Connects each list view to [`SectionSearch`].
//!
//! Split out of `window_runtime_wiring` so neither file grows past the
//! repository's source-size limit, and so the whole per-section contract —
//! who applies a query, who clears its own facets only for explicit Clear all,
//! and who pushes a cleared chip back into the entry — is readable in one
//! place. SEARCH-8a view switches use only the query half of each registration,
//! so type/window/hidden/unplayed/downloaded facets survive untouched.

use std::rc::Rc;

use reprise_view::search_scope::SearchScope;

use super::section_search::SectionSearch;
use super::track_list::TrackList;

pub(in crate::ui) struct SectionSearchViews<'a> {
    pub(in crate::ui) track_list: &'a Rc<TrackList>,
    pub(in crate::ui) podcasts_view: &'a Rc<crate::ui::podcasts::PodcastsView>,
    pub(in crate::ui) youtube_view: &'a Rc<crate::ui::podcasts::PodcastsView>,
    pub(in crate::ui) radio_view: &'a Rc<crate::ui::radio::RadioView>,
    pub(in crate::ui) releases_view: &'a Rc<crate::ui::releases::ReleasesView>,
    pub(in crate::ui) concerts_view: &'a Rc<crate::ui::concerts::ConcertsView>,
    pub(in crate::ui) library_doctor: &'a Rc<crate::ui::library_doctor::LibraryDoctorLauncher>,
}

pub(in crate::ui) fn install(search: &Rc<SectionSearch>, views: &SectionSearchViews<'_>) {
    install_tracks(search, views.track_list);
    install_podcasts(search, SearchScope::Podcasts, views.podcasts_view);
    install_podcasts(search, SearchScope::Youtube, views.youtube_view);
    install_radio(search, views.radio_view);
    install_releases(search, views.releases_view);
    install_concerts(search, views.concerts_view);
    install_library_doctor(search, views.library_doctor);
}

fn install_library_doctor(
    search: &Rc<SectionSearch>,
    launcher: &Rc<crate::ui::library_doctor::LibraryDoctorLauncher>,
) {
    let weak_search = Rc::downgrade(search);
    launcher.set_on_search_query_changed(move |query| {
        if let Some(search) = weak_search.upgrade() {
            search.set_query(SearchScope::DoctorReview, query);
        }
    });
    let applying = Rc::downgrade(launcher);
    let committing = Rc::downgrade(launcher);
    let clearing = Rc::downgrade(launcher);
    search.register(
        SearchScope::DoctorReview,
        move |query| {
            if let Some(launcher) = applying.upgrade() {
                launcher.set_search_query(query);
            }
        },
        move |query| {
            if let Some(launcher) = committing.upgrade() {
                launcher.set_committed_search_query(query);
            }
        },
        move || {
            if let Some(launcher) = clearing.upgrade() {
                launcher.clear_all_filters();
            }
        },
    );
}

/// Music and Missing files share the track list. Non-empty Music queries use
/// the track list's own debounced path (`view_session::wire_search`) so typing
/// does not run two reloads. The scope handler still owns empty queries: view
/// switches call it directly after changing the active scope, when the entry's
/// signal is deliberately no longer allowed to reach the track list.
///
/// Missing files is the exception: its rows are rendered by
/// `MissingFilesView`, not by the table, and that view has to be told the
/// query separately (FIL-1d — it matches file paths, which the table's
/// "any field" search does not).
pub(in crate::ui) fn install_tracks(search: &Rc<SectionSearch>, track_list: &Rc<TrackList>) {
    let applying = Rc::downgrade(track_list);
    let committing = Rc::downgrade(track_list);
    let clearing = Rc::downgrade(track_list);
    search.register(
        SearchScope::Tracks,
        move |query| {
            if !query.is_empty() {
                return;
            }
            if let Some(track_list) = applying.upgrade() {
                track_list.set_filter("");
            }
        },
        move |query| {
            if let Some(track_list) = committing.upgrade() {
                track_list.set_committed_search_query(query);
            }
        },
        move || {
            if let Some(track_list) = clearing.upgrade() {
                track_list.clear_all_restrictions();
            }
        },
    );
    let applying = Rc::downgrade(track_list);
    let committing = Rc::downgrade(track_list);
    let clearing = Rc::downgrade(track_list);
    search.register(
        SearchScope::Missing,
        move |query| {
            if let Some(track_list) = applying.upgrade() {
                track_list.set_missing_search_query(query);
            }
        },
        move |query| {
            if let Some(track_list) = committing.upgrade() {
                track_list.set_committed_search_query(query);
            }
        },
        move || {
            if let Some(track_list) = clearing.upgrade() {
                track_list.set_missing_search_query("");
                track_list.clear_all_restrictions();
            }
        },
    );
}

fn install_podcasts(
    search: &Rc<SectionSearch>,
    scope: SearchScope,
    view: &Rc<crate::ui::podcasts::PodcastsView>,
) {
    {
        let search = Rc::downgrade(search);
        view.set_on_search_query_changed(move |query| {
            if let Some(search) = search.upgrade() {
                search.set_query(scope, query);
            }
        });
    }
    let apply_view = Rc::downgrade(view);
    let commit_view = Rc::downgrade(view);
    let clear_view = Rc::downgrade(view);
    search.register(
        scope,
        move |query| {
            if let Some(view) = apply_view.upgrade() {
                view.set_search_query(query);
            }
        },
        move |query| {
            if let Some(view) = commit_view.upgrade() {
                view.set_committed_search_query(query);
            }
        },
        move || {
            if let Some(view) = clear_view.upgrade() {
                view.clear_all_filters();
            }
        },
    );
}

fn install_radio(search: &Rc<SectionSearch>, view: &Rc<crate::ui::radio::RadioView>) {
    {
        let search = Rc::downgrade(search);
        view.set_on_search_query_changed(move |query| {
            if let Some(search) = search.upgrade() {
                search.set_query(SearchScope::Radio, query);
            }
        });
    }
    let apply_view = Rc::downgrade(view);
    let commit_view = Rc::downgrade(view);
    let clear_view = Rc::downgrade(view);
    search.register(
        SearchScope::Radio,
        move |query| {
            if let Some(view) = apply_view.upgrade() {
                view.set_search_query(query);
            }
        },
        move |query| {
            if let Some(view) = commit_view.upgrade() {
                view.set_committed_search_query(query);
            }
        },
        move || {
            if let Some(view) = clear_view.upgrade() {
                view.clear_all_filters();
            }
        },
    );
}

fn install_releases(search: &Rc<SectionSearch>, view: &Rc<crate::ui::releases::ReleasesView>) {
    {
        let search = Rc::downgrade(search);
        view.set_on_search_query_changed(move |query| {
            if let Some(search) = search.upgrade() {
                search.set_query(SearchScope::Releases, query);
            }
        });
    }
    let apply_view = Rc::downgrade(view);
    let commit_view = Rc::downgrade(view);
    let clear_view = Rc::downgrade(view);
    search.register(
        SearchScope::Releases,
        move |query| {
            if let Some(view) = apply_view.upgrade() {
                view.set_search_query(query);
            }
        },
        move |query| {
            if let Some(view) = commit_view.upgrade() {
                view.set_committed_search_query(query);
            }
        },
        move || {
            if let Some(view) = clear_view.upgrade() {
                view.clear_all_filters();
            }
        },
    );
}

fn install_concerts(search: &Rc<SectionSearch>, view: &Rc<crate::ui::concerts::ConcertsView>) {
    {
        let search = Rc::downgrade(search);
        view.set_on_search_query_changed(move |query| {
            if let Some(search) = search.upgrade() {
                search.set_query(SearchScope::Concerts, query);
            }
        });
    }
    let apply_view = Rc::downgrade(view);
    let commit_view = Rc::downgrade(view);
    let clear_view = Rc::downgrade(view);
    search.register(
        SearchScope::Concerts,
        move |query| {
            if let Some(view) = apply_view.upgrade() {
                view.set_search_query(query);
            }
        },
        move |query| {
            if let Some(view) = commit_view.upgrade() {
                view.set_committed_search_query(query);
            }
        },
        move || {
            if let Some(view) = clear_view.upgrade() {
                view.clear_all_filters();
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;
    use crate::ui::track_list::queue_sections::QueueViewModel;
    use gtk4::prelude::*;
    use reprise_core::view_source::ViewSource;

    fn settle() {
        while gtk4::glib::MainContext::default().iteration(false) {}
    }

    fn settle_until(condition: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !condition() {
            settle();
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for search state"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    // UX SEARCH-8a: leaving Music clears both representations of its query.
    // The header entry and the track list's own filter must never diverge.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_8a_leaving_and_reentering_music_clears_the_track_filter() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let entry = gtk4::SearchEntry::new();
        entry.set_search_delay(0);
        let toggle = gtk4::ToggleButton::new();
        let popover = super::super::search_popover::SearchPopover::new(&toggle, &entry);
        let search = SectionSearch::new(&entry, &popover, &toggle);
        let track_list = Rc::new(TrackList::new(
            Rc::new(crate::test_db::open().unwrap()),
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        ));
        install_tracks(&search, &track_list);
        search.register(SearchScope::Podcasts, |_| {}, |_| {}, || {});
        let guard = crate::ui::view_session::new_search_restore_guard();
        let search_for_scope = search.clone();
        crate::ui::view_session::wire_search(
            &entry,
            track_list.clone(),
            guard,
            Rc::new(move || search_for_scope.is_active(SearchScope::Tracks)),
        );

        search.activate_source(&ViewSource::Library, "Music");
        entry.set_text("falling");
        settle_until(|| track_list.shared.filter.borrow().as_str() == "falling");
        assert_eq!(track_list.shared.filter.borrow().as_str(), "falling");

        search.activate_source(&ViewSource::Podcasts, "Podcasts");
        search.activate_source(&ViewSource::Library, "Music");
        settle();

        assert_eq!(entry.text(), "");
        assert_eq!(
            track_list.shared.filter.borrow().as_str(),
            "",
            "the track list must not retain a query the header discarded"
        );
    }
}
