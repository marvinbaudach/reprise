//! Player-bar sensitivity derived from playback, queue, and library state.

use reprise_core::playback::PlaybackState;

use crate::ui::playback::external_media::{
    ExternalMedia, ExternalPlaybackSnapshot, PodcastPhase, RadioPresentation,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) enum BarProgressMode {
    #[default]
    Local,
    Streaming,
    Live,
}

pub(in crate::ui) fn external_progress_mode(media: &ExternalMedia) -> BarProgressMode {
    match media {
        ExternalMedia::Podcast { .. } => BarProgressMode::Streaming,
        ExternalMedia::Radio { .. } => BarProgressMode::Live,
    }
}

/// What the player bar means by "the same thing is still loaded". A snapshot
/// arrives on every phase change, retry and neighbour update, not only when a
/// new episode starts — so buffer state must be keyed to the media itself,
/// never to the arrival of a snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum ExternalMediaIdentity {
    Podcast(i64),
    Radio(i64),
}

pub(in crate::ui) fn external_media_identity(media: &ExternalMedia) -> ExternalMediaIdentity {
    match media {
        ExternalMedia::Podcast { episode_id, .. } => ExternalMediaIdentity::Podcast(*episode_id),
        ExternalMedia::Radio { station_id, .. } => ExternalMediaIdentity::Radio(*station_id),
    }
}

/// The duration the bar should measure a buffer against when `media` becomes
/// the loaded item. `None` means "not known yet" and must clear whatever the
/// previous item left behind: measuring a new episode's buffer against the
/// previous one's length is how a fresh stream reports itself as most of the
/// way loaded. The first real position tick supplies the true value.
pub(in crate::ui) fn external_seed_duration_ms(media: &ExternalMedia) -> i64 {
    match media {
        ExternalMedia::Podcast { duration_ms, .. } => duration_ms.unwrap_or(0).max(0),
        // Live radio has no length to measure against, and shows no buffer.
        ExternalMedia::Radio { .. } => 0,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) struct ExternalBarDisplay {
    pub(in crate::ui) title: String,
    pub(in crate::ui) subtitle: String,
    pub(in crate::ui) playback: PlaybackState,
    pub(in crate::ui) progress_mode: BarProgressMode,
    pub(in crate::ui) title_dimmed: bool,
    pub(in crate::ui) inline_error: Option<String>,
}

pub(in crate::ui) fn external_bar_display(
    snapshot: &ExternalPlaybackSnapshot,
) -> ExternalBarDisplay {
    let progress_mode = external_progress_mode(&snapshot.media);
    match &snapshot.media {
        ExternalMedia::Podcast { title, show, .. } => ExternalBarDisplay {
            title: title.clone(),
            subtitle: show.clone(),
            playback: match snapshot.podcast_phase {
                Some(PodcastPhase::Paused) => PlaybackState::Paused,
                Some(PodcastPhase::Failed) => PlaybackState::Stopped,
                _ => PlaybackState::Playing,
            },
            progress_mode,
            title_dimmed: false,
            inline_error: snapshot.error.clone(),
        },
        ExternalMedia::Radio { name, .. } => radio_display(
            name,
            snapshot.radio.as_ref(),
            snapshot.error.clone(),
            progress_mode,
        ),
    }
}

fn radio_display(
    name: &str,
    presentation: Option<&RadioPresentation>,
    error: Option<String>,
    progress_mode: BarProgressMode,
) -> ExternalBarDisplay {
    let phase = presentation.map(RadioPresentation::phase);
    let title = presentation
        .and_then(RadioPresentation::last_title)
        .unwrap_or(name)
        .to_owned();
    ExternalBarDisplay {
        title,
        subtitle: error
            .as_ref()
            .map_or_else(|| name.to_owned(), |message| format!("{name} · {message}")),
        playback: match phase {
            Some(crate::ui::playback::external_media::RadioPhase::Paused) => PlaybackState::Paused,
            _ => PlaybackState::Playing,
        },
        progress_mode,
        title_dimmed: matches!(
            phase,
            Some(crate::ui::playback::external_media::RadioPhase::Paused)
        ),
        inline_error: error,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct BufferingPresentation {
    pub(in crate::ui) buffered_fraction: f64,
    pub(in crate::ui) loaded_percent: u8,
    pub(in crate::ui) show_status: bool,
}

/// Turns a backend buffering event into player-bar geometry and copy. The
/// backend's ring-buffer percentage is deliberately not used here: “loaded”
/// describes the share of the media duration covered by `buffered_ms`.
pub(in crate::ui) fn buffering_presentation(
    event: &reprise_core::playback::PlayerEvent,
    mode: BarProgressMode,
    duration_ms: i64,
) -> Option<BufferingPresentation> {
    let reprise_core::playback::PlayerEvent::Buffering { buffered_ms, .. } = event else {
        return None;
    };
    if mode != BarProgressMode::Streaming || duration_ms <= 0 {
        return None;
    }
    let buffered_ms = (*buffered_ms)?.clamp(0, duration_ms);
    let buffered_fraction = buffered_ms as f64 / duration_ms as f64;
    // Truncate rather than round: rounding lets 99.6 % report "100 % loaded"
    // while the caption is still on screen, which contradicts its own
    // presence. Truncation can only reach 100 when the media really is
    // complete — and then `show_status` has already taken the caption away.
    let loaded_percent = (buffered_fraction * 100.0) as u8;
    Some(BufferingPresentation {
        buffered_fraction,
        loaded_percent,
        show_status: buffered_ms < duration_ms,
    })
}

pub(in crate::ui) fn live_badge_should_pulse(
    mode: BarProgressMode,
    playback: PlaybackState,
    animations_enabled: bool,
) -> bool {
    mode == BarProgressMode::Live && playback == PlaybackState::Playing && animations_enabled
}

pub(in crate::ui) fn format_live_elapsed(elapsed_ms: i64) -> String {
    reprise_core::format::format_duration(elapsed_ms.max(0))
}

pub(in crate::ui) fn bar_should_be_sensitive(
    state: PlaybackState,
    queue_has_tracks: bool,
    library_has_tracks: bool,
    play_next_available: bool,
) -> bool {
    state != PlaybackState::Stopped || queue_has_tracks || library_has_tracks || play_next_available
}

pub(in crate::ui) fn waveform_should_be_sensitive(
    state: PlaybackState,
    seek_enabled: bool,
    has_loaded_length: bool,
) -> bool {
    seek_enabled && (state != PlaybackState::Stopped || has_loaded_length)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakePlaybackEvents;

    impl FakePlaybackEvents {
        fn buffering(percent: u8, buffered_ms: i64) -> reprise_core::playback::PlayerEvent {
            reprise_core::playback::PlayerEvent::Buffering {
                percent,
                buffered_ms: Some(buffered_ms),
            }
        }
    }

    #[test]
    fn streaming_buffering_event_reaches_the_bar_and_produces_a_segment() {
        let event = FakePlaybackEvents::buffering(61, 45_000);

        let presentation = buffering_presentation(&event, BarProgressMode::Streaming, 90_000)
            .expect("a streaming buffering event should reach the player bar");

        assert_eq!(presentation.buffered_fraction, 0.5);
        assert_eq!(presentation.loaded_percent, 50);
        assert!(presentation.show_status);
    }

    fn podcast(episode_id: i64, duration_ms: Option<i64>) -> ExternalMedia {
        ExternalMedia::Podcast {
            episode_id,
            title: "Episode".into(),
            show: "Show".into(),
            source: crate::ui::playback::external_media::EpisodeSource::Url("https://e".into()),
            resume_ms: 0,
            duration_ms,
        }
    }

    /// A snapshot arrives whenever anything about the external session changes
    /// — pausing alone produces one. Keying the buffer reset to the snapshot
    /// rather than to the media means pausing a stream wipes its buffer
    /// display, and nothing re-sends it: the backend only speaks when the
    /// buffer *changes*, and a finished download never changes again.
    #[test]
    fn pausing_the_same_episode_is_not_a_reason_to_forget_its_buffer() {
        let playing = podcast(7, Some(600_000));
        let paused = podcast(7, Some(600_000));
        assert_eq!(
            external_media_identity(&playing),
            external_media_identity(&paused),
            "the same episode stays the same episode across a phase change"
        );

        let next_episode = podcast(8, Some(600_000));
        assert_ne!(
            external_media_identity(&playing),
            external_media_identity(&next_episode)
        );

        let station = ExternalMedia::Radio {
            station_id: 7,
            name: "Station".into(),
            stream_url: "https://s".into(),
            uuid: None,
        };
        assert_ne!(
            external_media_identity(&playing),
            external_media_identity(&station),
            "an episode and a station that share a row id are not the same thing"
        );
    }

    /// Buffering messages for a new stream arrive before its first position
    /// tick. Carrying the previous item's duration over means the new stream's
    /// buffer is measured against the wrong length — a 90-minute episode
    /// following a 10-minute one reads as fully loaded from the first message.
    #[test]
    fn a_new_episode_never_measures_its_buffer_against_the_old_one_s_length() {
        assert_eq!(
            external_seed_duration_ms(&podcast(8, Some(5_400_000))),
            5_400_000
        );
        assert_eq!(
            external_seed_duration_ms(&podcast(8, None)),
            0,
            "an unknown length clears the previous one instead of inheriting it"
        );
        assert_eq!(
            external_seed_duration_ms(&podcast(8, Some(-1))),
            0,
            "a nonsense length is no length"
        );

        // A zero duration is exactly what makes the presentation stay silent
        // until the first genuine position tick supplies the real length.
        assert!(buffering_presentation(
            &FakePlaybackEvents::buffering(50, 30_000),
            BarProgressMode::Streaming,
            0,
        )
        .is_none());
    }

    /// The caption may only say 100 % when the media really is complete, and
    /// at that point it is gone. Rounding would let 99.6 % read as "100 %
    /// loaded" while the line is still on screen — a number that contradicts
    /// its own presence.
    #[test]
    fn a_nearly_loaded_stream_never_claims_to_be_finished() {
        let almost = buffering_presentation(
            &FakePlaybackEvents::buffering(100, 99_600),
            BarProgressMode::Streaming,
            100_000,
        )
        .expect("a partially buffered stream has a presentation");
        assert_eq!(almost.loaded_percent, 99);
        assert!(almost.show_status, "it is not finished, so it still speaks");

        let complete = buffering_presentation(
            &FakePlaybackEvents::buffering(100, 100_000),
            BarProgressMode::Streaming,
            100_000,
        )
        .expect("a fully buffered stream still has a segment");
        assert_eq!(complete.loaded_percent, 100);
        assert!(
            !complete.show_status,
            "at 100 % the caption goes away rather than standing there"
        );
    }

    #[test]
    fn mot_5_live_pulse_requires_live_playback_and_enabled_motion() {
        assert!(live_badge_should_pulse(
            BarProgressMode::Live,
            PlaybackState::Playing,
            true
        ));
        assert!(!live_badge_should_pulse(
            BarProgressMode::Live,
            PlaybackState::Paused,
            true
        ));
        assert!(!live_badge_should_pulse(
            BarProgressMode::Live,
            PlaybackState::Playing,
            false
        ));
        assert!(!live_badge_should_pulse(
            BarProgressMode::Streaming,
            PlaybackState::Playing,
            true
        ));
    }

    #[test]
    fn play_9_stopped_bar_is_enabled_when_the_library_can_start() {
        assert!(bar_should_be_sensitive(
            PlaybackState::Stopped,
            false,
            true,
            false
        ));
        assert!(!bar_should_be_sensitive(
            PlaybackState::Stopped,
            false,
            false,
            false
        ));
    }

    #[test]
    fn stopped_waveform_with_a_loaded_length_is_seekable() {
        assert!(waveform_should_be_sensitive(
            PlaybackState::Stopped,
            true,
            true
        ));
    }

    #[test]
    fn stopped_waveform_without_a_loaded_length_is_not_seekable() {
        assert!(!waveform_should_be_sensitive(
            PlaybackState::Stopped,
            true,
            false
        ));
    }

    #[test]
    fn playing_waveform_does_not_wait_for_a_length() {
        assert!(waveform_should_be_sensitive(
            PlaybackState::Playing,
            true,
            false
        ));
    }

    #[test]
    fn disabled_seeking_keeps_the_waveform_inert_in_every_state() {
        for state in [
            PlaybackState::Stopped,
            PlaybackState::Paused,
            PlaybackState::Playing,
        ] {
            assert!(!waveform_should_be_sensitive(state, false, true));
        }
    }

    #[test]
    fn stopped_bar_remains_enabled_for_a_podcast_play_next_offer() {
        assert!(bar_should_be_sensitive(
            PlaybackState::Stopped,
            false,
            false,
            true
        ));
    }

    #[test]
    fn rad_2_pause_keeps_last_title_dimmed_and_never_empty() {
        let mut radio = RadioPresentation::connected();
        radio.on_stream_title(Some("Last song".into()));
        radio.pause();

        let display = radio_display(
            "Station",
            Some(&radio),
            Some("Offline".into()),
            BarProgressMode::Live,
        );

        assert_eq!(display.title, "Last song");
        assert_eq!(display.playback, PlaybackState::Paused);
        assert_eq!(display.progress_mode, BarProgressMode::Live);
        assert!(display.title_dimmed);
        assert_eq!(display.inline_error.as_deref(), Some("Offline"));
    }

    #[test]
    fn rad_2_live_elapsed_has_no_duration_component() {
        assert_eq!(format_live_elapsed(65_000), "1:05");
    }

    #[test]
    fn play_13_the_source_selects_one_of_three_progress_languages() {
        let podcast = ExternalMedia::Podcast {
            episode_id: 7,
            title: "Episode".into(),
            show: "Show".into(),
            source: crate::ui::playback::external_media::EpisodeSource::Url(
                "https://example.test/episode.mp3".into(),
            ),
            resume_ms: 0,
            duration_ms: None,
        };
        let radio = ExternalMedia::Radio {
            station_id: 9,
            name: "Station".into(),
            stream_url: "https://radio.test/live".into(),
            uuid: None,
        };

        assert_eq!(BarProgressMode::default(), BarProgressMode::Local);
        assert_eq!(external_progress_mode(&podcast), BarProgressMode::Streaming);
        assert_eq!(external_progress_mode(&radio), BarProgressMode::Live);
    }
}
