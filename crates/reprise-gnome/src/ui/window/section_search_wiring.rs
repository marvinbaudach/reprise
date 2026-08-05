//! Connects each list view to [`SectionSearch`].
//!
//! Split out of `window_runtime_wiring` so neither file grows past the
//! repository's source-size limit, and so the whole per-section contract —
//! who applies a query, who clears its own facets, who pushes a cleared chip
//! back into the entry — is readable in one place.

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
}

pub(in crate::ui) fn install(search: &Rc<SectionSearch>, views: &SectionSearchViews<'_>) {
    install_tracks(search, views.track_list);
    install_podcasts(search, SearchScope::Podcasts, views.podcasts_view);
    install_podcasts(search, SearchScope::Youtube, views.youtube_view);
    install_radio(search, views.radio_view);
    install_releases(search, views.releases_view);
    install_concerts(search, views.concerts_view);
}

/// Music and Missing files share the track list, whose own debounced search
/// path (`view_session::wire_search`) already applies the query to the table.
/// Music therefore registers no `apply` at all — one would run the same
/// reload twice per keystroke.
///
/// Missing files is the exception: its rows are rendered by
/// `MissingFilesView`, not by the table, and that view has to be told the
/// query separately (FIL-1d — it matches file paths, which the table's
/// "any field" search does not).
fn install_tracks(search: &Rc<SectionSearch>, track_list: &Rc<TrackList>) {
    let clearing = Rc::downgrade(track_list);
    search.register(
        SearchScope::Tracks,
        |_| {},
        move || {
            if let Some(track_list) = clearing.upgrade() {
                track_list.clear_all_restrictions();
            }
        },
    );
    let applying = Rc::downgrade(track_list);
    let clearing = Rc::downgrade(track_list);
    search.register(
        SearchScope::Missing,
        move |query| {
            if let Some(track_list) = applying.upgrade() {
                track_list.set_missing_search_query(query);
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
    let clear_view = Rc::downgrade(view);
    search.register(
        scope,
        move |query| {
            if let Some(view) = apply_view.upgrade() {
                view.set_search_query(query);
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
    let clear_view = Rc::downgrade(view);
    search.register(
        SearchScope::Radio,
        move |query| {
            if let Some(view) = apply_view.upgrade() {
                view.set_search_query(query);
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
    let clear_view = Rc::downgrade(view);
    search.register(
        SearchScope::Releases,
        move |query| {
            if let Some(view) = apply_view.upgrade() {
                view.set_search_query(query);
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
    let clear_view = Rc::downgrade(view);
    search.register(
        SearchScope::Concerts,
        move |query| {
            if let Some(view) = apply_view.upgrade() {
                view.set_search_query(query);
            }
        },
        move || {
            if let Some(view) = clear_view.upgrade() {
                view.clear_all_filters();
            }
        },
    );
}
