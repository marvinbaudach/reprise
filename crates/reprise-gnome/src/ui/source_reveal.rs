//! Shared "should this move the viewport?" decision for the source lists.
//!
//! `SRC-12`: marking and scrolling are separate. Podcasts, YouTube and Radio
//! all answer "when do we reveal the loaded item" here, so the three surfaces
//! cannot drift into three answers. How a surface reveals differs (a grouped
//! expander tree versus a flat uniform table) and lives in each view's own
//! reveal module; *whether* it reveals is decided once, here.

use std::time::{Duration, Instant};

/// What caused the loaded item to change, from the point of view of the list
/// that is deciding whether to move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum LoadedItemChange {
    /// A row in this very list was activated. The row was therefore visible.
    ActivatedHere,
    /// The loaded item changed without this list causing it — the player bar,
    /// another surface, or a restored session.
    ChangedElsewhere,
    /// The list just became the visible page.
    ViewEntered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum RevealPolicy {
    /// Update the marker, leave the viewport exactly where it is.
    MarkerOnly,
    /// Bring the loaded item into view and center it.
    Reveal,
}

/// How long after the last scroll movement the list still counts as
/// "the user is reading it". Same value and same purpose as the track table's
/// `current_track_selection::USER_SCROLL_GRACE` (`NAV-10a`).
pub(in crate::ui) const USER_SCROLL_GRACE: Duration = Duration::from_millis(1_500);

/// Whether the last recorded scroll movement is recent enough that the list
/// belongs to the user right now.
pub(in crate::ui) fn is_user_scrolling(last_activity: Option<Instant>) -> bool {
    last_activity.is_some_and(|last| last.elapsed() < USER_SCROLL_GRACE)
}

/// `SRC-12`: activating a row never moves the viewport, because the row was
/// visible; a change caused elsewhere yields to recent scrolling; entering the
/// view always reveals.
pub(in crate::ui) fn reveal_policy(change: LoadedItemChange, user_scrolling: bool) -> RevealPolicy {
    match change {
        LoadedItemChange::ActivatedHere => RevealPolicy::MarkerOnly,
        LoadedItemChange::ChangedElsewhere if user_scrolling => RevealPolicy::MarkerOnly,
        LoadedItemChange::ChangedElsewhere | LoadedItemChange::ViewEntered => RevealPolicy::Reveal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn src_12_activating_a_row_never_moves_the_viewport() {
        // The row was visible — that is what made it activatable.
        assert_eq!(
            reveal_policy(LoadedItemChange::ActivatedHere, false),
            RevealPolicy::MarkerOnly
        );
        assert_eq!(
            reveal_policy(LoadedItemChange::ActivatedHere, true),
            RevealPolicy::MarkerOnly
        );
    }

    #[test]
    fn src_12_a_change_from_elsewhere_yields_to_recent_scrolling() {
        assert_eq!(
            reveal_policy(LoadedItemChange::ChangedElsewhere, true),
            RevealPolicy::MarkerOnly
        );
        assert_eq!(
            reveal_policy(LoadedItemChange::ChangedElsewhere, false),
            RevealPolicy::Reveal
        );
    }

    #[test]
    fn src_12_entering_the_view_always_reveals() {
        assert_eq!(
            reveal_policy(LoadedItemChange::ViewEntered, true),
            RevealPolicy::Reveal
        );
        assert_eq!(
            reveal_policy(LoadedItemChange::ViewEntered, false),
            RevealPolicy::Reveal
        );
    }

    #[test]
    fn src_12_scroll_grace_matches_the_track_list_and_expires() {
        assert_eq!(USER_SCROLL_GRACE, Duration::from_millis(1_500));
        assert!(!is_user_scrolling(None));
        assert!(is_user_scrolling(Some(Instant::now())));
        assert!(!is_user_scrolling(
            Instant::now().checked_sub(Duration::from_secs(10))
        ));
    }
}
