use super::*;

use std::time::Duration;

const TINY_PNG: [u8; 69] = [
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xA8, 0xAF, 0xAF, 0x07,
    0x00, 0x02, 0xFE, 0x01, 0x7E, 0xBA, 0x25, 0x70, 0x25, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
    0x44, 0xAE, 0x42, 0x60, 0x82,
];

pub(super) fn external_episode_snapshot(
) -> crate::ui::playback::external_media::ExternalPlaybackSnapshot {
    use crate::ui::playback::external_media::{
        EpisodeSource, ExternalMedia, ExternalPlaybackSnapshot, PodcastPhase, StreamTags,
    };
    use crate::ui::playback::preview::PlaybackMode;

    ExternalPlaybackSnapshot {
        mode: PlaybackMode::Podcast,
        podcast_kind: Some(reprise_core::podcasts::PodcastKind::Rss),
        media: ExternalMedia::Podcast {
            episode_id: 42,
            title: "External episode".into(),
            show: "External show".into(),
            source: EpisodeSource::Url("https://example.test/episode.mp3".into()),
            resume_ms: 0,
            duration_ms: Some(180_000),
        },
        art_url: Some("https://images.test/show.jpg".into()),
        can_go_previous: true,
        can_go_next: false,
        stream_tags: StreamTags::default(),
        podcast_phase: Some(PodcastPhase::Playing),
        restored: false,
        radio: None,
        error: None,
    }
}

pub(super) fn external_radio_snapshot(
) -> crate::ui::playback::external_media::ExternalPlaybackSnapshot {
    use crate::ui::playback::external_media::{
        ExternalMedia, ExternalPlaybackSnapshot, RadioPresentation, StreamTags,
    };
    use crate::ui::playback::preview::PlaybackMode;

    ExternalPlaybackSnapshot {
        mode: PlaybackMode::Radio,
        podcast_kind: None,
        media: ExternalMedia::Radio {
            station_id: 7,
            name: "External radio".into(),
            stream_url: "https://radio.test/live".into(),
            uuid: None,
        },
        art_url: Some("https://images.test/radio.jpg".into()),
        can_go_previous: false,
        can_go_next: false,
        stream_tags: StreamTags::default(),
        podcast_phase: None,
        restored: false,
        radio: Some(RadioPresentation::connected()),
        error: None,
    }
}

pub(super) fn external_youtube_snapshot(
) -> crate::ui::playback::external_media::ExternalPlaybackSnapshot {
    let mut snapshot = external_episode_snapshot();
    snapshot.podcast_kind = Some(reprise_core::podcasts::PodcastKind::Youtube);
    if let crate::ui::playback::external_media::ExternalMedia::Podcast { title, source, .. } =
        &mut snapshot.media
    {
        *title = "YouTube episode".into();
        *source = crate::ui::playback::external_media::EpisodeSource::Url(
            "https://youtube.test/watch?v=42".into(),
        );
    }
    snapshot
}

/// `PLAY-12`: the panel's cover, title, artist and album are links, and
/// `render_track` makes them operable exactly while `idle` is false.
#[test]
fn play_12_only_an_empty_panel_switches_its_link_surfaces_off() {
    let track = super::tests::loaded_track();

    assert!(panel_presentation_with_external(None, None, PlaybackState::Stopped).idle);
    assert!(
        !panel_presentation_with_external(Some(&track), None, PlaybackState::Paused).idle,
        "a loaded track keeps the panel's links operable"
    );
    for snapshot in [external_episode_snapshot(), external_radio_snapshot()] {
        assert!(
            !panel_presentation_with_external(None, Some(&snapshot), PlaybackState::Playing).idle,
            "an external session keeps the panel's links operable"
        );
    }
}

#[test]
fn pod_21_external_episode_uses_the_shared_bar_identity_instead_of_idle_copy() {
    let snapshot = external_episode_snapshot();
    let presentation =
        panel_presentation_with_external(None, Some(&snapshot), PlaybackState::Playing);

    assert_eq!(presentation.title, "External episode");
    assert_eq!(presentation.subtitle, "External show");
    assert!(!presentation.idle);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pod_21_external_header_uses_episode_identity_and_source_tile() {
    gtk4::init().unwrap();
    let (_window, panel) = super::tests::test_panel("org.reprise.Reprise.ExternalPanelHeaderTest");
    panel.set_external_snapshot(Some(external_episode_snapshot()));

    assert_eq!(panel.widgets.title.text(), "External episode");
    assert_eq!(panel.widgets.artist.text(), "External show");
    assert!(panel.widgets.album.text().is_empty());
    assert_eq!(
        panel.widgets.cover_stack.visible_child_name().as_deref(),
        Some("external")
    );
    assert!(panel.widgets.external_cover.first_child().is_some());
    assert!(!panel
        .widgets
        .stage
        .has_css_class("reprise-now-playing-idle"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pod_21_lyrics_falls_back_and_stays_hidden_for_podcast_youtube_and_radio() {
    gtk4::init().unwrap();
    let (_window, panel) = super::tests::test_panel("org.reprise.Reprise.ExternalLyricsTest");

    for snapshot in [
        external_episode_snapshot(),
        external_youtube_snapshot(),
        external_radio_snapshot(),
    ] {
        panel.widgets.tab_stack.set_visible_child_name(LYRICS_PAGE);
        assert_eq!(panel.widgets.session.selected.get(), PanelTab::Lyrics);
        panel.set_external_snapshot(Some(snapshot));

        assert!(!panel.widgets.lyrics_page.is_visible());
        assert_eq!(panel.widgets.session.selected.get(), PanelTab::UpNext);
        assert_eq!(
            panel.widgets.tab_stack.visible_child_name().as_deref(),
            Some(UP_NEXT_PAGE)
        );
        assert_eq!(
            panel.widgets.footer.text(),
            panel.widgets.footers.borrow().up_next
        );

        panel.set_external_snapshot(None);
        assert!(panel.widgets.lyrics_page.is_visible());
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_26_youtube_keeps_the_visual_page_while_an_rss_podcast_hides_it() {
    gtk4::init().unwrap();
    let (_window, panel) =
        super::tests::test_panel("org.reprise.Reprise.ExternalVisualVisibilityTest");
    panel.set_song_visuals_enabled(true);

    panel.set_external_snapshot(Some(external_youtube_snapshot()));
    assert!(panel.widgets.visual_page.is_visible());

    panel.set_external_snapshot(Some(external_episode_snapshot()));
    assert!(!panel.widgets.visual_page.is_visible());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_26_an_rss_podcast_moves_the_selected_visual_tab_to_up_next() {
    gtk4::init().unwrap();
    let (_window, panel) =
        super::tests::test_panel("org.reprise.Reprise.ExternalVisualFallbackTest");
    panel.set_song_visuals_enabled(true);
    panel.widgets.tab_stack.set_visible_child_name(VISUAL_PAGE);

    panel.set_external_snapshot(Some(external_episode_snapshot()));

    assert_eq!(panel.widgets.session.selected.get(), PanelTab::UpNext);
    assert_eq!(
        panel.widgets.tab_stack.visible_child_name().as_deref(),
        Some(UP_NEXT_PAGE)
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_26_a_late_empty_track_update_keeps_an_external_session_loaded() {
    gtk4::init().unwrap();
    let (_window, panel) =
        super::tests::test_panel("org.reprise.Reprise.ExternalVisualizerTrackTest");
    panel.set_external_snapshot(Some(external_youtube_snapshot()));

    panel.set_loaded_track(None);

    assert!(panel.widgets.visualizer.reports_track_for_test());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_26_youtube_artwork_drives_the_bloom_while_an_rss_podcast_has_none() {
    gtk4::init().unwrap();
    let url = "https://images.test/ac-26-youtube-bloom.png";
    let outcome =
        reprise_core::remote_image::resolve(Some(url), true, &mut |_| Ok(TINY_PNG.to_vec()));
    assert!(matches!(
        outcome,
        reprise_core::remote_image::ImageOutcome::Fetched(_)
            | reprise_core::remote_image::ImageOutcome::Cached(_)
    ));
    let (_window, panel) = super::tests::test_panel("org.reprise.Reprise.ExternalBloomTest");

    let mut youtube = external_youtube_snapshot();
    youtube.art_url = Some(url.into());
    panel.set_external_snapshot(Some(youtube));

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !panel.widgets.bloom.has_cover_for_test() {
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(
            std::time::Instant::now() < deadline,
            "cached YouTube artwork did not reach the cover bloom"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    let mut podcast = external_episode_snapshot();
    podcast.art_url = Some(url.into());
    panel.set_external_snapshot(Some(podcast));
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert!(!panel.widgets.bloom.has_cover_for_test());

    let stale_url = "https://images.test/ac-26-stale-youtube-bloom.png";
    reprise_core::remote_image::resolve(Some(stale_url), true, &mut |_| Ok(TINY_PNG.to_vec()));
    let mut stale_youtube = external_youtube_snapshot();
    stale_youtube.art_url = Some(stale_url.into());
    panel.set_external_snapshot(Some(stale_youtube));
    panel.set_external_snapshot(Some(external_episode_snapshot()));
    crate::ui::test_settle::settle_for(Duration::from_millis(150));

    assert!(
        !panel.widgets.bloom.has_cover_for_test(),
        "a late YouTube decode must not repaint a newer podcast session"
    );
}
