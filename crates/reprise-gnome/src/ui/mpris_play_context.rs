//! Pure queue-context decision for externally requested track playback.

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::rc::Rc;
    use std::sync::Arc;

    use reprise_core::media_integration::MprisCommand;
    use reprise_core::playback::{
        AudioEffects, PlaybackBackend, PlaybackError, PlaybackState, PlayerEvent,
    };
    use reprise_core::waveform::{RenderDataBackend, WaveformBackend, WaveformError};

    use crate::ui::playback::player_controller::{PlayerController, PlayerControllerBackends};
    use crate::ui::scrobble_runtime::ScrobbleRuntime;

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

    struct TestWaveform;

    impl WaveformBackend for TestWaveform {
        fn extract_peaks(&self, _: &Path, buckets: usize) -> Result<Vec<u8>, WaveformError> {
            Ok(vec![0; buckets])
        }
    }

    impl RenderDataBackend for TestWaveform {}

    fn controller_with_db(
        test_root: &Path,
        conn: Rc<reprise_core::db::Db>,
    ) -> Rc<PlayerController> {
        let app = libadwaita::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.AgentQueueTest")
            .build();
        let (_event_sender, playback_events) = async_channel::unbounded::<PlayerEvent>();
        let listenbrainz = ScrobbleRuntime::new(
            test_root.join("listenbrainz.db"),
            reprise_core::scrobbling::ScrobbleProvider::ListenBrainz,
            "ListenBrainz",
        );
        let lastfm = ScrobbleRuntime::new(
            test_root.join("lastfm.db"),
            reprise_core::scrobbling::ScrobbleProvider::LastFm,
            "Last.fm",
        );
        PlayerController::new(
            conn,
            crate::ui::cover_download_worker::setup_for_test(),
            listenbrainz,
            lastfm,
            PlayerControllerBackends {
                playback: Box::new(TestPlayback),
                playback_events,
                media: reprise_core::media_integration::MediaIntegrationHandles::inert(),
                waveform: Arc::new(TestWaveform),
            },
            &app,
        )
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
        let controller = controller_with_db(test_root.path(), conn);

        controller.handle_mpris_command(MprisCommand::PlayTrackIds(vec![20]));

        let queue = controller.queue.borrow();
        assert_eq!(queue.current(), Some(20));
        assert_eq!(queue.ids_in_order(), vec![30, 20, 10]);
        assert!(queue.ids_in_order().len() > 1);
    }
}
