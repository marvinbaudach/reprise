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

use super::external_media_state::{
    podcast_source_requires_resolution, ExternalSession, PodcastSession, RadioCommand,
    RadioSession, ResumePolicy,
};
use super::preview::PlaybackMode;

pub(in crate::ui) use super::external_media_state::{
    EpisodeSource, ExternalMedia, ExternalPlaybackSnapshot, ExternalPlaybackState, PodcastPhase,
    RadioPhase, RadioPresentation, StreamTags,
};

const POSITION_PERSIST_INTERVAL_MS: i64 = 5_000;

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

    pub(in crate::ui) fn play_podcast_episode(self: &Rc<Self>, episode: &EpisodeRow) {
        if let Err(error) = self.play_external(media_from_episode(episode)) {
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
        should_stop
    }

    pub(in crate::ui) fn play_external(
        self: &Rc<Self>,
        media: ExternalMedia,
    ) -> Result<(), PlaybackError> {
        self.persist_external_position();
        self.evaluate_play_tracking();
        self.sync_lyrics_track(None);
        self.current_track.set(None);
        self.max_position_ms.set(0);
        self.player.set_next(None);
        *self.now_playing.borrow_mut() = None;

        match media {
            media @ ExternalMedia::Podcast { episode_id, .. } => {
                self.begin_podcast(media, episode_id)
            }
            media @ ExternalMedia::Radio { station_id, .. } => {
                self.begin_radio(media, station_id);
                Ok(())
            }
        }
    }

    fn begin_podcast(
        self: &Rc<Self>,
        media: ExternalMedia,
        episode_id: i64,
    ) -> Result<(), PlaybackError> {
        let row = reprise_core::podcasts::store::episode(&self.conn.borrow(), episode_id)
            .map_err(|error| PlaybackError::Backend(error.to_string()))?;
        let kind = row
            .as_ref()
            .map_or(PodcastKind::Rss, |episode| episode.kind);
        let subscription_id = row.as_ref().map_or(0, |episode| episode.subscription_id);
        let published_at = row.as_ref().and_then(|episode| episode.published_at);
        let art_url = row.and_then(|episode| episode.show_image_url);
        let (title, show, source, resume_ms, duration_ms) = podcast_fields(&media);
        let needs_ytdlp = podcast_source_requires_resolution(kind, &source);
        let phase = if needs_ytdlp {
            PodcastPhase::Resolving
        } else {
            PodcastPhase::Playing
        };
        let session = PodcastSession {
            media,
            subscription_id,
            published_at,
            art_url,
            phase,
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
        let setting = reprise_core::podcasts::config::load(&self.conn.borrow())
            .ok()
            .and_then(|config| config.ytdlp_path);
        let result = crate::ui::one_shot_task::spawn("reprise-youtube-resolve", move || {
            reprise_core::podcasts::ytdlp::YtDlp::discover(setting.as_deref()).resolve(&video_url)
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
                    if !controller.external_generation_matches(generation, PlaybackMode::Podcast) {
                        return;
                    }
                    if let Some(duration) = audio.duration_secs {
                        let _ = reprise_core::podcasts::store::save_duration(
                            &controller.conn.borrow(),
                            episode_id,
                            duration,
                        );
                        controller.update_podcast_duration(
                            generation,
                            episode_id,
                            duration * 1_000,
                        );
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
        &self,
        generation: u64,
        episode_id: i64,
        source: EpisodeSource,
        resume_ms: i64,
    ) -> Result<(), PlaybackError> {
        if !self.external_generation_matches(generation, PlaybackMode::Podcast) {
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
        let should_seek = {
            let mut external = self.external.borrow_mut();
            let Some(ExternalSession::Podcast(session)) = external.session.as_mut() else {
                return Ok(());
            };
            if session_id(&session.media) != episode_id {
                return Ok(());
            }
            session.phase = PodcastPhase::Playing;
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
        let art_url = reprise_core::radio::station::get(&self.conn.borrow(), station_id)
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
        let _ = reprise_core::radio::station::update_stream_url(
            &self.conn.borrow(),
            station_id,
            stream_url,
        );
        match self.player.play_uri(stream_url) {
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
        match self.playback_mode() {
            PlaybackMode::Podcast => {
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
        if let Err(error) = self.player.stop() {
            tracing::error!(%error, "failed to stop external playback");
            self.sync_state(PlaybackState::Stopped);
            self.sync_clear_track();
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

    pub(in crate::ui) fn begin_external_preview(&self, path: String) {
        self.persist_external_position();
        self.external.borrow_mut().begin_preview(path);
        self.notify_external_changed();
    }

    pub(in crate::ui) fn persist_external_on_quit(&self) {
        self.persist_external_position();
    }

    pub(in crate::ui) fn handle_external_position(&self, position_ms: i64, duration_ms: i64) {
        let (episode_id, persist, save_duration, retry_seek) = {
            let mut external = self.external.borrow_mut();
            let Some(ExternalSession::Podcast(session)) = external.session.as_mut() else {
                return;
            };
            session.position_ms = position_ms.max(0);
            let episode_id = session_id(&session.media);
            let persist = (session.position_ms - session.last_persisted_ms).abs()
                >= POSITION_PERSIST_INTERVAL_MS;
            if persist {
                session.last_persisted_ms = session.position_ms;
            }
            let save_duration = (!session.duration_known && duration_ms > 0).then_some(duration_ms);
            if save_duration.is_some() {
                session.duration_known = true;
                if let ExternalMedia::Podcast {
                    duration_ms: current,
                    ..
                } = &mut session.media
                {
                    *current = save_duration;
                }
            }
            let retry_seek = session.resume.position_tick(duration_ms);
            (episode_id, persist, save_duration, retry_seek)
        };
        if persist {
            let _ = reprise_core::podcasts::store::save_position(
                &self.conn.borrow(),
                episode_id,
                position_ms,
            );
        }
        if let Some(duration_ms) = save_duration {
            let _ = reprise_core::podcasts::store::save_duration(
                &self.conn.borrow(),
                episode_id,
                duration_ms / 1_000,
            );
            self.update_external_mpris(self.external_mpris_status());
        }
        if let Some(resume_ms) = retry_seek {
            let _ = self.player.seek_to(resume_ms);
        }
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

    pub(in crate::ui) fn finish_external(self: &Rc<Self>) {
        match self.playback_mode() {
            PlaybackMode::Podcast => self.finish_podcast(),
            PlaybackMode::Radio => self.handle_external_error("Radio stream ended".into()),
            PlaybackMode::Preview => self.end_preview(),
            PlaybackMode::Queue => {}
        }
    }

    fn finish_podcast(self: &Rc<Self>) {
        let finished = {
            let external = self.external.borrow();
            let Some(ExternalSession::Podcast(session)) = external.session.as_ref() else {
                return;
            };
            (
                session_id(&session.media),
                session.subscription_id,
                session.published_at,
            )
        };
        let now = chrono::Utc::now().timestamp();
        if let Err(error) =
            reprise_core::podcasts::store::mark_played(&self.conn.borrow(), finished.0, now)
        {
            tracing::error!(%error, episode_id = finished.0, "could not mark podcast played");
        }
        let next = reprise_core::podcasts::query::next_unplayed_of_show(
            &self.conn.borrow(),
            finished.1,
            finished.2,
        )
        .ok()
        .flatten();
        let callbacks = {
            let mut external = self.external.borrow_mut();
            external.play_next = next.clone();
            external.clear_session();
            external.play_next_callbacks.clone()
        };
        if let Some(next) = next {
            self.show_play_next_offer(&next);
            for callback in callbacks {
                callback(next.clone());
            }
        }
        self.update_mpris_mirror(MprisPlaybackStatus::Stopped);
        self.sync_state(PlaybackState::Stopped);
        self.sync_clear_track();
        self.notify_external_changed();
    }

    pub(in crate::ui) fn handle_external_error(self: &Rc<Self>, message: String) {
        if self.playback_mode() != PlaybackMode::Radio {
            if self.playback_mode() == PlaybackMode::Podcast {
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

    fn fail_podcast(&self, generation: u64, message: &str) {
        if !self.external_generation_matches(generation, PlaybackMode::Podcast) {
            return;
        }
        if let Some(ExternalSession::Podcast(session)) = self.external.borrow_mut().session.as_mut()
        {
            session.phase = PodcastPhase::Failed;
            session.error = Some(message.to_owned());
        }
        self.show_toast(message);
        self.sync_state(PlaybackState::Stopped);
        self.update_external_mpris(MprisPlaybackStatus::Stopped);
        self.notify_external_changed();
    }

    fn set_podcast_phase(&self, phase: PodcastPhase) {
        if let Some(ExternalSession::Podcast(session)) = self.external.borrow_mut().session.as_mut()
        {
            session.phase = phase;
        }
        self.notify_external_changed();
    }

    fn persist_external_position(&self) {
        let value = {
            let external = self.external.borrow();
            let Some(ExternalSession::Podcast(session)) = external.session.as_ref() else {
                return;
            };
            (session_id(&session.media), session.position_ms)
        };
        if let Err(error) =
            reprise_core::podcasts::store::save_position(&self.conn.borrow(), value.0, value.1)
        {
            tracing::warn!(%error, episode_id = value.0, "could not persist podcast position");
        }
    }

    fn external_generation_matches(&self, generation: u64, mode: PlaybackMode) -> bool {
        let external = self.external.borrow();
        external.generation == generation && external.mode() == mode
    }

    fn notify_external_changed(&self) {
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

fn podcast_fields(media: &ExternalMedia) -> (String, String, EpisodeSource, i64, Option<i64>) {
    let ExternalMedia::Podcast {
        title,
        show,
        source,
        resume_ms,
        duration_ms,
        ..
    } = media
    else {
        unreachable!("podcast fields requested from radio media")
    };
    (
        title.clone(),
        show.clone(),
        source.clone(),
        *resume_ms,
        *duration_ms,
    )
}

fn podcast_identity_fields(
    media: &ExternalMedia,
) -> (i64, String, String, EpisodeSource, i64, Option<i64>) {
    let ExternalMedia::Podcast {
        episode_id,
        title,
        show,
        source,
        resume_ms,
        duration_ms,
    } = media
    else {
        unreachable!("podcast identity requested from radio media")
    };
    (
        *episode_id,
        title.clone(),
        show.clone(),
        source.clone(),
        *resume_ms,
        *duration_ms,
    )
}

fn session_id(media: &ExternalMedia) -> i64 {
    match media {
        ExternalMedia::Podcast { episode_id, .. } => *episode_id,
        ExternalMedia::Radio { station_id, .. } => *station_id,
    }
}

fn radio_fields(media: &ExternalMedia) -> (String, String, Option<String>) {
    let (_, name, stream_url, uuid) = radio_identity_fields(media);
    (name, stream_url, uuid)
}

fn radio_identity_fields(media: &ExternalMedia) -> (i64, String, String, Option<String>) {
    let ExternalMedia::Radio {
        station_id,
        name,
        stream_url,
        uuid,
    } = media
    else {
        unreachable!("radio fields requested from podcast media")
    };
    (*station_id, name.clone(), stream_url.clone(), uuid.clone())
}
