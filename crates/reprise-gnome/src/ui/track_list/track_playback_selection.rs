//! Pure selection policy for track-list playback actions.
//!
//! Playlist and Queue views can display missing rows. Context-menu playback
//! actions must therefore resolve the live model rows, preserve the user's
//! selection order, and remove missing tracks before anything reaches the
//! player. The resulting emptiness is also the single source of truth for
//! action sensitivity.

use crate::ui::track_list_model::TrackListModel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) struct PlayableSelection {
    ids: Vec<i64>,
}

impl PlayableSelection {
    pub(in crate::ui) fn ids(&self) -> &[i64] {
        &self.ids
    }

    pub(in crate::ui) fn enqueue_enabled(&self) -> bool {
        !self.ids.is_empty()
    }
}

pub(in crate::ui) fn selected_playable_tracks(
    positions: &[u32],
    model: &TrackListModel,
) -> PlayableSelection {
    let ids = positions
        .iter()
        .filter_map(|&position| {
            let track = model.track_at(position);
            if track.is_none() {
                tracing::warn!(
                    position,
                    "context menu: no track at selected position; skipping"
                );
            }
            track
        })
        .filter(|track| !track.is_missing())
        .map(|track| track.id)
        .collect();
    PlayableSelection { ids }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use rusqlite::params;

    use super::*;
    use crate::ui::track_list_activation::missing_activation_notice;
    use reprise_core::library::playlists;
    use reprise_core::view_source::ViewSource;

    fn seeded_playlist_model_with_unmounted_track() -> TrackListModel {
        let conn = Rc::new(RefCell::new(reprise_core::db::open(None).unwrap()));
        reprise_core::db::migrate(&conn.borrow()).unwrap();
        for id in 1..=3 {
            conn.borrow()
                .execute(
                    "INSERT INTO tracks (id, path, title, artist, added_at) \
                     VALUES (?1, ?2, ?3, 'Artist', 0)",
                    params![id, format!("/x/{id}.flac"), format!("Track {id}")],
                )
                .unwrap();
        }
        conn.borrow()
            .execute(
                "UPDATE tracks SET missing_since = 1000000000, \
                 missing_reason = 'unmounted' WHERE id = 2",
                [],
            )
            .unwrap();
        let playlist_id = playlists::create(&conn.borrow(), "P1").unwrap();
        playlists::add_tracks(&mut conn.borrow_mut(), playlist_id, &[1, 2, 3]).unwrap();

        let model = TrackListModel::new(conn);
        model.set_query(
            &ViewSource::Playlist(playlist_id),
            "playlist_order",
            "asc",
            "",
            &[],
        );
        model
    }

    #[test]
    fn play_4b_missing_rows_explain_and_enqueue_only_playable_tracks() {
        let model = seeded_playlist_model_with_unmounted_track();

        let mixed = selected_playable_tracks(&[0, 1, 2], &model);
        assert!(mixed.enqueue_enabled());
        assert_eq!(mixed.ids(), &[1, 3]);

        let missing_only = selected_playable_tracks(&[1], &model);
        assert!(!missing_only.enqueue_enabled());
        assert!(missing_only.ids().is_empty());

        let notice = missing_activation_notice(&model.track_at(1).unwrap())
            .expect("a concrete missing row must explain instead of playing");
        assert_eq!(
            notice.message,
            "On unavailable drive — returns when mounted"
        );
        assert_eq!(notice.button_label, "Show in Missing files");
        assert_eq!(notice.target, ViewSource::Missing);
        assert!(
            missing_activation_notice(&model.track_at(0).unwrap()).is_none(),
            "a playable row must continue to normal playback"
        );
    }
}
