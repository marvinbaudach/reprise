//! Shared "should this move the viewport?" decision for the source lists.
//!
//! `SRC-13`: marking and scrolling are separate. Podcasts, YouTube and Radio
//! all answer "when do we reveal the loaded item" here, so the three surfaces
//! cannot drift into three answers. How a surface reveals differs (a grouped
//! expander tree versus a flat uniform table) and lives in each view's own
//! reveal module; *whether* it reveals is decided once, here.

use std::time::{Duration, Instant};

use gtk4::prelude::*;

/// What caused the loaded item to change, from the point of view of the list
/// that is deciding whether to move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum LoadedItemChange {
    /// A row in this very list was activated. The row was therefore visible.
    ActivatedHere,
    /// The loaded item changed without this list causing it — the player bar,
    /// or another surface.
    ChangedElsewhere,
    /// The process reconstructed the last loaded item at cold start. This is
    /// the one change that intentionally restores selection and position.
    SessionRestore,
    /// The list just became the visible page.
    ViewEntered,
    /// `SRC-13`: the user asked for this jump from the player bar or
    /// `Ctrl+L`. It always reveals — also in the already visible view and
    /// regardless of the 1.5-second grace period. The grace protects a
    /// reading user from a viewport that jumps under their hand; here they
    /// asked for the jump themselves.
    RequestedByUser,
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
/// `current_track_selection::USER_SCROLL_GRACE` (`NAV-10b`).
pub(in crate::ui) const USER_SCROLL_GRACE: Duration = Duration::from_millis(1_500);

/// Records source-list scroll activity from input rather than adjustment
/// changes.
///
/// Capture phase and `Proceed` let the scrolled window keep handling wheel and
/// touchpad input. The scrollbar needs its own gesture: placing one drag
/// gesture over the entire scroll area would compete with the rows' own
/// `DragSource` pointer sequences.
pub(in crate::ui) fn install_scroll_activity_tracking(
    scroller: &gtk4::ScrolledWindow,
    mark_activity: impl Fn() + 'static,
) {
    let mark_activity = std::rc::Rc::new(mark_activity);
    // input-parity: ACC-8 keyboard=scrolled-window-navigation
    let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::BOTH_AXES);
    scroll.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let mark_scroll_activity = mark_activity.clone();
    scroll.connect_scroll(move |_, _, _| {
        mark_scroll_activity();
        gtk4::glib::Propagation::Proceed
    });
    scroller.add_controller(scroll);

    // input-parity: ACC-8 keyboard=scrollbar-navigation
    let scrollbar_drag = gtk4::GestureDrag::new();
    scrollbar_drag.set_propagation_phase(gtk4::PropagationPhase::Capture);
    scrollbar_drag.connect_drag_update(move |_, _, _| mark_activity());
    scroller.vscrollbar().add_controller(scrollbar_drag);
}

/// Whether the last recorded scroll movement is recent enough that the list
/// belongs to the user right now.
pub(in crate::ui) fn is_user_scrolling(last_activity: Option<Instant>) -> bool {
    last_activity.is_some_and(|last| last.elapsed() < USER_SCROLL_GRACE)
}

/// `SRC-13`: activating a row never moves the viewport, because the row was
/// visible; a change caused elsewhere yields to recent scrolling; entering the
/// view always reveals.
pub(in crate::ui) fn reveal_policy(change: LoadedItemChange, user_scrolling: bool) -> RevealPolicy {
    match change {
        LoadedItemChange::ActivatedHere => RevealPolicy::MarkerOnly,
        LoadedItemChange::ChangedElsewhere if user_scrolling => RevealPolicy::MarkerOnly,
        LoadedItemChange::ChangedElsewhere
        | LoadedItemChange::SessionRestore
        | LoadedItemChange::ViewEntered
        | LoadedItemChange::RequestedByUser => RevealPolicy::Reveal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_13_only_source_list_input_marks_the_user_as_scrolling() {
        gtk4::init().unwrap();
        let scroller = gtk4::ScrolledWindow::new();
        let last_activity = Rc::new(Cell::new(None));
        let activity = last_activity.clone();
        install_scroll_activity_tracking(&scroller, move || {
            activity.set(Some(Instant::now()));
        });
        let adjustment = scroller.vadjustment();
        adjustment.configure(0.0, 0.0, 500.0, 1.0, 20.0, 100.0);

        adjustment.set_value(40.0);

        assert_eq!(
            reveal_policy(
                LoadedItemChange::ChangedElsewhere,
                is_user_scrolling(last_activity.get()),
            ),
            RevealPolicy::Reveal,
            "a programmatic adjustment write is not user input"
        );

        let controllers = scroller.observe_controllers();
        let scroll = (0..controllers.n_items())
            .find_map(|index| {
                controllers
                    .item(index)?
                    .downcast::<gtk4::EventControllerScroll>()
                    .ok()
            })
            .expect("the source scroller has the shared input witness");
        let stopped = scroll.emit_by_name::<bool>("scroll", &[&0.0_f64, &1.0_f64]);

        assert!(!stopped, "the input witness must let scrolling proceed");
        assert_eq!(
            reveal_policy(
                LoadedItemChange::ChangedElsewhere,
                is_user_scrolling(last_activity.get()),
            ),
            RevealPolicy::MarkerOnly,
            "a synthesized source-list scroll starts the user grace period"
        );
    }

    #[test]
    fn src_13_podcasts_youtube_and_radio_use_the_shared_input_wiring() {
        let podcasts = include_str!("podcasts/podcasts_view_marker.rs");
        let radio = include_str!("radio/radio_reveal.rs");
        // Podcasts and YouTube are two kinds rendered by the same view, while
        // Radio has its own view. Both construction paths must use the one
        // shared input witness.
        for (surface, source) in [
            ("Podcasts", podcasts),
            ("YouTube", podcasts),
            ("Radio", radio),
        ] {
            assert_eq!(
                source.matches("install_scroll_activity_tracking").count(),
                1,
                "{surface} must wire the shared source-list input helper exactly once"
            );
        }
    }

    #[test]
    fn src_13_activating_a_row_never_moves_the_viewport() {
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
    fn src_13_a_change_from_elsewhere_yields_to_recent_scrolling() {
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
    fn src_13_entering_the_view_always_reveals() {
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
    fn src_13_a_user_requested_jump_always_reveals() {
        assert_eq!(
            reveal_policy(LoadedItemChange::RequestedByUser, true),
            RevealPolicy::Reveal
        );
        assert_eq!(
            reveal_policy(LoadedItemChange::RequestedByUser, false),
            RevealPolicy::Reveal
        );
    }

    #[test]
    fn start_4_session_restore_always_reveals() {
        assert_eq!(
            reveal_policy(LoadedItemChange::SessionRestore, true),
            RevealPolicy::Reveal
        );
        assert_eq!(
            reveal_policy(LoadedItemChange::SessionRestore, false),
            RevealPolicy::Reveal
        );
    }

    #[test]
    fn src_13_scroll_grace_matches_the_track_list_and_expires() {
        assert_eq!(USER_SCROLL_GRACE, Duration::from_millis(1_500));
        assert!(!is_user_scrolling(None));
        assert!(is_user_scrolling(Some(Instant::now())));
        assert!(!is_user_scrolling(
            Instant::now().checked_sub(Duration::from_secs(10))
        ));
    }
}
