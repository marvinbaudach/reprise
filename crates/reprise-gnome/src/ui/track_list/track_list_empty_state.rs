//! The track table's empty/populated page switching: which `gtk::Stack`
//! page a reload shows (list, dedicated ImportErrors panel, or a
//! `StatusPage`) and what that StatusPage says — split out of
//! `track_list_columns.rs` (file-size rule). QUE-4 lives here: the Queue
//! source's dedicated "Nothing queued — play something" state.

use std::rc::Rc;

use libadwaita as adw;

use crate::ui::strings;
use crate::ui::track_list::{Shared, STACK_PAGE_EMPTY, STACK_PAGE_IMPORT_ERRORS, STACK_PAGE_LIST};
use reprise_core::view_source::ViewSource;

/// Icon shown on the empty-library placeholder (nothing has been scanned
/// in yet).
const ICON_EMPTY_LIBRARY: &str = "folder-music-symbolic";

/// Icon shown when a search filter matched zero rows — distinct from the
/// empty-library icon so the two states also read differently at a glance.
const ICON_NO_RESULTS: &str = "system-search-symbolic";

/// Icon shown for the neutral "nothing here" state (`Missing`/`ImportErrors`
/// sources with no rows and no active filter) — distinct from both of the
/// above: this isn't "no music has been scanned in" nor "your search
/// matched nothing", just "this particular view has no members right now".
const ICON_NOTHING_HERE: &str = "dialog-information-symbolic";

/// Which page of the track-list `Stack` should be visible, and (for the
/// empty variants) which copy the shared `StatusPage` should carry. A plain
/// enum decided by a pure function (`empty_state_for`) so the selection
/// logic is unit-testable without a running GTK application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum EmptyState {
    /// The library itself has no tracks yet (no filter active either).
    EmptyLibrary,
    /// A neutral "nothing here" state (Stage 3 Task 3): the `Missing`/
    /// `ImportErrors` sources have no rows and no filter is active — unlike
    /// `EmptyLibrary`, this isn't about the library having no music at all,
    /// just this particular view having no members right now.
    NothingHere,
    /// The current source has rows, but the active search filter matched
    /// none.
    NoResults,
    /// QUE-4: the Queue source with nothing playing and nothing pending.
    /// Deliberately its own copy ("Nothing queued — play something"), one
    /// next step per FB-5, instead of the EmptyLibrary scan prompt.
    EmptyQueue,
    /// At least one row to show — the populated list page.
    List,
}

/// Pure decision of which empty state (or the populated list) applies for a
/// given result-row count, whether a search filter is currently active, and
/// which `ViewSource` is showing. Kept side-effect free and separate from
/// `reload`/`apply_empty_state` so it can be unit tested directly instead of
/// only through a live GTK stack. `source` only matters for the
/// zero-rows/no-filter case: `Missing`/`ImportErrors` get the neutral
/// `NothingHere` copy there instead of `EmptyLibrary`'s "no music yet"
/// (which would be a confusing thing to say about, e.g., a "no files are
/// currently missing" state — that's good news, not an invitation to scan a
/// folder).
pub(in crate::ui) fn empty_state_for(
    row_count: usize,
    has_filter: bool,
    source: &ViewSource,
) -> EmptyState {
    match (row_count, has_filter) {
        (0, true) => EmptyState::NoResults,
        (0, false) => match source {
            ViewSource::Missing
            | ViewSource::ImportErrors
            | ViewSource::Album { .. }
            | ViewSource::Artist(_) => EmptyState::NothingHere,
            ViewSource::Queue => EmptyState::EmptyQueue,
            _ => EmptyState::EmptyLibrary,
        },
        _ => EmptyState::List,
    }
}

/// Builds the shared empty-state placeholder, initially carrying the
/// empty-library copy (the state `TrackList::new`'s first `reload()` will
/// normally confirm, since there's no library yet on first launch).
/// `apply_empty_state` swaps its title/description/icon in place for the
/// no-results case rather than building a second widget.
pub(in crate::ui) fn build_status_page() -> adw::StatusPage {
    adw::StatusPage::builder()
        .icon_name(ICON_EMPTY_LIBRARY)
        .title(strings::text(strings::EMPTY_LIBRARY_TITLE))
        .description(strings::text(strings::EMPTY_LIBRARY_DESCRIPTION))
        .vexpand(true)
        .build()
}

/// Applies an `EmptyState` decision to the widget tree. For the two empty
/// variants this mutates the single shared `StatusPage`'s title,
/// description, and icon in place before switching the stack to it, rather
/// than maintaining a third stack page — the empty page's layout role
/// (centered icon + title + description, `vexpand`) never changes, only its
/// copy does, so swapping three properties on one widget is simpler than
/// building and switching between two near-identical `StatusPage`s.
pub(in crate::ui) fn apply_empty_state(shared: &Rc<Shared>, state: EmptyState) {
    match state {
        EmptyState::List => {
            // Stage 3 Task 8: the ImportErrors source's populated page is the
            // dedicated panel, not the shared `ColumnView` page — every other
            // source keeps using `STACK_PAGE_LIST` exactly as before.
            let page = if matches!(*shared.source.borrow(), ViewSource::ImportErrors) {
                STACK_PAGE_IMPORT_ERRORS
            } else {
                STACK_PAGE_LIST
            };
            shared.stack.set_visible_child_name(page);
        }
        EmptyState::EmptyLibrary => {
            shared.empty_page.set_icon_name(Some(ICON_EMPTY_LIBRARY));
            shared
                .empty_page
                .set_title(&strings::text(strings::EMPTY_LIBRARY_TITLE));
            shared
                .empty_page
                .set_description(Some(&strings::text(strings::EMPTY_LIBRARY_DESCRIPTION)));
            shared.stack.set_visible_child_name(STACK_PAGE_EMPTY);
        }
        EmptyState::NoResults => {
            shared.empty_page.set_icon_name(Some(ICON_NO_RESULTS));
            shared
                .empty_page
                .set_title(&strings::text(strings::NO_RESULTS_TITLE));
            shared
                .empty_page
                .set_description(Some(&strings::text(strings::NO_RESULTS_DESCRIPTION)));
            shared.stack.set_visible_child_name(STACK_PAGE_EMPTY);
        }
        EmptyState::EmptyQueue => {
            shared.empty_page.set_icon_name(Some(ICON_NOTHING_HERE));
            shared
                .empty_page
                .set_title(&strings::text(strings::EMPTY_QUEUE_TITLE));
            shared
                .empty_page
                .set_description(Some(&strings::text(strings::EMPTY_QUEUE_DESCRIPTION)));
            shared.stack.set_visible_child_name(STACK_PAGE_EMPTY);
        }
        EmptyState::NothingHere => {
            shared.empty_page.set_icon_name(Some(ICON_NOTHING_HERE));
            shared
                .empty_page
                .set_title(&strings::text(strings::NOTHING_HERE_TITLE));
            shared
                .empty_page
                .set_description(Some(&strings::text(strings::NOTHING_HERE_DESCRIPTION)));
            shared.stack.set_visible_child_name(STACK_PAGE_EMPTY);
        }
    }
    // Debug level (not info): this fires on every reload, including every
    // keystroke-debounced search, so it would be noisy at the default log
    // level — but it's exactly what a headless run needs to assert which
    // empty state (if any) is currently shown.
    tracing::debug!(?state, "track list empty-state page selected");
}

#[cfg(test)]
mod empty_state_tests {
    use super::*;

    #[test]
    fn empty_library_when_no_rows_and_no_filter_for_library_source() {
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Library),
            EmptyState::EmptyLibrary
        );
    }

    #[test]
    fn no_results_when_no_rows_and_filter_active_regardless_of_source() {
        assert_eq!(
            empty_state_for(0, true, &ViewSource::Library),
            EmptyState::NoResults
        );
        assert_eq!(
            empty_state_for(0, true, &ViewSource::Missing),
            EmptyState::NoResults
        );
        assert_eq!(
            empty_state_for(0, true, &ViewSource::ImportErrors),
            EmptyState::NoResults
        );
    }

    #[test]
    fn list_when_rows_present_regardless_of_filter_or_source() {
        assert_eq!(
            empty_state_for(3, false, &ViewSource::Library),
            EmptyState::List
        );
        assert_eq!(
            empty_state_for(3, true, &ViewSource::Missing),
            EmptyState::List
        );
    }

    /// Stage 3 Task 3: `Missing`/`ImportErrors` get the neutral "nothing
    /// here" copy for the zero-rows/no-filter case, not `EmptyLibrary`'s
    /// "no music yet" (which would read oddly for "no files are currently
    /// missing").
    #[test]
    fn nothing_here_for_transient_or_issue_sources_with_no_rows_and_no_filter() {
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Missing),
            EmptyState::NothingHere
        );
        assert_eq!(
            empty_state_for(0, false, &ViewSource::ImportErrors),
            EmptyState::NothingHere
        );
        assert_eq!(
            empty_state_for(
                0,
                false,
                &ViewSource::Album {
                    album: "Blue".into(),
                    album_artist: "Joni Mitchell".into(),
                },
            ),
            EmptyState::NothingHere
        );
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Artist("Björk".into())),
            EmptyState::NothingHere
        );
    }

    /// Non-Library, non-Missing/ImportErrors sources (Playlist, Smart)
    /// still get `EmptyLibrary`'s copy for now — a dedicated "this playlist
    /// has no tracks yet" message is left to a later stage. The Queue source
    /// has its own QUE-4 copy since the queue+nav plan.
    #[test]
    fn playlist_and_smart_fall_back_to_empty_library_copy() {
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Playlist(1)),
            EmptyState::EmptyLibrary
        );
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Smart(1)),
            EmptyState::EmptyLibrary
        );
    }

    #[test]
    fn empty_queue_gets_its_own_que4_state() {
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Queue),
            EmptyState::EmptyQueue
        );
    }
}
