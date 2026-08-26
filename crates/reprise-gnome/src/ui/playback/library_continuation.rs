//! PLAY-11/PLAY-15: what happens to a filter-born playback snapshot once the
//! Library filter that created it is gone.
//!
//! Playback is an immutable snapshot (PLAY-3b/PLAY-8): applying, refining, or
//! swapping a filter never rewrites a running queue. Clearing the filter is
//! the narrow exception: PLAY-15 rebinds a snapshot that still has a future,
//! while PLAY-11 continues one that is already exhausted. This module owns the
//! decision end to end so the shared eligibility gate cannot drift:
//!
//! * **Still has titles ahead** — the now-unfiltered visible list replaces the
//!   filtered snapshot without restarting the running title
//!   (`continue_library_after_filter_clear`).
//! * **Exhausted while a track is still playing** — the user cleared the
//!   filter after starting its last hit. The context has no future left, so
//!   the continuation is bound in immediately and the panel stops claiming
//!   "Queue is empty" for the rest of the running title
//!   (`continue_library_after_filter_clear`).
//! * **Exhausted at the end of the title** — nothing was bound in earlier
//!   (the filter was cleared only after the last title had already started
//!   its final seconds, or no reload happened in between), so the handoff
//!   runs on the automatic advance instead of falling silent
//!   (`refill_random_library_after_filter_clear`).
//!
//! Both ask [`cleared_library_filter_handoff`] the same question, which is the
//! point: a second copy of "may this snapshot escape into the full library?"
//! is how the two paths would start disagreeing.

use std::collections::HashSet;
use std::rc::Rc;

use reprise_core::queries;
use reprise_core::queue::Repeat;

use super::play_origin::PlayOrigin;
use crate::ui::player_controller::{PlayerController, VisibleView};

#[path = "library_continuation_rebind.rs"]
mod rebind;
use rebind::{
    rebind_live_count_matches_visible, rebind_to_unfiltered_view, rebound_library_origin,
};

/// The shared PLAY-11 gate: may this snapshot hand off to the whole library?
///
/// Answers with the full live library in random order when — and only when —
/// the snapshot was born in a search- or facet-filtered Music library whose
/// filter is now completely gone. "Completely gone" is decided by measuring
/// the view against the live library rather than by reading filter state, so
/// any narrowing the caller does not know about (a facet, the FIL-7 AI
/// exclusion, a source restriction) keeps the gate shut on its own.
///
/// The measurement is the view's **row count**, not its id list, because the
/// id query stops at `queries::QUEUE_LIMIT` rows: past 10,000 live titles the
/// two lists can never be equal, and comparing them would silently switch
/// PLAY-11 off for exactly the libraries that need it most. The ids that were
/// returned are still required to be live and distinct, so a view that
/// swapped rows instead of dropping them cannot slip through either.
fn cleared_library_filter_handoff(
    origin: Option<&PlayOrigin>,
    visible: &VisibleView,
    random_live_ids: Vec<i64>,
) -> Option<Vec<i64>> {
    let origin = origin?;
    if !origin.place.is_library_root() {
        return None;
    }
    let state = origin.place.track_state()?;
    if state.search.trim().is_empty() && state.browse.is_empty() {
        return None;
    }

    if visible.total != random_live_ids.len() {
        return None;
    }
    let live = random_live_ids.iter().copied().collect::<HashSet<_>>();
    if live.len() != random_live_ids.len() {
        return None;
    }
    let mut seen = HashSet::with_capacity(visible.ids.len());
    if !visible
        .ids
        .iter()
        .all(|id| live.contains(id) && seen.insert(*id))
    {
        return None;
    }

    Some(random_live_ids)
}

/// The continuation for a snapshot that ran out *at the end of its last
/// title*: the whole library in random order, rotated so the title that just
/// finished does not start the new snapshot. It may occur later — only the
/// immediate repeat is what PLAY-11 rules out.
fn random_library_continuation(
    origin: Option<&PlayOrigin>,
    visible: &VisibleView,
    random_live_ids: Vec<i64>,
    finished_track_id: Option<i64>,
) -> Option<Vec<i64>> {
    let mut ids = cleared_library_filter_handoff(origin, visible, random_live_ids)?;
    let finished_track_id = finished_track_id?;
    let next_position = ids.iter().position(|id| *id != finished_track_id)?;
    ids.rotate_left(next_position);
    Some(ids)
}

/// The continuation for a snapshot that is *already* exhausted while its last
/// title still plays: the running title stays at the head — untouched, still
/// playing, still the cursor — and the rest of the library follows it in
/// random order.
///
/// The running title is lifted out of the continuation rather than left in
/// it, so it plays exactly once and the remaining count matches what the end
/// of the title would have produced. Returns `None` whenever the ordinary
/// rules still apply: a repeat mode that never exhausts, a context with
/// tracks still ahead, no loaded title, or a library that holds nothing but
/// the running title.
fn immediate_library_continuation(
    origin: Option<&PlayOrigin>,
    repeat: Repeat,
    remaining: usize,
    current_track_id: Option<i64>,
    visible: &VisibleView,
    random_live_ids: Vec<i64>,
) -> Option<Vec<i64>> {
    // Repeat One/All keep their own queue behaviour: neither ever reaches the
    // end that PLAY-11 is about, so neither may be re-bound underneath.
    if repeat != Repeat::Off {
        return None;
    }
    // Anything still ahead means the snapshot is not exhausted, and PLAY-8's
    // immutability applies unchanged.
    if remaining != 0 {
        return None;
    }
    let current_track_id = current_track_id?;

    let mut ids = cleared_library_filter_handoff(origin, visible, random_live_ids)?;
    let current_position = ids.iter().position(|id| *id == current_track_id)?;
    if ids.len() < 2 {
        return None;
    }
    ids.remove(current_position);
    ids.insert(0, current_track_id);
    Some(ids)
}

impl PlayerController {
    /// PLAY-11/PLAY-15, reached from a Library reload (`window.rs`'s
    /// `on_reload`): once the filter that built the running snapshot is gone,
    /// either bind a future into an exhausted context or rebind a context that
    /// still has titles ahead. Playback itself is deliberately untouched.
    ///
    /// Returns whether a continuation was bound in. Callers fire this on every
    /// reload, so the cheap rejections (repeat mode, loaded title, filtered
    /// origin) all come before the two queries.
    pub(in crate::ui) fn continue_library_after_filter_clear(self: &Rc<Self>) -> bool {
        let repeat = self.queue.borrow().repeat();
        let remaining = self.queue.borrow().remaining_len();
        if repeat != Repeat::Off {
            return false;
        }
        let current_track_id = self.current_track.get().map(|(id, _)| id);
        let Some(current_track_id) = current_track_id else {
            return false;
        };
        let origin = self.play_origin.borrow().clone();
        // The filtered origin is what makes this a PLAY-11 case at all, and it
        // is the guard that keeps a bound-in continuation from being re-bound
        // by the very reload it causes: `play_origin` is rewritten to the
        // unfiltered Library below.
        if !origin.as_ref().is_some_and(cleared_filter_origin) {
            return false;
        }
        let provider = self.view_refill_ids.borrow().clone();
        let Some(provider) = provider else {
            return false;
        };
        let visible = provider();
        if remaining != 0 {
            let live_count = match queries::query_track_count(
                &self.conn,
                &reprise_core::view_source::ViewSource::Library,
                "",
                &[],
            ) {
                Ok(count) => count,
                Err(error) => {
                    tracing::error!(%error, "failed to count live library before queue rebind");
                    return false;
                }
            };
            if !rebind_live_count_matches_visible(&visible, live_count) {
                return false;
            }
        }
        let random_live_ids = match queries::query_random_live_track_ids(&self.conn) {
            Ok(ids) => ids,
            Err(error) => {
                tracing::error!(%error, "failed to build immediate library continuation");
                return false;
            }
        };
        if remaining == 0 {
            let Some(ids) = immediate_library_continuation(
                origin.as_ref(),
                repeat,
                remaining,
                Some(current_track_id),
                &visible,
                random_live_ids,
            ) else {
                return false;
            };

            let continuation_len = ids.len().saturating_sub(1);
            self.queue.borrow_mut().set_tracks(ids, 0);
            *self.play_origin.borrow_mut() = Some(PlayOrigin::library());
            self.notify_queue_changed();
            tracing::info!(
                continuation_len,
                "library filter cleared on an exhausted queue; bound in a random library continuation"
            );
            return true;
        }

        let Some((ids, start_index)) = rebind_to_unfiltered_view(
            origin.as_ref(),
            repeat,
            remaining,
            Some(current_track_id),
            &visible,
            random_live_ids,
        ) else {
            return false;
        };

        let snapshot_len = ids.len();
        self.queue.borrow_mut().set_tracks(ids, start_index);
        *self.play_origin.borrow_mut() = Some(rebound_library_origin());
        self.notify_queue_changed();
        tracing::info!(
            snapshot_len,
            "library filter cleared with titles ahead; rebound to the visible library snapshot"
        );
        true
    }

    /// PLAY-11 on the automatic advance: the filtered snapshot is exhausted
    /// and the Library filter is already gone, so a fresh random full-library
    /// snapshot takes over instead of playback falling silent.
    ///
    /// This is the path for a filter cleared too late for
    /// [`Self::continue_library_after_filter_clear`] to have bound anything in
    /// (no Library reload happened in between). Unlike that one it *starts*
    /// playback, because the previous title has ended.
    pub(in crate::ui) fn refill_random_library_after_filter_clear(self: &Rc<Self>) -> bool {
        let origin = self.play_origin.borrow().clone();
        let provider = self.view_refill_ids.borrow().clone();
        let Some(provider) = provider else {
            return false;
        };
        let visible = provider();
        let random_live_ids = match queries::query_random_live_track_ids(&self.conn) {
            Ok(ids) => ids,
            Err(error) => {
                tracing::error!(%error, "failed to build random library continuation");
                return false;
            }
        };
        let finished_track_id = self.current_track.get().map(|(id, _)| id);
        let Some(ids) = random_library_continuation(
            origin.as_ref(),
            &visible,
            random_live_ids,
            finished_track_id,
        ) else {
            return false;
        };
        let continuation_len = ids.len();
        self.play_from_view(ids, 0, PlayOrigin::library());
        tracing::info!(
            continuation_len,
            "filtered queue exhausted after filter clear; continuing from random library snapshot"
        );
        true
    }
}

/// Whether an origin is the search- or facet-filtered Music library PLAY-11
/// speaks about. Kept next to [`cleared_library_filter_handoff`], which
/// repeats this check as part of its own answer, so the cheap pre-filter and
/// the authoritative gate can never mean different things.
fn cleared_filter_origin(origin: &PlayOrigin) -> bool {
    if !origin.place.is_library_root() {
        return false;
    }
    origin
        .place
        .track_state()
        .is_some_and(|state| !state.search.trim().is_empty() || !state.browse.is_empty())
}

#[cfg(test)]
mod tests {
    use reprise_core::browser::BrowserPlace;
    use reprise_core::queue::Repeat;
    use reprise_core::view_source::ViewSource;

    use super::{
        immediate_library_continuation, random_library_continuation, rebind_to_unfiltered_view,
    };
    use crate::ui::playback::play_origin::PlayOrigin;
    use crate::ui::player_controller::VisibleView;

    /// A view showing the whole of what it lists — the ordinary case, where
    /// the id query did not hit `QUEUE_LIMIT`.
    fn whole(ids: &[i64]) -> VisibleView {
        VisibleView {
            ids: ids.to_vec(),
            total: ids.len(),
        }
    }

    fn library_origin(search: &str) -> PlayOrigin {
        let mut place = BrowserPlace::from(ViewSource::Library);
        place.track_state_mut().unwrap().search = search.into();
        PlayOrigin {
            place,
            label: "Music".into(),
        }
    }

    #[test]
    fn play_11_filter_clear_continues_with_random_full_library_snapshot() {
        let origin = library_origin("needle");

        assert_eq!(
            random_library_continuation(Some(&origin), &whole(&[1, 2, 3]), vec![2, 3, 1], Some(2)),
            Some(vec![3, 1, 2]),
            "the finished filtered hit must not immediately repeat when the full library takes over"
        );
    }

    #[test]
    fn play_11_continuation_requires_a_cleared_library_filter() {
        let unfiltered = PlayOrigin {
            place: BrowserPlace::from(ViewSource::Library),
            label: "Music".into(),
        };
        assert_eq!(
            random_library_continuation(
                Some(&unfiltered),
                &whole(&[1, 2, 3]),
                vec![3, 1, 2],
                Some(2)
            ),
            None,
            "an ordinary full-library snapshot still ends normally"
        );

        let mut filtered_place = BrowserPlace::from(ViewSource::Library);
        filtered_place.track_state_mut().unwrap().browse.genre = Some("Metal".into());
        let filtered = PlayOrigin {
            place: filtered_place,
            label: "Music".into(),
        };
        assert_eq!(
            random_library_continuation(Some(&filtered), &whole(&[2]), vec![3, 1, 2], Some(2)),
            None,
            "a filter that remains active must not loop or escape its hit set"
        );

        let mut playlist_place = BrowserPlace::from(ViewSource::Playlist(7));
        playlist_place.track_state_mut().unwrap().search = "needle".into();
        let playlist = PlayOrigin {
            place: playlist_place,
            label: "Mix".into(),
        };
        assert_eq!(
            random_library_continuation(
                Some(&playlist),
                &whole(&[1, 2, 3]),
                vec![3, 1, 2],
                Some(2)
            ),
            None,
            "clearing a playlist filter must not escape into the whole library"
        );

        let one_track = library_origin("only");
        assert_eq!(
            random_library_continuation(Some(&one_track), &whole(&[2]), vec![2], Some(2)),
            None,
            "the only library title must not repeat forever"
        );
    }

    #[test]
    fn play_11_clearing_the_filter_binds_the_continuation_while_the_title_still_plays() {
        let origin = library_origin("needle");

        assert_eq!(
            immediate_library_continuation(
                Some(&origin),
                Repeat::Off,
                0,
                Some(2),
                &whole(&[1, 2, 3]),
                vec![3, 1, 2],
            ),
            Some(vec![2, 3, 1]),
            "the running title stays at the head and the rest of the library follows it"
        );
    }

    #[test]
    fn play_11_bound_continuation_plays_every_title_once() {
        let origin = library_origin("needle");

        let ids = immediate_library_continuation(
            Some(&origin),
            Repeat::Off,
            0,
            Some(2),
            &whole(&[1, 2, 3]),
            vec![2, 3, 1],
        )
        .expect("an exhausted filtered snapshot in an unfiltered library continues");

        assert_eq!(ids.first(), Some(&2), "the running title keeps the cursor");
        assert_eq!(
            ids.iter().filter(|id| **id == 2).count(),
            1,
            "the running title must not be queued a second time behind itself"
        );
        assert_eq!(
            ids.len(),
            3,
            "every live library title is represented exactly once"
        );
    }

    #[test]
    fn play_11_continues_a_library_larger_than_the_visible_id_cap() {
        let origin = library_origin("needle");

        // What a >10,000-title library looks like here: the id query stopped
        // early, so `ids` is a strict subset of the live library while the
        // view's own row count still matches it exactly.
        let capped = VisibleView {
            ids: vec![1, 2],
            total: 3,
        };
        assert_eq!(
            random_library_continuation(Some(&origin), &capped, vec![2, 3, 1], Some(2)),
            Some(vec![3, 1, 2]),
            "an unfiltered library must continue even when its id list was capped"
        );
        assert_eq!(
            immediate_library_continuation(
                Some(&origin),
                Repeat::Off,
                0,
                Some(2),
                &capped,
                vec![3, 1, 2],
            ),
            Some(vec![2, 3, 1]),
            "the immediate binding must survive the cap the same way"
        );
    }

    #[test]
    fn play_11_a_capped_view_that_is_still_filtered_stays_shut() {
        let origin = library_origin("needle");

        // A filter that still matches more rows than the cap: the id list is
        // truncated exactly like the unfiltered one, so only the uncapped row
        // count can tell the two apart.
        let filtered = VisibleView {
            ids: vec![1, 2],
            total: 4,
        };
        assert_eq!(
            random_library_continuation(Some(&origin), &filtered, vec![2, 3, 1, 4, 5], Some(2)),
            None,
            "a view that still hides live titles must not escape into the library"
        );
        assert_eq!(
            immediate_library_continuation(
                Some(&origin),
                Repeat::Off,
                0,
                Some(2),
                &filtered,
                vec![2, 3, 1, 4, 5],
            ),
            None,
            "the immediate binding must respect the same narrowing"
        );

        // Rows the live library does not contain mean the view is not a
        // window onto it at all, however well the counts line up.
        let foreign = VisibleView {
            ids: vec![1, 9],
            total: 3,
        };
        assert_eq!(
            random_library_continuation(Some(&origin), &foreign, vec![2, 3, 1], Some(2)),
            None,
            "a visible id outside the live library keeps the gate shut"
        );
    }

    #[test]
    fn play_15_clearing_the_filter_rebinds_a_queue_with_a_future() {
        let origin = library_origin("needle");

        assert_eq!(
            rebind_to_unfiltered_view(
                Some(&origin),
                Repeat::Off,
                1,
                Some(2),
                &whole(&[1, 2, 3]),
                vec![3, 1, 2],
            ),
            Some((vec![1, 2, 3], 1)),
            "PLAY-15 replaces the filtered snapshot with the visible unfiltered library"
        );
    }

    #[test]
    fn play_11_immediate_continuation_never_overrides_repeat() {
        let origin = library_origin("needle");

        for repeat in [Repeat::All, Repeat::One] {
            assert_eq!(
                immediate_library_continuation(
                    Some(&origin),
                    repeat,
                    0,
                    Some(2),
                    &whole(&[1, 2, 3]),
                    vec![3, 1, 2],
                ),
                None,
                "Repeat One/All keep their existing queue behaviour"
            );
        }
    }

    #[test]
    fn play_11_immediate_continuation_keeps_every_stop_case_stopped() {
        let unfiltered = PlayOrigin {
            place: BrowserPlace::from(ViewSource::Library),
            label: "Music".into(),
        };
        assert_eq!(
            immediate_library_continuation(
                Some(&unfiltered),
                Repeat::Off,
                0,
                Some(2),
                &whole(&[1, 2, 3]),
                vec![3, 1, 2],
            ),
            None,
            "an ordinary unfiltered snapshot ends as before"
        );

        let mut playlist_place = BrowserPlace::from(ViewSource::Playlist(7));
        playlist_place.track_state_mut().unwrap().search = "needle".into();
        let playlist = PlayOrigin {
            place: playlist_place,
            label: "Mix".into(),
        };
        assert_eq!(
            immediate_library_continuation(
                Some(&playlist),
                Repeat::Off,
                0,
                Some(2),
                &whole(&[1, 2, 3]),
                vec![3, 1, 2],
            ),
            None,
            "clearing a playlist filter must not escape into the whole library"
        );

        let still_filtered = library_origin("needle");
        assert_eq!(
            immediate_library_continuation(
                Some(&still_filtered),
                Repeat::Off,
                0,
                Some(2),
                &whole(&[2]),
                vec![3, 1, 2],
            ),
            None,
            "a filter that is still narrowing the view must not escape its hit set"
        );

        let one_track = library_origin("only");
        assert_eq!(
            immediate_library_continuation(
                Some(&one_track),
                Repeat::Off,
                0,
                Some(2),
                &whole(&[2]),
                vec![2],
            ),
            None,
            "a library holding nothing but the running title has nothing to continue with"
        );

        assert_eq!(
            immediate_library_continuation(
                Some(&library_origin("needle")),
                Repeat::Off,
                0,
                None,
                &whole(&[1, 2, 3]),
                vec![3, 1, 2],
            ),
            None,
            "without a loaded title there is no cursor to continue from"
        );
    }
}
