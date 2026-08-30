//! `AC-26`: what happens when a category arrives for a running session.
//!
//! The state layer's own guards are covered next to it; what is proven here is
//! the part a unit test of the guard cannot see — that a category which earns
//! the music treatment also reaches the spectrum source. A tab that appears
//! over flat bars passes every assertion about visibility and is still broken.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use reprise_core::playback::{AudioEffects, PlaybackBackend, PlaybackError, PlaybackState};
use reprise_core::podcasts::feed::ParsedEpisode;
use reprise_core::podcasts::store::{
    add_or_restore, save_youtube_resolution, upsert_episode, NewSubscription,
};
use reprise_core::podcasts::PodcastKind;

use crate::ui::playback::external_media::{EpisodeSource, ExternalMedia};
use crate::ui::playback::player_controller::PlayerController;

/// Records every spectrum switch the controller pushes at the source.
#[derive(Default)]
struct SpectrumRecordingPlayback {
    switches: Rc<RefCell<Vec<bool>>>,
}

impl PlaybackBackend for SpectrumRecordingPlayback {
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

    fn set_spectrum_enabled(&self, enabled: bool) -> Result<(), PlaybackError> {
        self.switches.borrow_mut().push(enabled);
        Ok(())
    }

    fn stop(&self) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn set_next(&self, _: Option<&str>) {}

    fn set_transition(&self, _: reprise_core::library::settings::TrackTransition, _: u8) {}
}

/// A YouTube episode carrying `category`, played from a local file.
///
/// The file source is what matters: an episode still to be downloaded takes
/// the download path and learns its category there. This is the other case —
/// the one that would otherwise stay unclassified forever.
fn downloaded_youtube_episode(db: &reprise_core::db::Db, category: Option<&str>) -> i64 {
    let subscription_id = add_or_restore(
        db,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://youtube.test/channel".to_owned(),
            title: "Channel".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        10,
    )
    .unwrap();
    let episode_id = upsert_episode(
        db,
        subscription_id,
        &ParsedEpisode {
            guid: "video-1".to_owned(),
            title: "Video".to_owned(),
            image_url: None,
            audio_url: "https://youtube.test/watch?v=1".to_owned(),
            page_url: None,
            published_at: Some(20),
            duration_secs: Some(120),
        },
        20,
    )
    .unwrap()
    .unwrap()
    .episode_id;
    if let Some(category) = category {
        save_youtube_resolution(db, episode_id, None, Some(category)).unwrap();
    }
    episode_id
}

fn playing_youtube_session(
    category: Option<&str>,
) -> (Rc<PlayerController>, i64, Rc<RefCell<Vec<bool>>>) {
    let db = Rc::new(crate::test_db::open().unwrap());
    let episode_id = downloaded_youtube_episode(&db, category);
    let playback = SpectrumRecordingPlayback::default();
    let switches = playback.switches.clone();
    let controller = crate::ui::playback::test_support::controller_with_db(
        &PathBuf::from("unused-classify-test"),
        db,
        Box::new(playback),
    );
    controller.set_song_visuals_enabled(true).unwrap();
    controller
        .play_external(ExternalMedia::Podcast {
            episode_id,
            title: "Video".to_owned(),
            show: "Channel".to_owned(),
            source: EpisodeSource::File("/nonexistent/video.opus".to_owned()),
            resume_ms: 0,
            duration_ms: Some(120_000),
        })
        .unwrap();
    (controller, episode_id, switches)
}

fn spectrum_is_on(switches: &Rc<RefCell<Vec<bool>>>) -> bool {
    switches.borrow().last().copied().unwrap_or(false)
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_26_a_resolved_music_category_switches_the_spectrum_on() {
    if gtk4::init().is_err() {
        return;
    }
    let (controller, episode_id, switches) = playing_youtube_session(Some("Education"));
    assert!(
        !spectrum_is_on(&switches),
        "an `Education` episode runs no spectrum"
    );
    let generation = controller.external.borrow().generation;

    controller.apply_resolved_category(generation, episode_id, Some("Music".to_owned()));

    assert!(
        spectrum_is_on(&switches),
        "the resolved category has to reach the source, not only the tab"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_26_a_resolved_category_from_a_left_session_moves_nothing() {
    if gtk4::init().is_err() {
        return;
    }
    let (controller, episode_id, switches) = playing_youtube_session(Some("Education"));
    let generation = controller.external.borrow().generation;

    controller.apply_resolved_category(generation + 1, episode_id, Some("Music".to_owned()));
    assert!(
        !spectrum_is_on(&switches),
        "an answer for a session the user has left changes nothing"
    );

    controller.apply_resolved_category(generation, episode_id + 1, Some("Music".to_owned()));
    assert!(
        !spectrum_is_on(&switches),
        "an answer for another episode changes nothing"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_26_a_resolved_speech_category_leaves_the_spectrum_off() {
    if gtk4::init().is_err() {
        return;
    }
    let (controller, episode_id, switches) = playing_youtube_session(None);
    let generation = controller.external.borrow().generation;

    controller.apply_resolved_category(
        generation,
        episode_id,
        Some("News & Politics".to_owned()),
    );

    assert!(
        !spectrum_is_on(&switches),
        "classifying an episode as speech must not hand it a spectrum"
    );
}
