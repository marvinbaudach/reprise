use reprise_core::playback::PlaybackState;
use reprise_core::queue::Repeat;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CompactPresentation {
    pub(super) title: String,
    pub(super) artist: String,
    pub(super) album: String,
    pub(super) year: Option<i32>,
    pub(super) state: PlaybackState,
    pub(super) position_ms: i64,
    pub(super) duration_ms: i64,
    pub(super) transport_enabled: bool,
    pub(super) shuffled: bool,
    pub(super) repeat: Repeat,
    pub(super) volume_percent: u8,
}

impl Default for CompactPresentation {
    fn default() -> Self {
        Self {
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            year: None,
            state: PlaybackState::Stopped,
            position_ms: 0,
            duration_ms: 0,
            transport_enabled: false,
            shuffled: false,
            repeat: Repeat::Off,
            volume_percent: 100,
        }
    }
}

impl CompactPresentation {
    pub(super) fn set_playback_state(&mut self, state: PlaybackState) {
        self.state = state;
        if state == PlaybackState::Stopped {
            self.position_ms = 0;
            self.duration_ms = 0;
        }
    }

    pub(super) fn clear_track(&mut self) {
        self.title.clear();
        self.artist.clear();
        self.album.clear();
        self.year = None;
        self.position_ms = 0;
        self.duration_ms = 0;
    }
}

pub(super) fn normalized_position(position_ms: i64, duration_ms: i64) -> (i64, i64) {
    let duration_ms = duration_ms.max(0);
    (position_ms.clamp(0, duration_ms), duration_ms)
}

pub(super) fn volume_percent(volume: f64) -> u8 {
    if !volume.is_finite() {
        return 100;
    }
    (volume.clamp(0.0, 1.0) * 100.0).round() as u8
}

#[cfg(test)]
mod tests {
    use reprise_core::playback::PlaybackState;
    use reprise_core::queue::Repeat;

    use super::*;

    #[test]
    fn default_presentation_is_stopped_empty_and_full_volume() {
        let state = CompactPresentation::default();

        assert_eq!(state.title, "");
        assert_eq!(state.artist, "");
        assert_eq!(state.album, "");
        assert_eq!(state.year, None);
        assert_eq!(state.state, PlaybackState::Stopped);
        assert_eq!(state.position_ms, 0);
        assert_eq!(state.duration_ms, 0);
        assert!(!state.transport_enabled);
        assert!(!state.shuffled);
        assert_eq!(state.repeat, Repeat::Off);
        assert_eq!(state.volume_percent, 100);
    }

    #[test]
    fn position_is_clamped_to_zero_and_duration() {
        assert_eq!(normalized_position(-10, 1_000), (0, 1_000));
        assert_eq!(normalized_position(2_000, 1_000), (1_000, 1_000));
        assert_eq!(normalized_position(50, -1), (0, 0));
    }

    #[test]
    fn stopped_state_resets_position_and_duration() {
        let mut state = CompactPresentation {
            position_ms: 900,
            duration_ms: 1_000,
            ..CompactPresentation::default()
        };

        state.set_playback_state(PlaybackState::Stopped);

        assert_eq!(state.state, PlaybackState::Stopped);
        assert_eq!(state.position_ms, 0);
        assert_eq!(state.duration_ms, 0);
    }

    #[test]
    fn volume_percent_clamps_invalid_backend_values() {
        assert_eq!(volume_percent(-1.0), 0);
        assert_eq!(volume_percent(0.42), 42);
        assert_eq!(volume_percent(2.0), 100);
        assert_eq!(volume_percent(f64::NAN), 100);
    }

    #[test]
    fn clearing_track_removes_metadata_and_resets_time() {
        let mut state = CompactPresentation {
            title: "Track".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            year: Some(2026),
            position_ms: 500,
            duration_ms: 1_000,
            ..CompactPresentation::default()
        };

        state.clear_track();

        assert_eq!(state.title, "");
        assert_eq!(state.artist, "");
        assert_eq!(state.album, "");
        assert_eq!(state.year, None);
        assert_eq!(state.position_ms, 0);
        assert_eq!(state.duration_ms, 0);
    }
}
