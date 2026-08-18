//! External source artwork projection for the player bar and compact player.

use crate::ui::player_controller::PlayerController;

use super::external_media::ExternalPlaybackSnapshot;

impl PlayerController {
    pub(super) fn sync_external_artwork(&self, snapshot: Option<&ExternalPlaybackSnapshot>) {
        let Some(snapshot) = snapshot else {
            return;
        };
        let images_allowed = reprise_core::online_sources::network_allowed(
            &self.conn,
            &reprise_core::modules::ARTWORK_MODULE,
        )
        .unwrap_or(false);
        let startup_timing = if snapshot.restored {
            crate::ui::podcasts::source_image::StartupTiming::AfterQuiet
        } else {
            crate::ui::podcasts::source_image::StartupTiming::Immediate
        };

        let bar_generation = self.bar_cover_generation.get().wrapping_add(1);
        self.bar_cover_generation.set(bar_generation);
        let bar_size = self.bar.cover_image().pixel_size().max(1);
        crate::ui::podcasts::source_image::load_into_image(
            self.bar.cover_image(),
            crate::ui::podcasts::source_image::ArtworkRequest::new(
                snapshot.art_url.as_deref(),
                snapshot.fallback_art_url.as_deref(),
                (bar_size, bar_size),
                images_allowed,
                reprise_core::remote_image::CacheScope::Persistent,
                startup_timing,
            ),
            bar_generation,
            &self.bar_cover_generation,
        );

        let compact_generation = self.compact_cover_generation.get().wrapping_add(1);
        self.compact_cover_generation.set(compact_generation);
        let compact_cover = self.compact_player.cover_image();
        let previous_compact_paintable = compact_cover.paintable();
        let compact_size = compact_cover.pixel_size().max(1);
        crate::ui::podcasts::source_image::load_into_image(
            compact_cover,
            crate::ui::podcasts::source_image::ArtworkRequest::new(
                snapshot.art_url.as_deref(),
                snapshot.fallback_art_url.as_deref(),
                (compact_size, compact_size),
                images_allowed,
                reprise_core::remote_image::CacheScope::Persistent,
                startup_timing,
            ),
            compact_generation,
            &self.compact_cover_generation,
        );
        // The shared source-image loader clears its target before an async
        // cache miss. Compact mode has no crossfade, so keep the current cover
        // visible until the replacement arrives. A synchronous cache hit has
        // already installed its paintable and must not be overwritten here.
        if compact_cover.paintable().is_none() {
            if let Some(previous_compact_paintable) = previous_compact_paintable {
                compact_cover.set_paintable(Some(&previous_compact_paintable));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use std::sync::Arc;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use gtk4::prelude::*;
    use reprise_core::playback::{
        AudioEffects, PlaybackBackend, PlaybackError, PlaybackState, PlayerEvent,
    };
    use reprise_core::waveform::{RenderDataBackend, WaveformBackend, WaveformError};

    use super::*;
    use crate::ui::playback::external_media::{ExternalMedia, RadioPresentation, StreamTags};
    use crate::ui::playback::player_controller::PlayerControllerBackends;
    use crate::ui::playback::preview::PlaybackMode;
    use crate::ui::scrobble_runtime::ScrobbleRuntime;

    const INITIAL_COMPACT_RGB: [u8; 3] = [0x91; 3];
    const FIRST_SNAPSHOT_RGB: [u8; 3] = [0x16; 3];
    const SECOND_SNAPSHOT_RGB: [u8; 3] = [0xc7; 3];

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

    fn controller() -> Rc<PlayerController> {
        let conn = Rc::new(crate::test_db::open().unwrap());
        let app = libadwaita::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.ExternalArtworkTest")
            .build();
        let (_event_sender, playback_events) = async_channel::unbounded::<PlayerEvent>();
        let listenbrainz = ScrobbleRuntime::new(
            PathBuf::from("unused-listenbrainz-test.db"),
            reprise_core::scrobbling::ScrobbleProvider::ListenBrainz,
            "ListenBrainz",
        );
        let lastfm = ScrobbleRuntime::new(
            PathBuf::from("unused-lastfm-test.db"),
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

    fn snapshot(station_id: i64, name: &str, art_url: &str) -> ExternalPlaybackSnapshot {
        ExternalPlaybackSnapshot {
            mode: PlaybackMode::Radio,
            podcast_kind: None,
            media_category: None,
            media: ExternalMedia::Radio {
                station_id,
                name: name.into(),
                stream_url: format!("https://radio.test/{station_id}"),
                uuid: None,
            },
            art_url: Some(art_url.into()),
            fallback_art_url: None,
            can_go_previous: false,
            can_go_next: false,
            stream_tags: StreamTags::default(),
            podcast_phase: None,
            restored: false,
            radio: Some(RadioPresentation::connected()),
            error: None,
        }
    }

    fn texture(rgb: [u8; 3]) -> gtk4::gdk::Texture {
        let bytes = gtk4::glib::Bytes::from_owned(vec![rgb[0], rgb[1], rgb[2], 0xff]);
        gtk4::gdk::MemoryTexture::new(1, 1, gtk4::gdk::MemoryFormat::R8g8b8a8, &bytes, 4).upcast()
    }

    fn png_bytes(rgb: [u8; 3]) -> Vec<u8> {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("artwork.png");
        let pixbuf =
            gtk4::gdk_pixbuf::Pixbuf::new(gtk4::gdk_pixbuf::Colorspace::Rgb, true, 8, 2, 2)
                .unwrap();
        pixbuf.fill(u32::from_be_bytes([rgb[0], rgb[1], rgb[2], 0xff]));
        pixbuf.savev(&path, "png", &[]).unwrap();
        std::fs::read(path).unwrap()
    }

    fn cache_artwork(url: &str, rgb: [u8; 3]) -> PathBuf {
        let outcome = reprise_core::remote_image::resolve(
            Some(url),
            reprise_core::remote_image::CacheScope::Persistent,
            true,
            &mut |_| Ok(png_bytes(rgb)),
        );
        match outcome {
            reprise_core::remote_image::ImageOutcome::Fetched(path)
            | reprise_core::remote_image::ImageOutcome::Cached(path) => path,
            outcome => panic!("test artwork was not cached: {outcome:?}"),
        }
    }

    fn image_rgb(image: &gtk4::Image) -> Option<[u8; 3]> {
        let texture = image.paintable().and_downcast::<gtk4::gdk::Texture>()?;
        let stride = texture.width() as usize * 4;
        let mut pixels = vec![0; stride * texture.height() as usize];
        texture.download(&mut pixels, stride);
        Some([pixels[0], pixels[1], pixels[2]])
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn play_10_external_artwork_switch_keeps_compact_cover_and_latest_snapshot_wins() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let first_url = format!("https://images.test/play-10-first-{nonce}.png");
        let second_url = format!("https://images.test/play-10-second-{nonce}.png");
        let first_path = cache_artwork(&first_url, FIRST_SNAPSHOT_RGB);
        let second_path = cache_artwork(&second_url, SECOND_SNAPSHOT_RGB);
        let controller = controller();
        controller
            .compact_player
            .cover_image()
            .set_paintable(Some(&texture(INITIAL_COMPACT_RGB)));

        let first = snapshot(1, "First", &first_url);
        controller.sync_external_artwork(Some(&first));
        assert_eq!(
            image_rgb(controller.compact_player.cover_image()),
            Some(INITIAL_COMPACT_RGB),
            "the compact cover fell back to the placeholder while the first snapshot was loading"
        );

        let second = snapshot(2, "Second", &second_url);
        controller.sync_external_artwork(Some(&second));
        assert_eq!(
            image_rgb(controller.compact_player.cover_image()),
            Some(INITIAL_COMPACT_RGB),
            "the compact cover fell back to the placeholder while the second snapshot was loading"
        );

        let context = gtk4::glib::MainContext::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            while context.pending() {
                context.iteration(false);
            }
            let bar_rgb = image_rgb(controller.bar.cover_image());
            let compact_rgb = image_rgb(controller.compact_player.cover_image());
            assert!(
                compact_rgb.is_some(),
                "the compact cover fell back to the placeholder while the latest snapshot was loading"
            );
            if bar_rgb == Some(SECOND_SNAPSHOT_RGB) && compact_rgb == Some(SECOND_SNAPSHOT_RGB) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "both player artwork targets must end with the second snapshot; bar={bar_rgb:?}, compact={compact_rgb:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        std::fs::remove_file(first_path).ok();
        std::fs::remove_file(second_path).ok();
    }
}
