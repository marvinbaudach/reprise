use reprise_core::models::Track;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub(super) enum PanelContext {
    Empty,
    Multiple(usize),
    Track(Track),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RequestIntent {
    pub generation: u64,
    pub artist: String,
    pub force: bool,
}

pub(super) struct PanelState {
    generation: u64,
    enabled: bool,
    context: PanelContext,
}

impl PanelState {
    pub(super) fn new(enabled: bool) -> Self {
        Self {
            generation: 0,
            enabled,
            context: PanelContext::Empty,
        }
    }

    #[cfg(test)]
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn context(&self) -> PanelContext {
        self.context.clone()
    }

    pub(super) fn set_context(&mut self, context: PanelContext) -> Option<RequestIntent> {
        self.context = context;
        self.advance_and_request(false)
    }

    pub(super) fn set_enabled(&mut self, enabled: bool) -> Option<RequestIntent> {
        self.enabled = enabled;
        self.advance_and_request(false)
    }

    pub(super) fn refresh(&mut self) -> Option<RequestIntent> {
        self.advance_and_request(true)
    }

    pub(super) fn accepts(&self, generation: u64) -> bool {
        self.enabled && self.generation == generation
    }

    fn advance_and_request(&mut self, force: bool) -> Option<RequestIntent> {
        self.generation = self.generation.wrapping_add(1);
        if !self.enabled {
            return None;
        }
        let PanelContext::Track(track) = &self.context else {
            return None;
        };
        let artist = track.artist.trim();
        if artist.is_empty() {
            return None;
        }
        Some(RequestIntent {
            generation: self.generation,
            artist: artist.to_string(),
            force,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::models::Track;

    fn track(artist: &str) -> Track {
        Track {
            id: 1,
            path: "/fixture.flac".into(),
            title: "Song".into(),
            artist: artist.into(),
            album: "Album".into(),
            album_artist: artist.into(),
            year: None,
            track_no: None,
            genre: String::new(),
            duration_ms: 0,
            bitrate_kbps: None,
            rating: 0,
            play_count: 0,
            last_played_at: None,
            added_at: 0,
            file_mtime: 0,
            missing: false,
            file_size: 0,
            device: None,
            inode: None,
            playlist_position: None,
        }
    }

    #[test]
    fn blank_artist_never_creates_a_request() {
        let mut state = PanelState::new(true);
        assert_eq!(state.set_context(PanelContext::Track(track("  "))), None);
    }

    #[test]
    fn context_disable_and_refresh_each_advance_generation() {
        let mut state = PanelState::new(true);
        let first = state
            .set_context(PanelContext::Track(track("Artist")))
            .unwrap();
        assert_eq!(first.generation, 1);
        state.set_enabled(false);
        assert_eq!(state.generation(), 2);
        assert_eq!(state.refresh(), None);
        assert_eq!(state.generation(), 3);
    }

    #[test]
    fn only_the_current_generation_may_apply() {
        let mut state = PanelState::new(true);
        let first = state
            .set_context(PanelContext::Track(track("Artist A")))
            .unwrap();
        let second = state
            .set_context(PanelContext::Track(track("Artist B")))
            .unwrap();
        assert!(!state.accepts(first.generation));
        assert!(state.accepts(second.generation));
    }
}
