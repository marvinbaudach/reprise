use super::info_panel_state::PanelContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) struct RequestFeedback {
    pub(in crate::ui) refresh_sensitive: bool,
    pub(in crate::ui) progress_visible: bool,
}

pub(in crate::ui) fn request_feedback(
    enabled: bool,
    context: &PanelContext,
    loading: bool,
) -> RequestFeedback {
    let has_artist = matches!(
        context,
        PanelContext::Track(track) if !track.artist.trim().is_empty()
    );
    RequestFeedback {
        refresh_sensitive: enabled && has_artist && !loading,
        progress_visible: loading,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::info_panel_state::PanelContext;
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
            missing_since: None,
            missing_reason: None,
            untagged: false,
            file_size: 0,
            device: None,
            inode: None,
            playlist_position: None,
        }
    }

    #[test]
    fn refresh_is_available_only_for_an_enabled_single_artist() {
        assert!(
            !request_feedback(false, &PanelContext::Track(track("Artist")), false)
                .refresh_sensitive
        );
        assert!(!request_feedback(true, &PanelContext::Multiple(2), false).refresh_sensitive);
        assert!(
            !request_feedback(true, &PanelContext::Track(track("  ")), false).refresh_sensitive
        );
        assert!(
            request_feedback(true, &PanelContext::Track(track("Artist")), false).refresh_sensitive
        );
    }

    #[test]
    fn pending_request_has_indeterminate_progress_and_blocks_duplicate_refresh() {
        let feedback = request_feedback(true, &PanelContext::Track(track("Artist")), true);
        assert!(feedback.progress_visible);
        assert!(!feedback.refresh_sensitive);
    }
}
