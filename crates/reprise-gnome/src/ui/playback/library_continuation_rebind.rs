//! PLAY-15: rebinding a running filtered-library snapshot when its refinement
//! is completely cleared.

use reprise_core::queue::Repeat;

use super::cleared_library_filter_handoff;
use crate::ui::playback::play_origin::PlayOrigin;
use crate::ui::player_controller::VisibleView;

/// Rebuild a running filtered-library snapshot from the now-unfiltered view.
///
/// The live ids are independent evidence for the shared PLAY-11/PLAY-15 row
/// count and membership gate. The returned ids come back in the visible
/// view's order with the running title's index and the ordinary queue cap.
/// `Queue::set_tracks` then keeps that order at `pos = start_index`, or, when
/// shuffle is on, reshuffles behind the running title at `pos = 0`.
pub(super) fn rebind_to_unfiltered_view(
    origin: Option<&PlayOrigin>,
    repeat: Repeat,
    remaining: usize,
    current_track_id: Option<i64>,
    visible: &VisibleView,
    live_ids: Vec<i64>,
) -> Option<(Vec<i64>, usize)> {
    if repeat != Repeat::Off || remaining == 0 {
        return None;
    }
    let current_track_id = current_track_id?;
    cleared_library_filter_handoff(origin, visible, live_ids)?;
    let current_position = visible.ids.iter().position(|id| *id == current_track_id)?;
    Some((visible.ids.clone(), current_position))
}

pub(super) fn rebind_live_count_matches_visible(visible: &VisibleView, live_count: i64) -> bool {
    usize::try_from(live_count).is_ok_and(|count| count == visible.total)
}

/// The origin stored after PLAY-15 takes over. An unfiltered Library origin is
/// also the loop guard: the reload caused by the queue notification cannot
/// satisfy `cleared_filter_origin` again.
pub(super) fn rebound_library_origin() -> PlayOrigin {
    PlayOrigin::library()
}

#[cfg(test)]
mod tests {
    use reprise_core::browser::BrowserPlace;
    use reprise_core::queries::QUEUE_LIMIT;
    use reprise_core::queue::Repeat;
    use reprise_core::view_source::ViewSource;

    use super::{
        rebind_live_count_matches_visible, rebind_to_unfiltered_view, rebound_library_origin,
    };
    use crate::ui::playback::library_continuation::cleared_filter_origin;
    use crate::ui::playback::play_origin::PlayOrigin;
    use crate::ui::player_controller::VisibleView;

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
    fn play_15_random_live_ids_are_only_needed_when_counts_match() {
        let visible = whole(&[1, 2, 3]);

        assert!(rebind_live_count_matches_visible(&visible, 3));
        assert!(!rebind_live_count_matches_visible(&visible, 4));
    }

    #[test]
    fn play_15_clearing_the_filter_rebinds_a_snapshot_with_a_future() {
        let origin = library_origin("needle");
        let live_ids = vec![3, 1, 2];

        assert_eq!(
            rebind_to_unfiltered_view(
                Some(&origin),
                Repeat::Off,
                1,
                Some(2),
                &whole(&[1, 2, 3]),
                live_ids,
            ),
            Some((vec![1, 2, 3], 1)),
            "the unfiltered visible order replaces a filtered snapshot without restarting its cursor"
        );
    }

    #[test]
    fn play_15_rebind_requires_a_cleared_library_filter() {
        let mut playlist_place = BrowserPlace::from(ViewSource::Playlist(7));
        playlist_place.track_state_mut().unwrap().search = "needle".into();
        let playlist = PlayOrigin {
            place: playlist_place,
            label: "Mix".into(),
        };
        let mut album_place = BrowserPlace::from(ViewSource::Album {
            album: "Blue".into(),
            album_artist: "Joni Mitchell".into(),
        });
        album_place.track_state_mut().unwrap().search = "needle".into();
        let album = PlayOrigin {
            place: album_place,
            label: "Blue".into(),
        };

        for origin in [&playlist, &album] {
            assert_eq!(
                rebind_to_unfiltered_view(
                    Some(origin),
                    Repeat::Off,
                    1,
                    Some(2),
                    &whole(&[1, 2, 3]),
                    vec![3, 1, 2],
                ),
                None,
                "only a snapshot born in a filtered Music root may be rebound"
            );
        }
    }

    #[test]
    fn play_15_rebind_stays_shut_on_a_capped_view_that_is_still_filtered() {
        let origin = library_origin("needle");
        let visible_ids = (1..=QUEUE_LIMIT).collect::<Vec<_>>();
        let visible = VisibleView {
            ids: visible_ids,
            total: (QUEUE_LIMIT + 1) as usize,
        };
        let live_ids = (1..=QUEUE_LIMIT + 2).collect::<Vec<_>>();

        assert_eq!(
            rebind_to_unfiltered_view(Some(&origin), Repeat::Off, 1, Some(2), &visible, live_ids,),
            None,
            "the uncapped row count must keep a still-filtered capped view shut"
        );
    }

    #[test]
    fn play_15_rebind_never_overrides_repeat() {
        let origin = library_origin("needle");

        for repeat in [Repeat::One, Repeat::All] {
            assert_eq!(
                rebind_to_unfiltered_view(
                    Some(&origin),
                    repeat,
                    1,
                    Some(2),
                    &whole(&[1, 2, 3]),
                    vec![3, 1, 2],
                ),
                None,
                "Repeat One/All retain their existing queue behavior"
            );
        }
    }

    #[test]
    fn play_15_rebind_keeps_the_running_title_at_the_cursor() {
        let origin = library_origin("needle");
        let (ids, cursor) = rebind_to_unfiltered_view(
            Some(&origin),
            Repeat::Off,
            2,
            Some(7),
            &whole(&[4, 7, 9, 12]),
            vec![12, 4, 9, 7],
        )
        .expect("a cleared filtered-library snapshot with a future is rebound");

        assert_eq!(ids, vec![4, 7, 9, 12]);
        assert_eq!(cursor, 1);
        assert_eq!(ids[cursor], 7);
        assert_eq!(ids.iter().filter(|id| **id == 7).count(), 1);
    }

    #[test]
    fn play_15_rebind_of_a_library_larger_than_the_visible_id_cap_queues_the_cap() {
        let origin = library_origin("needle");
        let visible_ids = (1..=QUEUE_LIMIT).collect::<Vec<_>>();
        let visible = VisibleView {
            ids: visible_ids.clone(),
            total: (QUEUE_LIMIT + 1) as usize,
        };
        let live_ids = (1..=QUEUE_LIMIT + 1).rev().collect::<Vec<_>>();

        assert_eq!(
            rebind_to_unfiltered_view(
                Some(&origin),
                Repeat::Off,
                1,
                Some(9_999),
                &visible,
                live_ids,
            ),
            Some((visible_ids, 9_998)),
            "the visible 10,000-row prefix is the queue even when the live library is larger"
        );
    }

    #[test]
    fn play_15_rebind_needs_the_running_title_to_be_in_the_new_list() {
        let origin = library_origin("needle");

        assert_eq!(
            rebind_to_unfiltered_view(
                Some(&origin),
                Repeat::Off,
                1,
                Some(7),
                &whole(&[1, 2, 3]),
                vec![3, 1, 2],
            ),
            None,
            "a rebind without its running title cannot preserve the cursor"
        );
    }

    #[test]
    fn play_15_a_rebind_rewrites_the_origin_so_the_reload_it_causes_cannot_rebind_again() {
        let origin = rebound_library_origin();

        assert!(origin.place.is_library_root());
        assert!(
            !cleared_filter_origin(&origin),
            "the origin written by a rebind must not look filtered to its own reload"
        );
    }
}
