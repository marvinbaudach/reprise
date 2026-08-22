//! NAV-17: the track list's selection anchor.
//!
//! GTK's `GtkListBase` keeps an internal anchor that only a click or focus
//! movement sets. NAV-10b forbids playback from doing either, so GTK's anchor
//! stays behind when playback starts -- at row zero after a view change -- and
//! Shift+click stretches across half the library. This module therefore keeps
//! the anchor itself.

use crate::ui::table_selection;

pub(super) type Anchored = table_selection::Anchored<i64>;
pub(super) type AnchorState = table_selection::AnchorState<i64>;
pub(super) use table_selection::{resolve, validate, SelectMode, SelectionOp};

use super::track_list::Shared;

/// Reads the stored anchor and discards anything made stale by sorting,
/// filtering, or reloading. Every read goes through this function, keeping
/// invalidation in one place instead of attaching it to every model rebuild.
pub(super) fn live_anchor_state(shared: &Shared) -> AnchorState {
    let state = validate(shared.selection_anchor.get(), |position| {
        shared.model.track_at(position).map(|track| track.id)
    });
    shared.selection_anchor.set(state);
    state
}

pub(super) fn store_anchor_state(shared: &Shared, state: AnchorState) {
    shared.selection_anchor.set(state);
}

pub(super) fn anchored_at(shared: &Shared, position: u32) -> Option<Anchored> {
    shared.model.track_at(position).map(|track| Anchored {
        position,
        id: track.id,
    })
}

/// Resolves the playing track as a fallback anchor at input time and never
/// stores it. That preserves NAV-10b: playback writes no selection state and
/// therefore cannot move the anchor behind the user's back.
pub(super) fn playing_anchor(shared: &Shared) -> Option<Anchored> {
    let track_id = shared.playing_track_id.get()?;
    let ids = shared.current_view_ids();
    let is_queue = matches!(
        *shared.source.borrow(),
        reprise_core::view_source::ViewSource::Queue
    );
    let position = super::current_track_selection::visible_position_for_track_in_source(
        &ids, track_id, None, is_queue,
    )?;
    Some(Anchored {
        position,
        id: track_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_17_a_reload_drops_a_stale_anchor_against_the_real_model() {
        gtk4::init().unwrap();
        let conn = crate::test_db::open().unwrap();
        let fixture_conn = crate::test_db::connection(&conn);
        let tx = fixture_conn.unchecked_transaction().unwrap();
        for id in 1..=20 {
            tx.execute(
                "INSERT INTO tracks (id, path, title, artist, added_at) \
                 VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
                (
                    id,
                    format!("/synthetic/{id:03}.flac"),
                    format!("Track {id:03}"),
                ),
            )
            .unwrap();
        }
        tx.commit().unwrap();
        let track_list = crate::ui::track_list::TrackList::new(
            std::rc::Rc::new(conn),
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            crate::ui::track_list::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        );
        let shared = &track_list.shared;

        let real = anchored_at(shared, 3).expect("row 3 exists");
        store_anchor_state(
            shared,
            AnchorState {
                anchor: Some(real),
                cursor: Some(real),
            },
        );
        assert_eq!(live_anchor_state(shared).anchor, Some(real));

        // The position exists, but now claims an id that is not at that row.
        let stale = Anchored {
            position: 3,
            id: real.id + 5_000,
        };
        store_anchor_state(
            shared,
            AnchorState {
                anchor: Some(stale),
                cursor: Some(stale),
            },
        );
        assert_eq!(
            live_anchor_state(shared),
            AnchorState::default(),
            "an anchor whose row carries another track is discarded"
        );
    }
}
