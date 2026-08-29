//! Queue-context decisions for externally requested track playback.
//!
//! `agent_playback_queue` is the pure decision: a single track inherits the
//! flat library when the snapshot contains it, otherwise the explicit
//! context stays unchanged. `resolve_agent_playback_queue` builds that
//! library snapshot by reading the persisted session and sticky Library
//! filters from the database, then delegates to the pure decision.

use crate::ui::browse_bar::EXCLUDE_AI_KEY;

/// Chooses the immutable playback snapshot for an external track request.
/// A single track inherits the flat library when that snapshot contains it;
/// every other request keeps its explicit context unchanged.
pub(super) fn agent_playback_queue(
    requested_ids: Vec<i64>,
    library_ids: Vec<i64>,
) -> (Vec<i64>, usize) {
    let [requested] = requested_ids.as_slice() else {
        return (requested_ids, 0);
    };
    match library_ids.iter().position(|id| id == requested) {
        Some(index) => (library_ids, index),
        None => (requested_ids, 0),
    }
}

/// Resolves a single external track request against the flat Library snapshot
/// described by the persisted session and sticky Library filters. Any request
/// that is not exactly one id short-circuits and keeps its explicit context
/// unchanged, `(requested_ids, 0)`.
pub(super) fn resolve_agent_playback_queue(
    db: &reprise_core::db::Db,
    requested_ids: Vec<i64>,
) -> (Vec<i64>, usize) {
    if requested_ids.len() != 1 {
        return (requested_ids, 0);
    }

    let persisted = reprise_core::library::session::load(db);
    let sort =
        crate::ui::track_list_sort::restored_sort(&persisted.sort_field, &persisted.sort_dir);
    let exclude_ai =
        reprise_core::library::settings::get_bool(db, EXCLUDE_AI_KEY, false).unwrap_or(false);
    match reprise_core::queries::query_track_ids_browsed_ai(
        db,
        &reprise_core::view_source::ViewSource::Library,
        &sort.field,
        &sort.dir,
        "",
        &reprise_core::queries::BrowseFilter::default(),
        &[],
        exclude_ai,
    ) {
        Ok(library_ids) => {
            if reprise_core::queries::is_queue_capped(library_ids.len()) {
                tracing::warn!(
                    limit = reprise_core::queries::QUEUE_LIMIT,
                    "queue capped at {} tracks",
                    reprise_core::queries::QUEUE_LIMIT
                );
            }
            agent_playback_queue(requested_ids, library_ids)
        }
        Err(error) => {
            tracing::error!(
                %error,
                "failed to build library queue for MPRIS play; falling back to a single-track queue"
            );
            (requested_ids, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    use reprise_core::media_integration::MprisCommand;
    use reprise_core::playback::{AudioEffects, PlaybackBackend, PlaybackError, PlaybackState};

    use crate::ui::playback::test_support::controller_with_db;

    struct TestPlayback;

    impl PlaybackBackend for TestPlayback {
        fn play(&self, _: &str) -> Result<(), PlaybackError> {
            Ok(())
        }

        fn play_uri(&self, _: &str) -> Result<(), PlaybackError> {
            Ok(())
        }

        fn toggle_pause(&self) -> Result<PlaybackState, PlaybackError> {
            Ok(PlaybackState::Paused)
        }

        fn seek_to(&self, _: i64) -> Result<(), PlaybackError> {
            Ok(())
        }

        fn set_volume(&self, _: f64) {}

        fn set_audio_effects(&self, _: AudioEffects) -> Result<(), PlaybackError> {
            Ok(())
        }

        fn stop(&self) -> Result<(), PlaybackError> {
            Ok(())
        }

        fn set_next(&self, _: Option<&str>) {}

        fn set_transition(&self, _: reprise_core::library::settings::TrackTransition, _: u8) {}
    }

    #[test]
    fn play_2_agent_single_track_inherits_the_library_at_its_real_index() {
        assert_eq!(
            agent_playback_queue(vec![20], vec![30, 20, 10]),
            (vec![30, 20, 10], 1)
        );
    }

    #[test]
    fn play_2_agent_track_absent_from_the_library_keeps_single_track_context() {
        assert_eq!(agent_playback_queue(vec![20], vec![30, 10]), (vec![20], 0));
    }

    #[test]
    fn play_2_agent_explicit_multi_track_context_stays_verbatim() {
        assert_eq!(
            agent_playback_queue(vec![20, 10], vec![30, 20, 10]),
            (vec![20, 10], 0)
        );
    }

    #[test]
    fn play_2_agent_empty_context_stays_empty() {
        assert_eq!(
            agent_playback_queue(Vec::new(), vec![30, 20, 10]),
            (Vec::new(), 0)
        );
    }

    #[test]
    fn play_2_agent_resolves_persisted_sort_and_ai_exclusion_from_the_database() {
        let conn = crate::test_db::open().unwrap();
        crate::test_db::connection(&conn)
            .execute_batch(
                "INSERT INTO tracks (id, path, title, artist, added_at) VALUES
                    (10, '/music/alpha.flac', 'Alpha', 'Artist', 0),
                    (20, '/music/bravo.flac', 'Bravo', 'Artist', 0),
                    (30, '/music/charlie.flac', 'Charlie', 'Artist', 0),
                    (40, '/music/delta.flac', 'Delta', 'Artist', 0);
                 INSERT INTO track_provenance (track_id, kind, ai, created_at)
                    VALUES (40, 'vocals-removed', 1, 0);",
            )
            .unwrap();
        reprise_core::library::session::save(
            &conn,
            &reprise_core::library::session::SessionState {
                sort_field: "title".into(),
                sort_dir: "desc".into(),
                ..Default::default()
            },
        )
        .unwrap();
        reprise_core::library::settings::set_bool(&conn, EXCLUDE_AI_KEY, true).unwrap();

        assert_eq!(
            resolve_agent_playback_queue(&conn, vec![20]),
            (vec![30, 20, 10], 1)
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn play_2_agent_single_track_handler_seeds_the_session_sorted_library() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let test_root = tempfile::tempdir().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        crate::test_db::connection(&conn)
            .execute_batch(
                "INSERT INTO tracks (id, path, title, artist, added_at) VALUES
                    (10, '/music/alpha.flac', 'Alpha', 'Artist', 0),
                    (20, '/music/bravo.flac', 'Bravo', 'Artist', 0),
                    (30, '/music/charlie.flac', 'Charlie', 'Artist', 0);",
            )
            .unwrap();
        let session = reprise_core::library::session::SessionState {
            sort_field: "title".into(),
            sort_dir: "desc".into(),
            ..Default::default()
        };
        reprise_core::library::session::save(&conn, &session).unwrap();
        let controller = controller_with_db(test_root.path(), conn, Box::new(TestPlayback));

        controller.handle_mpris_command(MprisCommand::PlayTrackIds(vec![20]));

        let queue = controller.queue.borrow();
        assert_eq!(queue.current(), Some(20));
        assert_eq!(queue.ids_in_order(), vec![30, 20, 10]);
        assert!(queue.ids_in_order().len() > 1);
    }
}
