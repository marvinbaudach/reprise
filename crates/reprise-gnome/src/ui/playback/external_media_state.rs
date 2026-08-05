//! Pure state for podcast and radio playback.
//!
//! Wave 2 source views consume these state types. E2 intentionally lands the
//! complete state seam before those callers.
#![allow(dead_code)]

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use reprise_core::podcasts::{EpisodeRow, PodcastKind};
use reprise_core::up_next::QueueItem;

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

/// Queue items in rendered order, plus this session's index in them.
/// Frozen at start: a feed refresh must not move the user's neighbours.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NeighbourContext {
    items: Vec<QueueItem>,
    index: usize,
    pub(super) sequence: u64,
}

static NEXT_NEIGHBOUR_SEQUENCE: AtomicU64 = AtomicU64::new(1);

impl NeighbourContext {
    pub(super) fn for_episode(episode_ids: &[i64], episode_id: i64) -> Option<Self> {
        let items = episode_ids
            .iter()
            .copied()
            .map(QueueItem::Episode)
            .collect::<Vec<_>>();
        Self::for_item(&items, QueueItem::Episode(episode_id))
    }

    pub(super) fn for_manual_queue(current: QueueItem, pending: &[QueueItem]) -> Option<Self> {
        let mut items = Vec::with_capacity(pending.len().saturating_add(1));
        items.push(current);
        items.extend_from_slice(pending);
        Self::for_item(&items, current)
    }

    fn for_item(items: &[QueueItem], current: QueueItem) -> Option<Self> {
        let index = items.iter().position(|item| *item == current)?;
        Some(Self {
            items: items.to_vec(),
            index,
            sequence: NEXT_NEIGHBOUR_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        })
    }

    /// The current episode id, for tests that build episode-only contexts.
    ///
    /// Test-only on purpose: a `ManualQueue` context legitimately contains
    /// `Track` items, so asking a context for "the episode id" is only ever
    /// meaningful for a show-order one. Production reads `current_item()` and
    /// matches on the kind. Keeping this compiled out of the binary means a
    /// future caller cannot reach the panicking path by accident — the
    /// invariant is enforced by the build, not by discipline.
    #[cfg(test)]
    pub(super) fn current_id(&self) -> i64 {
        self.current_item()
            .episode_id()
            .expect("episode-only neighbour context must contain episodes")
    }

    pub(super) fn current_item(&self) -> QueueItem {
        self.items[self.index]
    }

    /// The items after the current one, in frozen show order.
    pub(super) fn upcoming(&self) -> &[QueueItem] {
        &self.items[self.index.saturating_add(1)..]
    }

    pub(super) fn episode_ids(&self) -> Option<Vec<i64>> {
        self.items.iter().map(|item| item.episode_id()).collect()
    }

    /// Position of the current item — the stable `start` for the tail identity.
    pub(super) fn position(&self) -> usize {
        self.index
    }

    pub(super) fn upcoming_context(&self, offset: usize) -> Option<Self> {
        self.shifted(self.index.checked_add(1)?.checked_add(offset)?)
    }

    pub(super) fn previous(&self) -> Option<Self> {
        self.shifted(self.index.checked_sub(1)?)
    }

    pub(super) fn next(&self) -> Option<Self> {
        self.shifted(self.index.checked_add(1)?)
    }

    pub(super) fn has_previous(&self) -> bool {
        self.index > 0
    }

    pub(super) fn has_next(&self) -> bool {
        self.index
            .checked_add(1)
            .is_some_and(|index| index < self.items.len())
    }

    fn shifted(&self, index: usize) -> Option<Self> {
        (index < self.items.len()).then(|| Self {
            items: self.items.clone(),
            index,
            sequence: self.sequence,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NeighbourDirection {
    Previous,
    Next,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AutomaticAdvance {
    direction: NeighbourDirection,
    failures: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AdvanceFailure {
    Retry {
        neighbours: NeighbourContext,
        chain: AutomaticAdvance,
    },
    Stop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PodcastFailureAction {
    Direct,
    Automatic(AdvanceFailure),
}

pub(super) fn should_skip_manual_queue_after_failure(
    origin: PodcastOrigin,
    action: &PodcastFailureAction,
) -> bool {
    origin == PodcastOrigin::ManualQueue && matches!(action, PodcastFailureAction::Direct)
}

impl AutomaticAdvance {
    const MAX_FAILURES: u8 = 3;

    pub(super) fn new(direction: NeighbourDirection) -> Self {
        Self {
            direction,
            failures: 0,
        }
    }

    pub(super) fn after_failure(self, current: &NeighbourContext) -> AdvanceFailure {
        let failures = self.failures.saturating_add(1);
        if failures >= Self::MAX_FAILURES {
            return AdvanceFailure::Stop;
        }
        let neighbours = match self.direction {
            NeighbourDirection::Previous => current.previous(),
            NeighbourDirection::Next => current.next(),
        };
        neighbours.map_or(AdvanceFailure::Stop, |neighbours| AdvanceFailure::Retry {
            neighbours,
            chain: Self {
                direction: self.direction,
                failures,
            },
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PodcastOrigin {
    Direct,
    ManualQueue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PodcastSession {
    pub(super) media: ExternalMedia,
    pub(super) neighbours: Option<NeighbourContext>,
    pub(super) automatic_advance: Option<AutomaticAdvance>,
    pub(super) subscription_id: i64,
    pub(super) kind: PodcastKind,
    pub(super) published_at: Option<i64>,
    pub(super) art_url: Option<String>,
    pub(super) phase: PodcastPhase,
    /// True only for the paused metadata shell reconstructed at cold start.
    /// The first Play resolves a fresh source instead of toggling a pipeline
    /// that belongs to the previous process.
    pub(super) restored: bool,
    pub(super) origin: PodcastOrigin,
    pub(super) resume: ResumePolicy,
    pub(super) position_ms: i64,
    pub(super) last_persisted_ms: i64,
    pub(super) duration_known: bool,
    pub(super) error: Option<String>,
}

impl PodcastSession {
    pub(super) fn failure_action(&self) -> PodcastFailureAction {
        match (&self.neighbours, self.automatic_advance) {
            (Some(neighbours), Some(automatic_advance)) => {
                PodcastFailureAction::Automatic(automatic_advance.after_failure(neighbours))
            }
            _ => PodcastFailureAction::Direct,
        }
    }

    /// Ends the advance chain once playback has genuinely progressed.
    ///
    /// It deliberately does *not* end when the pipeline accepts the URI: for a
    /// resolved YouTube stream `play_uri` returns `Ok` and the HTTP answer —
    /// often a 403 on a freshly signed googlevideo url — only arrives on the
    /// bus afterwards. Ending the chain at start time would classify that as a
    /// direct failure and strand the user on a dead row, which is exactly the
    /// case this feature exists for.
    pub(super) fn note_playback_progress(&mut self, position_ms: i64) {
        if position_ms > 0 {
            self.automatic_advance = None;
        }
    }
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
    pub(in crate::ui) art_url: Option<String>,
    pub(in crate::ui) can_go_previous: bool,
    pub(in crate::ui) can_go_next: bool,
    pub(in crate::ui) stream_tags: StreamTags,
    pub(in crate::ui) podcast_phase: Option<PodcastPhase>,
    pub(in crate::ui) restored: bool,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NeighbourTransport {
    Queue,
    Item {
        neighbours: NeighbourContext,
        origin: PodcastOrigin,
    },
    Unavailable,
}

impl ExternalPlaybackState {
    pub(in crate::ui) fn mode(&self) -> PlaybackMode {
        match self.session {
            Some(ExternalSession::Podcast(ref session)) => match session.origin {
                PodcastOrigin::Direct => PlaybackMode::Podcast,
                PodcastOrigin::ManualQueue => PlaybackMode::QueuedEpisode,
            },
            Some(ExternalSession::Radio(_)) => PlaybackMode::Radio,
            None if self.preview_path.is_some() => PlaybackMode::Preview,
            None => PlaybackMode::Queue,
        }
    }

    pub(in crate::ui) fn plays_podcast_subscription(&self, subscription_id: i64) -> bool {
        matches!(
            self.session.as_ref(),
            Some(ExternalSession::Podcast(session))
                if session.subscription_id == subscription_id
        )
    }

    pub(super) fn transport_target(&self, direction: NeighbourDirection) -> NeighbourTransport {
        match self.session.as_ref() {
            Some(ExternalSession::Podcast(session)) => session
                .neighbours
                .as_ref()
                .and_then(|neighbours| match direction {
                    NeighbourDirection::Previous => neighbours.previous(),
                    NeighbourDirection::Next => neighbours.next(),
                })
                .map_or(NeighbourTransport::Unavailable, |neighbours| {
                    NeighbourTransport::Item {
                        neighbours,
                        origin: session.origin,
                    }
                }),
            Some(ExternalSession::Radio(_)) => NeighbourTransport::Unavailable,
            None if self.preview_path.is_some() => NeighbourTransport::Unavailable,
            None => NeighbourTransport::Queue,
        }
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
                mode: match session.origin {
                    PodcastOrigin::Direct => PlaybackMode::Podcast,
                    PodcastOrigin::ManualQueue => PlaybackMode::QueuedEpisode,
                },
                media: session.media.clone(),
                art_url: session.art_url.clone(),
                can_go_previous: session
                    .neighbours
                    .as_ref()
                    .is_some_and(NeighbourContext::has_previous),
                can_go_next: session
                    .neighbours
                    .as_ref()
                    .is_some_and(NeighbourContext::has_next),
                stream_tags: self.stream_tags.clone(),
                podcast_phase: Some(session.phase),
                restored: session.restored,
                radio: None,
                error: session.error.clone(),
            }),
            ExternalSession::Radio(session) => Some(ExternalPlaybackSnapshot {
                mode: PlaybackMode::Radio,
                media: session.media.clone(),
                art_url: session.art_url.clone(),
                can_go_previous: false,
                can_go_next: false,
                stream_tags: self.stream_tags.clone(),
                podcast_phase: None,
                restored: false,
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
#[path = "external_media_state_queue_tests.rs"]
mod queue_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn podcast_session(
        neighbours: Option<NeighbourContext>,
        automatic_advance: Option<AutomaticAdvance>,
    ) -> PodcastSession {
        PodcastSession {
            media: ExternalMedia::Podcast {
                episode_id: 7,
                title: "Episode".into(),
                show: "Show".into(),
                source: EpisodeSource::Url("https://example.test/episode.mp3".into()),
                resume_ms: 0,
                duration_ms: None,
            },
            neighbours,
            automatic_advance,
            subscription_id: 42,
            kind: PodcastKind::Rss,
            published_at: None,
            art_url: None,
            phase: PodcastPhase::Playing,
            restored: false,
            origin: PodcastOrigin::Direct,
            resume: ResumePolicy::new(0),
            position_ms: 0,
            last_persisted_ms: 0,
            duration_known: false,
            error: None,
        }
    }

    #[test]
    fn pod_21_a_stream_that_dies_before_playing_stays_on_the_advance_chain() {
        let neighbours = NeighbourContext::for_episode(&[1, 2, 3], 2).unwrap();
        let chain = AutomaticAdvance::new(NeighbourDirection::Next);
        let mut session = podcast_session(Some(neighbours), Some(chain));

        // `play_uri` accepted the URI, so the session is nominally "playing",
        // but nothing has streamed yet — this is the 403-after-start case.
        session.note_playback_progress(0);
        assert!(
            matches!(session.failure_action(), PodcastFailureAction::Automatic(_)),
            "a stream that never advanced must keep skipping, not strand the user"
        );

        // Once it genuinely advances, a later break is a mid-playback break.
        session.note_playback_progress(4_000);
        assert_eq!(session.failure_action(), PodcastFailureAction::Direct);
    }

    #[test]
    fn automatic_advance_skips_two_failures_and_stops_on_the_third() {
        let first_target = NeighbourContext::for_episode(&[1, 2, 3, 4, 5], 2).unwrap();
        let chain = AutomaticAdvance::new(NeighbourDirection::Next);

        let AdvanceFailure::Retry {
            neighbours: second_target,
            chain,
        } = chain.after_failure(&first_target)
        else {
            panic!("the first failure should skip to the next neighbour");
        };
        assert_eq!(second_target.current_id(), 3);

        let AdvanceFailure::Retry {
            neighbours: third_target,
            chain,
        } = chain.after_failure(&second_target)
        else {
            panic!("the second failure should skip to the next neighbour");
        };
        assert_eq!(third_target.current_id(), 4);
        assert_eq!(chain.after_failure(&third_target), AdvanceFailure::Stop);
    }

    #[test]
    fn direct_failure_stops_on_the_clicked_episode_without_skipping() {
        let neighbours = NeighbourContext::for_episode(&[6, 7, 8], 7).unwrap();
        let session = podcast_session(Some(neighbours), None);

        assert_eq!(session.failure_action(), PodcastFailureAction::Direct);
    }

    #[test]
    fn queue_navigation_is_selected_only_by_the_session_not_the_open_view() {
        let queue = ExternalPlaybackState::default();
        assert_eq!(
            queue.transport_target(NeighbourDirection::Next),
            NeighbourTransport::Queue
        );

        let neighbours = NeighbourContext::for_episode(&[6, 7, 8], 7).unwrap();
        let podcast = ExternalPlaybackState {
            session: Some(ExternalSession::Podcast(podcast_session(
                Some(neighbours),
                None,
            ))),
            ..ExternalPlaybackState::default()
        };
        assert!(matches!(
            podcast.transport_target(NeighbourDirection::Next),
            NeighbourTransport::Item {
                neighbours: context,
                origin: PodcastOrigin::Direct,
            } if context.current_id() == 8
        ));
    }

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

    /// `PLAY-12`: stopping external playback ends the session outright —
    /// nothing takes over, so the projection the player bar and the panel
    /// follow goes away with it and they have to drop to their empty state.
    /// (`leave_external_for_queue` is the other exit: there a queue track
    /// takes over and keeps the surfaces loaded.)
    #[test]
    fn play_12_a_stopped_radio_session_leaves_nothing_to_project() {
        let mut state = ExternalPlaybackState {
            session: Some(ExternalSession::Radio(RadioSession {
                media: ExternalMedia::Radio {
                    station_id: 5,
                    name: "Station".into(),
                    stream_url: "https://example.test/stream".into(),
                    uuid: None,
                },
                art_url: None,
                presentation: RadioPresentation::connected(),
                retry_guard: reprise_core::radio::click::ReresolveGuard::default(),
            })),
            ..ExternalPlaybackState::default()
        };
        assert_eq!(
            state.snapshot().map(|snapshot| snapshot.mode),
            Some(PlaybackMode::Radio)
        );

        state.clear_session();
        state.clear_preview();

        assert!(state.snapshot().is_none());
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

    #[test]
    fn removing_a_show_matches_only_its_active_podcast_session() {
        let session = podcast_session(None, None);
        let state = ExternalPlaybackState {
            session: Some(ExternalSession::Podcast(session)),
            ..ExternalPlaybackState::default()
        };

        assert!(state.plays_podcast_subscription(42));
        assert!(!state.plays_podcast_subscription(41));
    }
}
