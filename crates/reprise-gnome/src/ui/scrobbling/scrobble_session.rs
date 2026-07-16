//! Pure per-track scrobble lifecycle owned by `PlayerController`.

use reprise_core::scrobbling::{self, Listen, TrackMetadata};

#[derive(Default)]
pub(in crate::ui) struct ScrobbleSession {
    listen: Option<Listen>,
}

impl ScrobbleSession {
    pub(in crate::ui) fn begin(&mut self, track: TrackMetadata, listened_at: i64) {
        self.listen = Some(Listen {
            id: None,
            listened_at,
            track,
        });
    }

    /// Ends the current session regardless of outcome. Taking the listen
    /// before checking the threshold makes repeated stop/error callbacks
    /// idempotent by construction.
    pub(in crate::ui) fn finish(&mut self, max_position_ms: i64, enabled: bool) -> Option<Listen> {
        let listen = self.listen.take()?;
        if !enabled || !scrobbling::should_scrobble(max_position_ms, listen.track.duration_ms) {
            return None;
        }
        Some(listen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> reprise_core::scrobbling::TrackMetadata {
        reprise_core::scrobbling::TrackMetadata {
            artist_name: "Portishead".to_string(),
            track_name: "Roads".to_string(),
            release_name: Some("Dummy".to_string()),
            duration_ms: 300_000,
        }
    }

    #[test]
    fn completion_below_threshold_returns_nothing_and_clears_session() {
        let mut session = ScrobbleSession::default();
        session.begin(metadata(), 1_700_000_000);
        assert!(session.finish(149_999, true).is_none());
        assert!(session.finish(300_000, true).is_none());
    }

    #[test]
    fn completion_at_threshold_returns_exactly_one_listen() {
        let mut session = ScrobbleSession::default();
        session.begin(metadata(), 1_700_000_000);
        let listen = session.finish(150_000, true).unwrap();
        assert_eq!(listen.listened_at, 1_700_000_000);
        assert_eq!(listen.track, metadata());
        assert!(session.finish(300_000, true).is_none());
    }

    #[test]
    fn disabling_before_completion_discards_the_session() {
        let mut session = ScrobbleSession::default();
        session.begin(metadata(), 1_700_000_000);
        assert!(session.finish(300_000, false).is_none());
        assert!(session.finish(300_000, true).is_none());
    }

    #[test]
    fn starting_a_new_session_replaces_unfinished_metadata() {
        let mut session = ScrobbleSession::default();
        session.begin(metadata(), 1);
        let mut replacement = metadata();
        replacement.track_name = "Glory Box".to_string();
        session.begin(replacement.clone(), 2);
        let listen = session.finish(150_000, true).unwrap();
        assert_eq!(listen.listened_at, 2);
        assert_eq!(listen.track, replacement);
    }
}
