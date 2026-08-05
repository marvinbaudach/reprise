//! Player-bar sensitivity derived from playback, queue, and library state.

use reprise_core::playback::PlaybackState;

use crate::ui::playback::external_media::{
    ExternalMedia, ExternalPlaybackSnapshot, PodcastPhase, RadioPresentation,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum BarProgressMode {
    Local,
    Streaming,
    Live,
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
    match &snapshot.media {
        ExternalMedia::Podcast { title, show, .. } => ExternalBarDisplay {
            title: title.clone(),
            subtitle: show.clone(),
            playback: match snapshot.podcast_phase {
                Some(PodcastPhase::Paused) => PlaybackState::Paused,
                Some(PodcastPhase::Failed) => PlaybackState::Stopped,
                _ => PlaybackState::Playing,
            },
            progress_mode: BarProgressMode::Streaming,
            title_dimmed: false,
            inline_error: snapshot.error.clone(),
        },
        ExternalMedia::Radio { name, .. } => {
            radio_display(name, snapshot.radio.as_ref(), snapshot.error.clone())
        }
    }
}

fn radio_display(
    name: &str,
    presentation: Option<&RadioPresentation>,
    error: Option<String>,
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
        progress_mode: BarProgressMode::Live,
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
    let loaded_percent = (buffered_fraction * 100.0).round() as u8;
    Some(BufferingPresentation {
        buffered_fraction,
        loaded_percent,
        show_status: buffered_ms < duration_ms,
    })
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

        let display = radio_display("Station", Some(&radio), Some("Offline".into()));

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
}
