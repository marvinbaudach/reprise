//! Current-playback fan-out for the Lyrics page.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use gtk4::glib;
use reprise_core::lyrics::{LyricsBody, LyricsQuery};
use reprise_core::playback::{PlaybackBackend, PlaybackError, PlaybackState};
use reprise_core::queries::TrackSummary;

use super::lyrics_state::{LyricsState, RequestIntent};
use super::lyrics_view::LyricsView;
use super::lyrics_worker::{LyricsRequest, LyricsResponse, LyricsRuntime};
use super::player_controller::PlayerController;

pub(in crate::ui) struct PlayerLyrics {
    runtime: Rc<LyricsRuntime>,
    state: RefCell<LyricsState>,
    view: RefCell<Weak<LyricsView>>,
    position_ms: Cell<i64>,
    enabled: Cell<bool>,
    tab_open: Cell<bool>,
    playback_state: Cell<PlaybackState>,
    line_timer: RefCell<Option<glib::SourceId>>,
    line_timer_generation: Cell<u64>,
}

impl PlayerLyrics {
    pub(in crate::ui) fn new(conn: &rusqlite::Connection) -> Rc<Self> {
        let enabled = reprise_core::online_sources::network_allowed(
            conn,
            &reprise_core::modules::ONLINE_LYRICS_MODULE,
        )
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "could not read Online Lyrics module state; defaulting to off");
            false
        });
        Self::with_runtime(LyricsRuntime::setup(), enabled)
    }

    fn with_runtime(runtime: Rc<LyricsRuntime>, enabled: bool) -> Rc<Self> {
        Rc::new(Self {
            runtime,
            state: RefCell::new(LyricsState::default()),
            view: RefCell::new(Weak::new()),
            position_ms: Cell::new(0),
            enabled: Cell::new(enabled),
            tab_open: Cell::new(false),
            playback_state: Cell::new(PlaybackState::Stopped),
            line_timer: RefCell::new(None),
            line_timer_generation: Cell::new(0),
        })
    }

    #[cfg(test)]
    pub(in crate::ui) fn setup_with_runtime(runtime: Rc<LyricsRuntime>, enabled: bool) -> Rc<Self> {
        Self::with_runtime(runtime, enabled)
    }

    pub(in crate::ui) fn set_view(self: &Rc<Self>, view: &Rc<LyricsView>) {
        *self.view.borrow_mut() = Rc::downgrade(view);
        let lyrics = Rc::downgrade(self);
        view.set_on_retry(move || {
            if let Some(lyrics) = lyrics.upgrade() {
                lyrics.retry();
            }
        });
        let lyrics = Rc::downgrade(self);
        view.set_on_tab_open_changed(move |open| {
            if let Some(lyrics) = lyrics.upgrade() {
                lyrics.set_tab_open(open);
            }
        });
        self.set_tab_open(view.is_tab_open());
        self.render_current();
    }

    pub(in crate::ui) fn set_enabled(self: &Rc<Self>, enabled: bool) {
        if self.enabled.replace(enabled) == enabled {
            return;
        }
        if !enabled {
            self.cancel_line_timer();
        } else if self.tab_open.get() {
            self.request_current();
        }
    }

    pub(in crate::ui) fn set_tab_open(self: &Rc<Self>, open: bool) {
        if self.tab_open.replace(open) == open {
            return;
        }
        if !open {
            self.cancel_line_timer();
            return;
        }
        self.request_current();
        self.schedule_next_line();
    }

    pub(in crate::ui) fn set_track(self: &Rc<Self>, query: Option<LyricsQuery>) {
        self.cancel_line_timer();
        self.position_ms.set(0);
        let clear = query.is_none();
        let intent = self.state.borrow_mut().set_track(query);
        if clear {
            if let Some(view) = self.view() {
                view.show_empty();
            }
            return;
        }
        if let Some(intent) = intent {
            self.start_request(intent);
        } else {
            self.schedule_next_line();
        }
    }

    pub(in crate::ui) fn set_position(self: &Rc<Self>, position_ms: i64) {
        self.update_position(position_ms, false);
    }

    pub(in crate::ui) fn external_seek(self: &Rc<Self>, position_ms: i64) {
        self.update_position(position_ms, true);
    }

    pub(in crate::ui) fn set_playback_state(self: &Rc<Self>, state: PlaybackState) {
        if self.playback_state.replace(state) == state {
            return;
        }
        if state == PlaybackState::Playing {
            self.schedule_next_line();
        } else {
            self.cancel_line_timer();
        }
    }

    fn update_position(self: &Rc<Self>, position_ms: i64, external_seek: bool) {
        let position_ms = position_ms.max(0);
        self.position_ms.set(position_ms);
        let (active, timestamp_ms) = {
            let mut state = self.state.borrow_mut();
            state.update_position(position_ms);
            (state.active_line(), state.active_line_timestamp_ms())
        };
        if let Some(view) = self.view() {
            view.set_active_line_at(active, timestamp_ms, position_ms);
            if external_seek {
                view.external_seek();
            }
        }
        self.schedule_next_line();
    }

    fn retry(self: &Rc<Self>) {
        let intent = self.state.borrow_mut().retry();
        if let Some(intent) = intent {
            self.start_request(intent);
        }
    }

    fn start_request(self: &Rc<Self>, intent: RequestIntent) {
        if !self.tab_open.get() {
            return;
        }
        if !self.enabled.get() {
            if let Some(view) = self.view() {
                view.show_disabled();
            }
            return;
        }
        if let Some(view) = self.view() {
            view.show_loading(&intent.query.title, &intent.query.artist);
        }
        let (sender, receiver) = async_channel::bounded(1);
        self.runtime.request(LyricsRequest {
            generation: intent.generation,
            query: intent.query,
            response: sender,
        });
        let lyrics = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(response) = receiver.recv().await else {
                return;
            };
            if let Some(lyrics) = lyrics.upgrade() {
                lyrics.apply_response(response);
            }
        });
    }

    fn request_current(self: &Rc<Self>) {
        let (has_query, has_body) = {
            let state = self.state.borrow();
            (state.query().is_some(), state.body().is_some())
        };
        if !has_query || has_body {
            self.render_current();
            return;
        }
        if !self.enabled.get() {
            if let Some(view) = self.view() {
                view.show_disabled();
            }
            return;
        }
        let intent = self.state.borrow_mut().request_missing();
        if let Some(intent) = intent {
            self.start_request(intent);
        }
    }

    fn apply_response(self: &Rc<Self>, response: LyricsResponse) {
        let accepted = self.state.borrow().accepts(response.generation);
        if !accepted {
            tracing::debug!(
                generation = response.generation,
                "lyrics response discarded as stale"
            );
            return;
        }
        match response.result {
            Ok(body) => self.apply_body(&body),
            Err(error) => {
                self.cancel_line_timer();
                if let Some(view) = self.view() {
                    view.show_error(&error);
                }
            }
        }
    }

    fn apply_body(self: &Rc<Self>, body: &LyricsBody) {
        self.state.borrow_mut().set_body(body.clone());
        if let Some(view) = self.view() {
            view.show_result(body);
        }
        let (active, timestamp_ms) = {
            let mut state = self.state.borrow_mut();
            state.update_position(self.position_ms.get());
            (state.active_line(), state.active_line_timestamp_ms())
        };
        if let Some(view) = self.view() {
            view.set_active_line_at(active, timestamp_ms, self.position_ms.get());
        }
        self.schedule_next_line();
    }

    fn render_current(self: &Rc<Self>) {
        let (query, body, active, timestamp_ms) = {
            let state = self.state.borrow();
            (
                state.query().cloned(),
                state.body().cloned(),
                state.active_line(),
                state.active_line_timestamp_ms(),
            )
        };
        let Some(view) = self.view() else {
            return;
        };
        match (query, body) {
            (_, Some(body)) => {
                view.show_result(&body);
                view.set_active_line_at(active, timestamp_ms, self.position_ms.get());
            }
            (Some(_), None) if !self.enabled.get() => view.show_disabled(),
            (Some(query), None) => view.show_loading(&query.title, &query.artist),
            (None, None) => view.show_empty(),
        }
        self.schedule_next_line();
    }

    fn schedule_next_line(self: &Rc<Self>) {
        self.cancel_line_timer();
        if !self.enabled.get()
            || !self.tab_open.get()
            || self.playback_state.get() != PlaybackState::Playing
        {
            return;
        }
        let position_ms = self.position_ms.get();
        let next_timestamp_ms = self.state.borrow().next_line_timestamp_ms(position_ms);
        let Some(next_timestamp_ms) = next_timestamp_ms else {
            return;
        };
        let Ok(delay_ms) = u64::try_from(next_timestamp_ms - position_ms) else {
            return;
        };
        let generation = self.line_timer_generation.get();
        let lyrics = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(Duration::from_millis(delay_ms), move || {
            let Some(lyrics) = lyrics.upgrade() else {
                return;
            };
            if lyrics.line_timer_generation.get() != generation {
                return;
            }
            lyrics.line_timer.borrow_mut().take();
            lyrics.update_position(next_timestamp_ms, false);
        });
        *self.line_timer.borrow_mut() = Some(source);
    }

    fn cancel_line_timer(&self) {
        self.line_timer_generation
            .set(self.line_timer_generation.get().wrapping_add(1));
        if let Some(source) = self.line_timer.borrow_mut().take() {
            source.remove();
        }
    }

    fn view(&self) -> Option<Rc<LyricsView>> {
        self.view.borrow().upgrade()
    }
}

/// Builds the lyrics lookup key for `summary` without touching playback. Used
/// on the gapless hand-off path, where the audio is already rolling and only
/// the UI/lyrics need to catch up (no `play()` call).
pub(in crate::ui) fn lyrics_query_for(summary: &TrackSummary) -> LyricsQuery {
    LyricsQuery {
        title: summary.title.clone(),
        artist: summary.artist.clone(),
        album: summary.album.clone(),
        duration_ms: summary.duration_ms,
    }
}

pub(in crate::ui) fn start_track_for_lyrics(
    player: &dyn PlaybackBackend,
    summary: &TrackSummary,
) -> Result<LyricsQuery, PlaybackError> {
    player.play(&summary.path)?;
    Ok(lyrics_query_for(summary))
}

impl PlayerController {
    pub(in crate::ui) fn set_lyrics_view(self: &Rc<Self>, view: &Rc<LyricsView>) {
        self.lyrics.set_view(view);
        let player = Rc::downgrade(self);
        view.set_on_seek(move |position_ms| {
            if let Some(player) = player.upgrade() {
                player.seek(position_ms);
            }
        });
    }

    pub(in crate::ui) fn sync_lyrics_track(&self, query: Option<LyricsQuery>) {
        self.lyrics.set_track(query);
    }

    pub(in crate::ui) fn sync_lyrics_position(&self, position_ms: i64) {
        self.lyrics.set_position(position_ms);
    }

    pub(in crate::ui) fn sync_lyrics_state(&self, state: PlaybackState) {
        self.lyrics.set_playback_state(state);
    }

    pub(in crate::ui) fn set_online_lyrics_enabled(
        self: &Rc<Self>,
        enabled: bool,
    ) -> Result<(), rusqlite::Error> {
        reprise_core::modules::set_enabled(
            &self.conn.borrow(),
            &reprise_core::modules::ONLINE_LYRICS_MODULE,
            enabled,
        )?;
        self.recompute_lyrics_enabled();
        Ok(())
    }

    /// `NET-1a`: re-derives the Lyrics tab's enabled state from the global
    /// online-sources gate ANDed with the Online Lyrics module — called
    /// after either toggles.
    pub(in crate::ui) fn recompute_lyrics_enabled(self: &Rc<Self>) {
        let enabled = reprise_core::online_sources::network_allowed(
            &self.conn.borrow(),
            &reprise_core::modules::ONLINE_LYRICS_MODULE,
        )
        .unwrap_or(false);
        self.lyrics.set_enabled(enabled);
    }
}
