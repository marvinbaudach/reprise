//! External podcast and radio controller integration.
//!
//! Wave 2 source views consume this controller-facing API. Keep the complete
//! E2 seam compilable while those callers are still landing.
#![allow(dead_code)]

use std::rc::Rc;

use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::playback::{PlaybackError, PlaybackState};
use reprise_core::podcasts::{EpisodeRow, PodcastKind};

use crate::ui::player_controller::PlayerController;

pub(super) use super::external_media_fields::session_id;
use super::external_media_fields::{podcast_fields, radio_fields};
use super::external_media_state::{
    podcast_source_requires_resolution, AutomaticAdvance, ExternalSession, NeighbourContext,
    PodcastOrigin, PodcastSession, RadioCommand, RadioSession, ResumePolicy,
};
use super::preview::PlaybackMode;

pub(in crate::ui) use super::external_media_state::{
    EpisodeSource, ExternalMedia, ExternalPlaybackSnapshot, ExternalPlaybackState, PodcastPhase,
    RadioPhase, RadioPresentation, StreamTags,
};

pub(super) const POSITION_PERSIST_INTERVAL_MS: i64 = 5_000;

impl PlayerController {
    pub(in crate::ui) fn playback_mode(&self) -> PlaybackMode {
        self.external.borrow().mode()
    }

    pub(in crate::ui) fn add_on_stream_tags(&self, callback: impl Fn(StreamTags) + 'static) {
        self.external
            .borrow_mut()
            .stream_tags_callbacks
            .push(Rc::new(callback));
    }

    pub(in crate::ui) fn add_on_external_changed(
        &self,
        callback: impl Fn(Option<ExternalPlaybackSnapshot>) + 'static,
    ) {
        self.external
            .borrow_mut()
            .changed_callbacks
            .push(Rc::new(callback));
    }

    pub(in crate::ui) fn add_on_play_next_offer(&self, callback: impl Fn(EpisodeRow) + 'static) {
        self.external
            .borrow_mut()
            .play_next_callbacks
            .push(Rc::new(callback));
    }

    /// Registers a listener for "an episode was marked played".
    ///
    /// This is deliberately separate from `add_on_queue_changed`: a queue
    /// change is playback state and moves no database-backed count, which is
    /// what lets that path patch a single badge instead of rebuilding. Marking
    /// an episode played *does* move one — the unplayed counts behind the
    /// Podcasts and YouTube rows — so it needs a signal of its own.
    pub(in crate::ui) fn add_on_episode_played(&self, callback: impl Fn() + 'static) {
        self.external
            .borrow_mut()
            .episode_played_callbacks
            .push(Rc::new(callback));
    }

    /// Announces a completed episode. Callbacks are cloned out first so no
    /// borrow on `external` is live while they run — they reach back into the
    /// sidebar and must be free to touch player state.
    pub(in crate::ui) fn notify_episode_played(&self) {
        let callbacks = self.external.borrow().episode_played_callbacks.clone();
        for callback in callbacks {
            callback();
        }
    }

    pub(in crate::ui) fn pending_play_next(&self) -> Option<EpisodeRow> {
        self.external.borrow().play_next.clone()
    }

    pub(in crate::ui) fn play_pending_next(self: &Rc<Self>) {
        let Some(episode) = self.pending_play_next() else {
            return;
        };
        if let Err(error) =
            self.play_external(super::external_media_toast::media_from_episode(&episode))
        {
            self.show_toast(&error.to_string());
        }
    }

    pub(in crate::ui) fn stop_podcast_subscription(&self, subscription_id: i64) -> bool {
        let should_stop = self
            .external
            .borrow()
            .plays_podcast_subscription(subscription_id);
        if should_stop {
            self.stop_external();
        }
        self.purge_unavailable_episodes();
        should_stop
    }

    pub(in crate::ui) fn play_external(
        self: &Rc<Self>,
        media: ExternalMedia,
    ) -> Result<(), PlaybackError> {
        self.play_external_with_context(media, None, None)
    }

    pub(super) fn play_external_with_context(
        self: &Rc<Self>,
        media: ExternalMedia,
        neighbours: Option<NeighbourContext>,
        automatic_advance: Option<AutomaticAdvance>,
    ) -> Result<(), PlaybackError> {
        self.play_external_with_context_and_origin(
            media,
            neighbours,
            automatic_advance,
            PodcastOrigin::Direct,
        )
    }

    fn play_external_with_origin(
        self: &Rc<Self>,
        media: ExternalMedia,
        origin: PodcastOrigin,
    ) -> Result<(), PlaybackError> {
        self.play_external_with_context_and_origin(media, None, None, origin)
    }

    pub(super) fn play_external_with_context_and_origin(
        self: &Rc<Self>,
        media: ExternalMedia,
        neighbours: Option<NeighbourContext>,
        automatic_advance: Option<AutomaticAdvance>,
        origin: PodcastOrigin,
    ) -> Result<(), PlaybackError> {
        match media {
            media @ ExternalMedia::Podcast { episode_id, .. } => {
                let row = reprise_core::podcasts::store::episode(&self.conn, episode_id)
                    .map_err(|error| PlaybackError::Backend(error.to_string()))?;
                self.prepare_external_playback();
                self.begin_podcast(media, row, neighbours, automatic_advance, origin)
            }
            media @ ExternalMedia::Radio { station_id, .. } => {
                self.prepare_external_playback();
                self.begin_radio(media, station_id);
                Ok(())
            }
        }
    }

    pub(super) fn play_podcast_row_with_context(
        self: &Rc<Self>,
        episode: EpisodeRow,
        neighbours: NeighbourContext,
        automatic_advance: AutomaticAdvance,
        origin: PodcastOrigin,
    ) -> Result<(), PlaybackError> {
        let media = media_from_episode(&episode);
        self.prepare_external_playback();
        self.begin_podcast(
            media,
            Some(episode),
            Some(neighbours),
            Some(automatic_advance),
            origin,
        )
    }

    fn prepare_external_playback(&self) {
        self.persist_external_position();
        self.evaluate_play_tracking();
        self.sync_lyrics_track(None);
        self.current_track.set(None);
        self.max_position_ms.set(0);
        self.player.set_next(None);
        *self.now_playing.borrow_mut() = None;
    }

    pub(in crate::ui) fn play_queued_episode(self: &Rc<Self>, episode_id: i64) {
        let neighbours = {
            let pending = self.up_next.borrow();
            NeighbourContext::for_manual_queue(
                reprise_core::up_next::QueueItem::Episode(episode_id),
                pending.ids(),
            )
        };
        let episode = reprise_core::podcasts::store::episode(&self.conn, episode_id);
        match episode {
            Ok(Some(episode)) => {
                if let Err(error) = self.play_external_with_context_and_origin(
                    media_from_episode(&episode),
                    neighbours,
                    None,
                    PodcastOrigin::ManualQueue,
                ) {
                    tracing::error!(%error, episode_id, "queued episode playback failed");
                }
            }
            Ok(None) => {
                tracing::info!(
                    episode_id,
                    "queued episode is no longer subscribed; dropping it silently"
                );
                self.advance_playback(super::up_next_transport::AdvanceReason::Automatic);
            }
            Err(error) => {
                tracing::error!(%error, episode_id, "could not resolve queued episode");
                super::playback_faults::note_episode_skip(&self.consecutive_episode_skips);
                self.skip_after_failure();
            }
        }
    }

    fn begin_podcast(
        self: &Rc<Self>,
        media: ExternalMedia,
        row: Option<EpisodeRow>,
        neighbours: Option<NeighbourContext>,
        automatic_advance: Option<AutomaticAdvance>,
        origin: PodcastOrigin,
    ) -> Result<(), PlaybackError> {
        let episode_id = session_id(&media);
        let kind = row
            .as_ref()
            .map_or(PodcastKind::Rss, |episode| episode.kind);
        let subscription_id = row.as_ref().map_or(0, |episode| episode.subscription_id);
        let published_at = row.as_ref().and_then(|episode| episode.published_at);
        let media_category = row
            .as_ref()
            .and_then(|episode| episode.media_category.clone());
        let art_url = row.and_then(|episode| episode.image_url.or(episode.show_image_url));
        let (title, show, source, resume_ms, duration_ms) = podcast_fields(&media);
        let needs_ytdlp = podcast_source_requires_resolution(kind, &source);
        let phase = if needs_ytdlp {
            PodcastPhase::Resolving
        } else {
            PodcastPhase::Playing
        };
        let session = PodcastSession {
            media,
            neighbours,
            automatic_advance,
            subscription_id,
            kind,
            media_category,
            published_at,
            art_url,
            phase,
            restored: false,
            origin,
            resume: ResumePolicy::new(resume_ms),
            position_ms: resume_ms.max(0),
            last_persisted_ms: resume_ms.max(0),
            duration_known: duration_ms.is_some_and(|duration| duration > 0),
            error: None,
        };
        let generation = self
            .external
            .borrow_mut()
            .begin_session(ExternalSession::Podcast(session));
        self.sync_track(&title, &show, "", None);
        self.sync_cover("");
        self.update_mpris_position(resume_ms.max(0));
        self.update_external_mpris(MprisPlaybackStatus::Playing);
        self.notify_external_changed();

        if needs_ytdlp {
            self.resolve_youtube(generation, episode_id, source, resume_ms);
            return Ok(());
        }
        self.start_podcast_source(generation, episode_id, source, resume_ms)
    }

    fn resolve_youtube(
        self: &Rc<Self>,
        generation: u64,
        episode_id: i64,
        source: EpisodeSource,
        resume_ms: i64,
    ) {
        let EpisodeSource::Url(video_url) = source else {
            self.fail_podcast(generation, "YouTube episodes require a video URL");
            return;
        };
        let config = reprise_core::podcasts::config::load(&self.conn).ok();
        let setting = config.as_ref().and_then(|config| config.ytdlp_path.clone());
        let browser = config.and_then(|config| config.youtube_browser);
        let result = crate::ui::one_shot_task::spawn("reprise-youtube-resolve", move || {
            reprise_core::podcasts::ytdlp::YtDlp::discover_with_browser(setting.as_deref(), browser)
                .resolve(&video_url)
        });
        let receiver = match result {
            Ok(receiver) => receiver,
            Err(error) => {
                self.fail_podcast(generation, &error.to_string());
                return;
            }
        };
        let weak = Rc::downgrade(self);
        gtk4::glib::spawn_future_local(async move {
            let Ok(result) = receiver.recv().await else {
                return;
            };
            let Some(controller) = weak.upgrade() else {
                return;
            };
            match result {
                Ok(audio) => {
                    if !controller.external_generation_matches_podcast(generation) {
                        return;
                    }
                    let media_category = audio
                        .categories
                        .into_iter()
                        .next()
                        .filter(|category| !category.is_empty());
                    // Playback goes ahead either way — a failed write costs the
                    // stored duration and category, not the episode. But it
                    // must not vanish: without this the classification would
                    // simply never stick, and the panel would look like the
                    // feature was never built.
                    if let Err(error) = reprise_core::podcasts::store::save_youtube_resolution(
                        &controller.conn,
                        episode_id,
                        audio.duration_secs,
                        media_category.as_deref(),
                    ) {
                        tracing::warn!(
                            %error,
                            episode_id,
                            "could not persist the resolved episode metadata"
                        );
                    }
                    if let Some(duration) = audio.duration_secs {
                        controller.update_podcast_duration(
                            generation,
                            episode_id,
                            duration * 1_000,
                        );
                    }
                    let category_changed = controller
                        .external
                        .borrow_mut()
                        .update_podcast_media_category(generation, episode_id, media_category);
                    if category_changed {
                        controller.notify_external_changed();
                    }
                    let _ = controller.start_podcast_source(
                        generation,
                        episode_id,
                        EpisodeSource::Url(audio.stream_url),
                        resume_ms,
                    );
                }
                Err(error) => controller.fail_podcast(generation, &error.to_string()),
            }
        });
    }

    fn start_podcast_source(
        self: &Rc<Self>,
        generation: u64,
        episode_id: i64,
        source: EpisodeSource,
        resume_ms: i64,
    ) -> Result<(), PlaybackError> {
        if !self.external_generation_matches_podcast(generation) {
            return Ok(());
        }
        let result = match source {
            EpisodeSource::File(path) => self.player.play(&path),
            EpisodeSource::Url(uri) => self.player.play_uri(&uri),
        };
        if let Err(error) = result {
            self.fail_podcast(generation, &error.to_string());
            return Err(error);
        }
        self.flush_episode_skip_toast();
        let should_seek = {
            let mut external = self.external.borrow_mut();
            let Some(ExternalSession::Podcast(session)) = external.session.as_mut() else {
                return Ok(());
            };
            if session_id(&session.media) != episode_id {
                return Ok(());
            }
            session.phase = PodcastPhase::Playing;
            // The advance chain deliberately survives this point. `play_uri`
            // returning `Ok` only means the pipeline accepted the URI — for a
            // resolved YouTube stream the HTTP answer (commonly a 403 on a
            // freshly signed googlevideo url) arrives asynchronously on the
            // bus afterwards. Clearing the chain here would classify that
            // failure as "direct" and strand the user on a dead row. It is
            // cleared once playback actually advances, in
            // `handle_external_position`.
            resume_ms > 0
        };
        if should_seek {
            let succeeded = self.player.seek_to(resume_ms).is_ok();
            let mut external = self.external.borrow_mut();
            if external.generation == generation {
                if let Some(ExternalSession::Podcast(session)) = external.session.as_mut() {
                    if session_id(&session.media) == episode_id {
                        session.resume.initial_seek_finished(succeeded);
                    }
                }
            }
        }
        self.update_external_mpris(MprisPlaybackStatus::Playing);
        self.notify_external_changed();
        Ok(())
    }

    fn begin_radio(self: &Rc<Self>, media: ExternalMedia, station_id: i64) {
        let art_url = reprise_core::radio::station::get(&self.conn, station_id)
            .ok()
            .flatten()
            .and_then(|station| station.favicon_url);
        let (name, stream_url, uuid) = radio_fields(&media);
        let session = RadioSession {
            media,
            art_url,
            presentation: RadioPresentation {
                phase: RadioPhase::Reconnecting,
                last_title: None,
                inline_error: None,
            },
            retry_guard: reprise_core::radio::click::ReresolveGuard::default(),
        };
        let generation = self
            .external
            .borrow_mut()
            .begin_session(ExternalSession::Radio(session));
        self.sync_track(&name, "", "", None);
        self.sync_cover("");
        self.update_mpris_position(0);
        self.update_external_mpris(MprisPlaybackStatus::Playing);
        self.notify_external_changed();
        self.resolve_radio(generation, station_id, uuid, stream_url, false);
    }

    fn resolve_radio(
        self: &Rc<Self>,
        generation: u64,
        station_id: i64,
        uuid: Option<String>,
        fallback_url: String,
        retry: bool,
    ) {
        let Some(uuid) = uuid else {
            self.apply_radio_resolution(generation, station_id, &fallback_url, retry);
            return;
        };
        let result = crate::ui::one_shot_task::spawn("reprise-radio-resolve", move || {
            reprise_core::radio::click::click_and_resolve(&uuid).ok()
        });
        let receiver = match result {
            Ok(receiver) => receiver,
            Err(_) => {
                self.apply_radio_resolution(generation, station_id, &fallback_url, retry);
                return;
            }
        };
        let weak = Rc::downgrade(self);
        gtk4::glib::spawn_future_local(async move {
            let Ok(resolved) = receiver.recv().await else {
                return;
            };
            let Some(controller) = weak.upgrade() else {
                return;
            };
            controller.apply_radio_resolution(
                generation,
                station_id,
                &resolved.unwrap_or(fallback_url),
                retry,
            );
        });
    }

    fn apply_radio_resolution(
        &self,
        generation: u64,
        station_id: i64,
        stream_url: &str,
        retry: bool,
    ) {
        if !self.external_generation_matches(generation, PlaybackMode::Radio) {
            return;
        }
        {
            let mut external = self.external.borrow_mut();
            let Some(ExternalSession::Radio(session)) = external.session.as_mut() else {
                return;
            };
            if let ExternalMedia::Radio {
                stream_url: fallback,
                ..
            } = &mut session.media
            {
                *fallback = stream_url.to_owned();
            }
        }
        let _ = reprise_core::radio::station::update_stream_url(&self.conn, station_id, stream_url);
        match self.player.play_live_uri(stream_url) {
            Ok(()) => {
                if let Some(ExternalSession::Radio(session)) =
                    self.external.borrow_mut().session.as_mut()
                {
                    session.presentation.reconnect_succeeded();
                }
                self.update_external_mpris(MprisPlaybackStatus::Playing);
                self.notify_external_changed();
            }
            Err(error) if !retry => {
                self.radio_reconnect_failed(error.to_string());
            }
            Err(error) => self.radio_reconnect_failed(error.to_string()),
        }
    }

    pub(in crate::ui) fn toggle_external_pause(self: &Rc<Self>) -> bool {
        if self.resume_restored_episode() {
            return true;
        }
        match self.playback_mode() {
            PlaybackMode::Podcast | PlaybackMode::QueuedEpisode => {
                match self.player.toggle_pause() {
                    Ok(PlaybackState::Paused) => {
                        self.persist_external_position();
                        self.set_podcast_phase(PodcastPhase::Paused);
                        self.update_external_mpris(MprisPlaybackStatus::Paused);
                    }
                    Ok(PlaybackState::Playing) => {
                        self.set_podcast_phase(PodcastPhase::Playing);
                        self.update_external_mpris(MprisPlaybackStatus::Playing);
                    }
                    Ok(PlaybackState::Stopped) => {}
                    Err(error) => tracing::error!(%error, "podcast pause toggle failed"),
                }
                true
            }
            PlaybackMode::Radio => {
                let command = {
                    let mut external = self.external.borrow_mut();
                    let Some(ExternalSession::Radio(session)) = external.session.as_mut() else {
                        return false;
                    };
                    if matches!(
                        session.presentation.phase,
                        RadioPhase::Connected | RadioPhase::Reconnecting
                    ) {
                        session.presentation.pause()
                    } else {
                        session.presentation.play()
                    }
                };
                match command {
                    Some(RadioCommand::Disconnect) => {
                        self.external.borrow_mut().invalidate_pending();
                        let _ = self.player.stop();
                        self.sync_state(PlaybackState::Paused);
                        self.update_external_mpris(MprisPlaybackStatus::Paused);
                        self.notify_external_changed();
                    }
                    Some(RadioCommand::Reconnect) => {
                        self.notify_external_changed();
                        self.retry_radio();
                    }
                    _ => {}
                }
                true
            }
            PlaybackMode::Queue | PlaybackMode::Preview => false,
        }
    }

    /// Ends the external session for good — nothing takes over from it. The
    /// queue-takeover path is [`Self::leave_external_for_queue`], which leaves
    /// the loaded track alone.
    pub(in crate::ui) fn stop_external(&self) {
        self.persist_external_position();
        {
            let mut external = self.external.borrow_mut();
            external.clear_session();
            external.clear_preview();
        }
        *self.now_playing.borrow_mut() = None;
        self.update_mpris_mirror(MprisPlaybackStatus::Stopped);
        self.notify_external_changed();
        // `PLAY-12`: nothing is loaded any more, so the bar, the compact
        // player and the panel go to their empty state — whether or not the
        // pipeline managed to stop. Leaving this to the error path kept the
        // finished session's title, channel and cover links operable, still
        // labelled for a target that no longer plays. Same order as
        // `finish_podcast`.
        self.sync_clear_track();
        if let Err(error) = self.player.stop() {
            tracing::error!(%error, "failed to stop external playback");
            self.sync_state(PlaybackState::Stopped);
        }
    }

    pub(in crate::ui) fn leave_external_for_queue(&self) {
        self.persist_external_position();
        let mut external = self.external.borrow_mut();
        external.clear_session();
        external.clear_preview();
        external.play_next = None;
        drop(external);
        self.notify_external_changed();
    }

    pub(in crate::ui) fn on_stream_tags(
        &self,
        title: Option<String>,
        organization: Option<String>,
    ) {
        let accepts_tags = {
            let external = self.external.borrow();
            matches!(
                external.session.as_ref(),
                Some(ExternalSession::Radio(session))
                    if session.presentation.accepts_stream_tags()
            )
        };
        if !accepts_tags {
            return;
        }
        let tags = StreamTags {
            title,
            organization,
        };
        let callbacks = {
            let mut external = self.external.borrow_mut();
            external.stream_tags = tags.clone();
            if let Some(ExternalSession::Radio(session)) = external.session.as_mut() {
                session.presentation.on_stream_title(tags.title.clone());
            }
            external.stream_tags_callbacks.clone()
        };
        for callback in callbacks {
            callback(tags.clone());
        }
        self.update_external_mpris(self.external_mpris_status());
        self.notify_external_changed();
    }

    pub(in crate::ui) fn handle_external_error(self: &Rc<Self>, message: String) {
        if self.playback_mode() != PlaybackMode::Radio {
            if matches!(
                self.playback_mode(),
                PlaybackMode::Podcast | PlaybackMode::QueuedEpisode
            ) {
                let generation = self.external.borrow().generation;
                self.fail_podcast(generation, &message);
            }
            return;
        }
        let retry = {
            let mut external = self.external.borrow_mut();
            let Some(ExternalSession::Radio(session)) = external.session.as_mut() else {
                return;
            };
            let ExternalMedia::Radio {
                station_id,
                stream_url,
                uuid,
                ..
            } = &session.media
            else {
                return;
            };
            if !session.retry_guard.take_retry(uuid.as_deref()) {
                None
            } else {
                session.presentation.phase = RadioPhase::Reconnecting;
                Some((*station_id, stream_url.clone(), uuid.clone()))
            }
        };
        if let Some((station_id, fallback, uuid)) = retry {
            let generation = self.external.borrow().generation;
            self.resolve_radio(generation, station_id, uuid, fallback, true);
        } else {
            self.radio_reconnect_failed(message);
        }
    }

    fn retry_radio(self: &Rc<Self>) {
        let request = {
            let mut external = self.external.borrow_mut();
            let Some(ExternalSession::Radio(session)) = external.session.as_mut() else {
                return;
            };
            let ExternalMedia::Radio {
                station_id,
                stream_url,
                uuid,
                ..
            } = &session.media
            else {
                return;
            };
            session.retry_guard = reprise_core::radio::click::ReresolveGuard::default();
            (*station_id, stream_url.clone(), uuid.clone())
        };
        let generation = self.external.borrow().generation;
        self.resolve_radio(generation, request.0, request.2, request.1, true);
    }

    fn radio_reconnect_failed(&self, message: String) {
        if let Some(ExternalSession::Radio(session)) = self.external.borrow_mut().session.as_mut() {
            session.presentation.reconnect_failed(message);
        }
        self.sync_state(PlaybackState::Paused);
        self.update_external_mpris(MprisPlaybackStatus::Paused);
        self.notify_external_changed();
    }

    fn set_podcast_phase(&self, phase: PodcastPhase) {
        if let Some(ExternalSession::Podcast(session)) = self.external.borrow_mut().session.as_mut()
        {
            session.phase = phase;
        }
        self.notify_external_changed();
    }

    pub(super) fn external_generation_matches(&self, generation: u64, mode: PlaybackMode) -> bool {
        let external = self.external.borrow();
        external.generation == generation && external.mode() == mode
    }

    fn external_generation_matches_podcast(&self, generation: u64) -> bool {
        let external = self.external.borrow();
        external.generation == generation
            && matches!(
                external.mode(),
                PlaybackMode::Podcast | PlaybackMode::QueuedEpisode
            )
    }

    pub(super) fn notify_external_changed(&self) {
        let (snapshot, callbacks) = {
            let external = self.external.borrow();
            (external.snapshot(), external.changed_callbacks.clone())
        };
        for callback in callbacks {
            callback(snapshot.clone());
        }
    }
}

pub(in crate::ui) fn media_from_episode(episode: &EpisodeRow) -> ExternalMedia {
    super::external_media_toast::media_from_episode(episode)
}
