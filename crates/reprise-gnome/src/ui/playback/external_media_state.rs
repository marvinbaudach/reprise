//! Pure state for podcast and radio playback.
//!
//! Wave 2 source views consume these state types. E2 intentionally lands the
//! complete state seam before those callers.
#![allow(dead_code)]

use std::rc::Rc;

use reprise_core::podcasts::{EpisodeRow, PodcastKind};

use super::preview::PlaybackMode;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) enum EpisodeSource {
    Url(String),
    File(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) enum ExternalMedia {
    Podcast {
        episode_id: i64,
        title: String,
        show: String,
        source: EpisodeSource,
        resume_ms: i64,
        duration_ms: Option<i64>,
    },
    Radio {
        station_id: i64,
        name: String,
        stream_url: String,
        uuid: Option<String>,
    },
}

pub(super) fn podcast_source_requires_resolution(
    kind: PodcastKind,
    source: &EpisodeSource,
) -> bool {
    kind == PodcastKind::Youtube && matches!(source, EpisodeSource::Url(_))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) struct StreamTags {
    pub(in crate::ui) title: Option<String>,
    pub(in crate::ui) organization: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum PodcastPhase {
    Resolving,
    Playing,
    Paused,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PodcastSession {
    pub(super) media: ExternalMedia,
    pub(super) subscription_id: i64,
    pub(super) published_at: Option<i64>,
    pub(super) art_url: Option<String>,
    pub(super) phase: PodcastPhase,
    pub(super) resume: ResumePolicy,
    pub(super) position_ms: i64,
    pub(super) last_persisted_ms: i64,
    pub(super) duration_known: bool,
    pub(super) error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RadioSession {
    pub(super) media: ExternalMedia,
    pub(super) art_url: Option<String>,
    pub(super) presentation: RadioPresentation,
    pub(super) retry_guard: reprise_core::radio::click::ReresolveGuard,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ExternalSession {
    Podcast(PodcastSession),
    Radio(RadioSession),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) struct ExternalPlaybackSnapshot {
    pub(in crate::ui) mode: PlaybackMode,
    pub(in crate::ui) media: ExternalMedia,
    pub(in crate::ui) stream_tags: StreamTags,
    pub(in crate::ui) podcast_phase: Option<PodcastPhase>,
    pub(in crate::ui) radio: Option<RadioPresentation>,
    pub(in crate::ui) error: Option<String>,
}

type StreamTagsCallback = Rc<dyn Fn(StreamTags)>;
type ExternalChangedCallback = Rc<dyn Fn(Option<ExternalPlaybackSnapshot>)>;
type PlayNextCallback = Rc<dyn Fn(EpisodeRow)>;

#[derive(Default)]
pub(in crate::ui) struct ExternalPlaybackState {
    pub(super) preview_path: Option<String>,
    pub(super) session: Option<ExternalSession>,
    pub(super) stream_tags: StreamTags,
    pub(super) generation: u64,
    pub(super) play_next: Option<EpisodeRow>,
    pub(super) stream_tags_callbacks: Vec<StreamTagsCallback>,
    pub(super) changed_callbacks: Vec<ExternalChangedCallback>,
    pub(super) play_next_callbacks: Vec<PlayNextCallback>,
}

impl ExternalPlaybackState {
    pub(in crate::ui) fn mode(&self) -> PlaybackMode {
        match self.session {
            Some(ExternalSession::Podcast(_)) => PlaybackMode::Podcast,
            Some(ExternalSession::Radio(_)) => PlaybackMode::Radio,
            None if self.preview_path.is_some() => PlaybackMode::Preview,
            None => PlaybackMode::Queue,
        }
    }

    pub(in crate::ui) fn preview_path(&self) -> Option<String> {
        self.preview_path.clone()
    }

    pub(in crate::ui) fn begin_preview(&mut self, path: String) {
        self.session = None;
        self.preview_path = Some(path);
        self.play_next = None;
        self.bump_generation();
    }

    pub(in crate::ui) fn clear_preview(&mut self) {
        self.preview_path = None;
    }

    pub(super) fn begin_session(&mut self, session: ExternalSession) -> u64 {
        self.preview_path = None;
        self.session = Some(session);
        self.stream_tags = StreamTags::default();
        self.play_next = None;
        self.bump_generation()
    }

    fn bump_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }

    pub(super) fn clear_session(&mut self) {
        self.session = None;
        self.stream_tags = StreamTags::default();
        self.bump_generation();
    }

    pub(super) fn invalidate_pending(&mut self) -> u64 {
        self.bump_generation()
    }

    pub(super) fn snapshot(&self) -> Option<ExternalPlaybackSnapshot> {
        match self.session.as_ref()? {
            ExternalSession::Podcast(session) => Some(ExternalPlaybackSnapshot {
                mode: PlaybackMode::Podcast,
                media: session.media.clone(),
                stream_tags: self.stream_tags.clone(),
                podcast_phase: Some(session.phase),
                radio: None,
                error: session.error.clone(),
            }),
            ExternalSession::Radio(session) => Some(ExternalPlaybackSnapshot {
                mode: PlaybackMode::Radio,
                media: session.media.clone(),
                stream_tags: self.stream_tags.clone(),
                podcast_phase: None,
                radio: Some(session.presentation.clone()),
                error: session.presentation.inline_error.clone(),
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct ResumePolicy {
    pending_ms: Option<i64>,
}

impl ResumePolicy {
    pub(in crate::ui) fn new(resume_ms: i64) -> Self {
        Self {
            pending_ms: (resume_ms > 0).then_some(resume_ms),
        }
    }

    pub(in crate::ui) fn initial_seek_finished(&mut self, succeeded: bool) {
        if succeeded {
            self.pending_ms = None;
        }
    }

    pub(in crate::ui) fn position_tick(&mut self, duration_ms: i64) -> Option<i64> {
        (duration_ms > 0).then(|| self.pending_ms.take()).flatten()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum RadioPhase {
    Connected,
    Paused,
    Reconnecting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum RadioCommand {
    Disconnect,
    Reconnect,
    Stop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) struct RadioPresentation {
    pub(super) phase: RadioPhase,
    pub(super) last_title: Option<String>,
    pub(super) inline_error: Option<String>,
}

impl RadioPresentation {
    pub(in crate::ui) fn connected() -> Self {
        Self {
            phase: RadioPhase::Connected,
            last_title: None,
            inline_error: None,
        }
    }

    pub(in crate::ui) fn phase(&self) -> RadioPhase {
        self.phase
    }

    pub(in crate::ui) fn last_title(&self) -> Option<&str> {
        self.last_title.as_deref()
    }

    pub(in crate::ui) fn inline_error(&self) -> Option<&str> {
        self.inline_error.as_deref()
    }

    pub(in crate::ui) fn table_now_playing(&self) -> Option<&str> {
        (self.phase == RadioPhase::Connected)
            .then(|| self.last_title())
            .flatten()
    }

    pub(in crate::ui) fn accepts_stream_tags(&self) -> bool {
        self.phase == RadioPhase::Connected
    }

    pub(in crate::ui) fn is_empty(&self) -> bool {
        false
    }

    pub(in crate::ui) fn on_stream_title(&mut self, title: Option<String>) {
        if let Some(title) = title.filter(|title| !title.trim().is_empty()) {
            self.last_title = Some(title);
        }
    }

    pub(in crate::ui) fn pause(&mut self) -> Option<RadioCommand> {
        if self.phase == RadioPhase::Paused {
            return None;
        }
        self.phase = RadioPhase::Paused;
        self.inline_error = None;
        Some(RadioCommand::Disconnect)
    }

    pub(in crate::ui) fn play(&mut self) -> Option<RadioCommand> {
        if self.phase != RadioPhase::Paused {
            return None;
        }
        self.phase = RadioPhase::Reconnecting;
        self.inline_error = None;
        Some(RadioCommand::Reconnect)
    }

    pub(in crate::ui) fn reconnect_succeeded(&mut self) {
        self.phase = RadioPhase::Connected;
        self.inline_error = None;
    }

    pub(in crate::ui) fn reconnect_failed(&mut self, message: String) {
        self.phase = RadioPhase::Paused;
        self.inline_error = Some(message);
    }

    pub(in crate::ui) fn activation(&self) -> RadioCommand {
        if self.phase == RadioPhase::Connected {
            RadioCommand::Stop
        } else {
            RadioCommand::Reconnect
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_early_resume_is_retried_once_after_duration_arrives() {
        let mut resume = ResumePolicy::new(42_000);
        resume.initial_seek_finished(false);

        assert_eq!(resume.position_tick(0), None);
        assert_eq!(resume.position_tick(180_000), Some(42_000));
        assert_eq!(resume.position_tick(180_000), None);
    }

    #[test]
    fn successful_early_resume_needs_no_retry() {
        let mut resume = ResumePolicy::new(42_000);
        resume.initial_seek_finished(true);

        assert_eq!(resume.position_tick(180_000), None);
    }

    #[test]
    fn rad_2_pause_is_disconnect_presented_as_paused() {
        let mut state = RadioPresentation::connected();
        state.on_stream_title(Some("A song".into()));

        assert_eq!(state.pause(), Some(RadioCommand::Disconnect));
        assert_eq!(state.phase(), RadioPhase::Paused);
        assert_eq!(state.last_title(), Some("A song"));
        assert_eq!(state.table_now_playing(), None);
        assert_eq!(state.play(), Some(RadioCommand::Reconnect));

        state.reconnect_failed("station unavailable".into());
        assert_eq!(state.phase(), RadioPhase::Paused);
        assert_eq!(state.inline_error(), Some("station unavailable"));
        assert!(!state.is_empty());
    }

    #[test]
    fn reconnecting_radio_can_be_paused_before_audio_starts() {
        let mut state = RadioPresentation {
            phase: RadioPhase::Reconnecting,
            last_title: None,
            inline_error: None,
        };

        assert_eq!(state.pause(), Some(RadioCommand::Disconnect));
        assert_eq!(state.phase(), RadioPhase::Paused);
    }

    #[test]
    fn downloaded_youtube_episode_plays_without_resolution() {
        assert!(!podcast_source_requires_resolution(
            PodcastKind::Youtube,
            &EpisodeSource::File("/data/episode.webm".into()),
        ));
        assert!(podcast_source_requires_resolution(
            PodcastKind::Youtube,
            &EpisodeSource::Url("https://youtube.example/watch?v=1".into()),
        ));
    }

    #[test]
    fn pausing_an_async_resolution_invalidates_its_generation() {
        let mut state = ExternalPlaybackState::default();
        let previous = state.generation;

        state.invalidate_pending();

        assert_ne!(state.generation, previous);
    }

    #[test]
    fn stream_tags_only_belong_to_a_connected_radio() {
        let mut state = RadioPresentation::connected();
        assert!(state.accepts_stream_tags());
        state.pause();
        assert!(!state.accepts_stream_tags());
        state.play();
        assert!(!state.accepts_stream_tags());
    }

    #[test]
    fn radio_retry_is_consumed_until_a_user_reconnect_resets_it() {
        let mut guard = reprise_core::radio::click::ReresolveGuard::default();
        assert!(guard.take_retry(Some("station")));
        assert!(!guard.take_retry(Some("station")));

        guard = reprise_core::radio::click::ReresolveGuard::default();
        assert!(guard.take_retry(Some("station")));
    }

    #[test]
    fn playing_radio_activation_stops_instead_of_pausing() {
        let state = RadioPresentation::connected();
        assert_eq!(state.activation(), RadioCommand::Stop);
    }
}
