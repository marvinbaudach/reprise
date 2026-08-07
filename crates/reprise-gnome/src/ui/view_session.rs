//! Batched track-view restoration with one final query reload.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::browser::BrowserPlace;
use reprise_core::library::session::{SessionSource, SessionState};
use reprise_core::library::settings;
use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;

use crate::ui::column_layout::{ColumnId, ColumnRegistry};
use crate::ui::sidebar::Sidebar;
use crate::ui::track_list::{reload, track_list_reload, TrackList};
use crate::ui::track_list_sort::{restored_sort, SortState};

const SMOKE_ENV: &str = "REPRISE_SMOKE_VIEW_SESSION";
/// SEARCH-9: the one and only wait between typing and the result. GTK's own
/// `search-delay` is switched off in `window.rs`, so this is not stacked on
/// top of anything — raising it here raises the felt latency one-to-one.
const SEARCH_DEBOUNCE_MS: u64 = 150;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TrackViewSnapshot {
    pub(super) source: ViewSource,
    pub(super) search: String,
    pub(super) browse: BrowseFilter,
    pub(super) sort: SortState,
}

pub(super) type SearchRestoreGuard = Rc<Cell<bool>>;

pub(super) fn new_search_restore_guard() -> SearchRestoreGuard {
    Rc::new(Cell::new(false))
}

pub(super) fn restore(
    search_entry: &gtk4::SearchEntry,
    track_list: &TrackList,
    sidebar: &Sidebar,
    window_title: &adw::WindowTitle,
    search_guard: &SearchRestoreGuard,
    state: &SessionState,
) {
    search_guard.set(true);
    search_entry.set_text(&state.search);
    prepare_track_view(
        track_list,
        &state.search,
        &state.browse,
        &state.sort_field,
        &state.sort_dir,
    );
    let (source, title) = sidebar.restore_source(to_view_source(&state.source));
    window_title.set_title(&title);
    let viewed = record_issue_viewed(&track_list.shared.conn, &source, now_unix());
    match viewed {
        Ok(true) => sidebar.refresh("restored issue view opened"),
        Ok(false) => {}
        Err(error) => tracing::error!(%error, "failed to record restored issue view as viewed"),
    }
    finish_track_source(track_list, &source, None);
    search_guard.set(false);
}

pub(super) fn snapshot(track_list: &TrackList) -> TrackViewSnapshot {
    TrackViewSnapshot {
        source: track_list.shared.source.borrow().clone(),
        search: track_list.shared.filter.borrow().clone(),
        browse: track_list.shared.browse_filter.borrow().clone(),
        sort: track_list.shared.sort.borrow().clone(),
    }
}

/// `applies_here` answers SEARCH-8's question: is a track section the visible
/// one? A query typed in Podcasts still reaches this handler — one entry
/// serves the whole window — and must be ignored here rather than silently
/// re-filtering Music behind the user's back.
pub(super) fn wire_search(
    search_entry: &gtk4::SearchEntry,
    track_list: Rc<TrackList>,
    restoring: SearchRestoreGuard,
    applies_here: Rc<dyn Fn() -> bool>,
) {
    {
        let entry = search_entry.clone();
        let restoring = restoring.clone();
        track_list.set_on_search_restored(move |search| {
            restoring.set(true);
            entry.set_text(search);
            restoring.set(false);
        });
    }
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    search_entry.connect_search_changed(move |entry| {
        // A same-value programmatic update (notably clear-all setting the
        // model first and the entry second) still has to cancel a pending
        // older debounce, or that stale text can reapply after the reset.
        //
        // SEARCH-9: this cancellation now runs *before* the guards below, and
        // that ordering is load-bearing. With `search-delay` at 0 the
        // `search-changed` of a programmatic `set_text` arrives while
        // `restoring` is still set — a navigation restoring its own query —
        // where the old code returned early and left a timer armed. That timer
        // then fired against the view the user had just moved to, applying a
        // query and a `pre_search_anchor` belonging to the view they had left.
        // GTK's own 150 ms delay used to hide this by pushing the emission
        // past the guard's reset; removing the delay removed the accident that
        // made the old order work.
        if let Some(previous) = pending.borrow_mut().take() {
            previous.remove();
        }
        if restoring.get() || !applies_here() {
            return;
        }
        let current_filter = track_list.shared.filter.borrow().clone();
        if current_filter == entry.text() {
            return;
        }
        let text = entry.text().to_string();
        track_list_reload::prepare_filter_change(
            &track_list.shared,
            current_filter.as_str(),
            text.as_str(),
        );
        // Browser state follows the visible entry synchronously so leaving a
        // place during the debounce window still captures the exact query.
        *track_list.shared.filter.borrow_mut() = text.clone();
        // SEARCH-9: clearing is not typing. Esc, the chip's ×, "Show all N
        // tracks" and a hand-emptied field all arrive here as empty text, and
        // none of them is the middle of a sequence worth waiting out.
        if text.is_empty() {
            track_list_reload::reload_filter_change(&track_list.shared, current_filter.as_str());
            return;
        }
        let track_list = track_list.clone();
        let pending_for_timeout = pending.clone();
        let source_id =
            glib::timeout_add_local(Duration::from_millis(SEARCH_DEBOUNCE_MS), move || {
                track_list_reload::reload_filter_change(
                    &track_list.shared,
                    current_filter.as_str(),
                );
                pending_for_timeout.borrow_mut().take();
                glib::ControlFlow::Break
            });
        *pending.borrow_mut() = Some(source_id);
    });
}

pub(super) fn restore_browser_place(track_list: &TrackList, place: &BrowserPlace) -> bool {
    let BrowserPlace::Tracks(track_place) = place else {
        return false;
    };
    let source = place.view_source();
    let saved =
        crate::ui::track_list::view_state_memory::SavedViewState::from_core(&track_place.state);
    prepare_track_view(
        track_list,
        &saved.search,
        &saved.browse,
        &saved.sort.field,
        &saved.sort.dir,
    );
    finish_track_source(track_list, &source, Some(&saved));
    let ids = track_list.shared.current_view_ids();
    crate::ui::track_list::view_state_memory::restore(&track_list.shared, &saved, &ids);
    if let Some(callback) = track_list.shared.on_search_restored.borrow().as_ref() {
        callback(&saved.search);
    }
    true
}

fn prepare_track_view(
    track_list: &TrackList,
    search: &str,
    browse: &BrowseFilter,
    sort_field: &str,
    sort_dir: &str,
) {
    let shared = &track_list.shared;
    shared.restoring_view.set(true);
    // SEARCH-9: an anchor belongs to the search it was captured in. Every route
    // through here installs a different place's query and sort, so whatever the
    // previous view had remembered points at a row this one may not even
    // contain. `set_source_and_reload` clears it for plain source switches;
    // this covers the navigation paths (sidebar, Back/Forward, session
    // restore) that reach a new view without going through it.
    shared.pre_search_anchor.set(None);
    *shared.filter.borrow_mut() = search.to_string();
    *shared.browse_filter.borrow_mut() = browse.clone();
    shared.browse_bar.restore_filter(browse);

    let sort = visible_restored_sort(&track_list.column_registry, sort_field, sort_dir);
    *shared.sort.borrow_mut() = sort.clone();
    let id = ColumnId::from_sort_field(&sort.field).unwrap_or(ColumnId::Title);
    if let Some(column) = track_list.column_registry.column(id) {
        let order = if sort.dir == "desc" {
            gtk4::SortType::Descending
        } else {
            gtk4::SortType::Ascending
        };
        crate::ui::track_list_sort::sort_by_column(&shared.column_view, column, order);
    }
    shared.restoring_view.set(false);
}

fn visible_restored_sort(registry: &ColumnRegistry, field: &str, dir: &str) -> SortState {
    let sort = restored_sort(field, dir);
    let id = ColumnId::from_sort_field(&sort.field).unwrap_or(ColumnId::Title);
    if registry.is_visible(id) {
        sort
    } else {
        restored_sort("title", "asc")
    }
}

/// Swaps the source and re-runs the query.
///
/// `target` is the viewport this restore is *going* to install a moment later
/// (`view_state_memory::restore`). Handing it to the reload instead of letting
/// it capture the live one is what keeps a reveal from being pulled back:
/// `reload` otherwise preserves where the user came from — with a 250 ms
/// `AdjustmentHold` guarding it — and then fights the anchor that follows.
/// Both halves now aim at the same row, so the hold protects the reveal
/// instead of undoing it. A restore without an anchor of its own (a plain
/// source switch, the startup session restore) keeps the preserving reload.
fn finish_track_source(
    track_list: &TrackList,
    source: &ViewSource,
    target: Option<&crate::ui::track_list::view_state_memory::SavedViewState>,
) {
    *track_list.shared.source.borrow_mut() = source.clone();
    track_list.shared.browse_bar.set_source_context(source);
    match target.filter(|saved| saved.anchor.is_some()) {
        Some(saved) => crate::ui::track_list::track_list_reload::reload_with_anchor(
            &track_list.shared,
            &crate::ui::track_list::reload_restore::capture(
                saved.selected_ids.clone(),
                saved.anchor,
            ),
        ),
        None => reload(&track_list.shared),
    }
}

pub(super) fn arm_smoke(
    search_entry: &gtk4::SearchEntry,
    track_list: &Rc<TrackList>,
    sidebar: &Rc<Sidebar>,
    window_title: &adw::WindowTitle,
    search_guard: &SearchRestoreGuard,
) {
    let Ok(value) = std::env::var(SMOKE_ENV) else {
        return;
    };
    let Some(state) = parse_smoke_state(&value) else {
        tracing::warn!(value, "invalid view-session smoke fixture");
        return;
    };
    let search_entry = search_entry.clone();
    let track_list = track_list.clone();
    let sidebar = sidebar.clone();
    let window_title = window_title.clone();
    let search_guard = search_guard.clone();
    glib::idle_add_local_once(move || {
        restore(
            &search_entry,
            &track_list,
            &sidebar,
            &window_title,
            &search_guard,
            &state,
        );
        let restored = snapshot(&track_list);
        tracing::info!(
            source = %restored.source.label(),
            search = %restored.search,
            ?restored.browse,
            field = %restored.sort.field,
            dir = %restored.sort.dir,
            "view session smoke restored"
        );
    });
}

fn to_view_source(source: &SessionSource) -> ViewSource {
    match source {
        SessionSource::Library => ViewSource::Library,
        SessionSource::RecentlyAdded => ViewSource::RecentlyAdded,
        SessionSource::Playlist(id) => ViewSource::Playlist(*id),
        SessionSource::Smart(id) => ViewSource::Smart(*id),
        SessionSource::Queue => ViewSource::Queue,
        SessionSource::Missing => ViewSource::Missing,
        SessionSource::ImportErrors => ViewSource::ImportErrors,
    }
}

fn parse_smoke_state(value: &str) -> Option<SessionState> {
    let mut fields = value.split('|');
    let source = match fields.next()? {
        "library" => SessionSource::Library,
        "recently-added" => SessionSource::RecentlyAdded,
        "queue" => SessionSource::Queue,
        "missing" => SessionSource::Missing,
        "import-errors" => SessionSource::ImportErrors,
        value if value.starts_with("playlist:") => {
            SessionSource::Playlist(value.strip_prefix("playlist:")?.parse().ok()?)
        }
        value if value.starts_with("smart:") => {
            SessionSource::Smart(value.strip_prefix("smart:")?.parse().ok()?)
        }
        _ => return None,
    };
    let search = fields.next()?.to_string();
    let browse = BrowseFilter {
        genre: parse_optional(fields.next()?),
        artist: parse_optional(fields.next()?),
        album: parse_optional(fields.next()?),
        // Year/Rating are not part of the smoke fixture's wire format.
        ..BrowseFilter::default()
    };
    let sort_field = fields.next()?.to_string();
    let sort_dir = fields.next()?.to_string();
    if fields.next().is_some() {
        return None;
    }
    Some(SessionState {
        source,
        search,
        browse,
        sort_field,
        sort_dir,
        ..SessionState::default()
    })
}

fn parse_optional(value: &str) -> Option<String> {
    (value != "~").then(|| value.to_string())
}

pub(in crate::ui) fn record_issue_viewed(
    db: &reprise_core::db::Db,
    source: &ViewSource,
    now: i64,
) -> Result<bool, rusqlite::Error> {
    match source {
        ViewSource::Missing => settings::set_last_viewed_missing(db, now)?,
        ViewSource::ImportErrors => settings::set_last_viewed_import_errors(db, now)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod issue_view_tests {
    use super::*;
    use reprise_core::library::settings;

    #[test]
    fn opening_each_issue_view_records_only_its_last_viewed_clock() {
        let conn = crate::test_db::open().unwrap();

        assert!(record_issue_viewed(&conn, &ViewSource::Missing, 111).unwrap());
        assert_eq!(settings::get_last_viewed_missing(&conn).unwrap(), 111);
        assert_eq!(settings::get_last_viewed_import_errors(&conn).unwrap(), 0);

        assert!(record_issue_viewed(&conn, &ViewSource::ImportErrors, 222).unwrap());
        assert_eq!(settings::get_last_viewed_missing(&conn).unwrap(), 111);
        assert_eq!(settings::get_last_viewed_import_errors(&conn).unwrap(), 222);

        assert!(!record_issue_viewed(&conn, &ViewSource::Library, 333).unwrap());
        assert_eq!(settings::get_last_viewed_missing(&conn).unwrap(), 111);
        assert_eq!(settings::get_last_viewed_import_errors(&conn).unwrap(), 222);
    }
}

#[cfg(test)]
mod tests {
    use super::SEARCH_DEBOUNCE_MS;

    /// SEARCH-9: exactly one wait sits between a keystroke and the result, and
    /// it is this one. 150 ms is the agreed value; the constant existing at
    /// 200 means GTK's own 150 ms delay is stacked underneath it.
    #[test]
    fn search_9_debounce_is_the_only_wait() {
        assert_eq!(SEARCH_DEBOUNCE_MS, 150);
    }
}
