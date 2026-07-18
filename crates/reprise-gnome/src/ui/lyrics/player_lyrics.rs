//! Current-playback fan-out for the Lyrics page.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use gtk4::glib;
use reprise_core::lyrics::{LyricsBody, LyricsQuery};
use reprise_core::playback::{PlaybackBackend, PlaybackError};
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
}

impl PlayerLyrics {
    pub(in crate::ui) fn new() -> Rc<Self> {
        Self::with_runtime(LyricsRuntime::setup())
    }

    fn with_runtime(runtime: Rc<LyricsRuntime>) -> Rc<Self> {
        Rc::new(Self {
            runtime,
            state: RefCell::new(LyricsState::default()),
            view: RefCell::new(Weak::new()),
            position_ms: Cell::new(0),
        })
    }

    #[cfg(test)]
    pub(in crate::ui) fn setup_with_runtime(runtime: Rc<LyricsRuntime>) -> Rc<Self> {
        Self::with_runtime(runtime)
    }

    pub(in crate::ui) fn set_view(self: &Rc<Self>, view: &Rc<LyricsView>) {
        *self.view.borrow_mut() = Rc::downgrade(view);
        let lyrics = Rc::downgrade(self);
        view.set_on_retry(move || {
            if let Some(lyrics) = lyrics.upgrade() {
                lyrics.retry();
            }
        });
        self.render_current();
    }

    pub(in crate::ui) fn set_track(self: &Rc<Self>, query: Option<LyricsQuery>) {
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
        }
    }

    pub(in crate::ui) fn set_position(&self, position_ms: i64) {
        let position_ms = position_ms.max(0);
        self.position_ms.set(position_ms);
        let (active, timestamp_ms) = {
            let mut state = self.state.borrow_mut();
            state.update_position(position_ms);
            (state.active_line(), state.active_line_timestamp_ms())
        };
        if let Some(view) = self.view() {
            view.set_active_line_at(active, timestamp_ms, position_ms);
        }
    }

    fn retry(self: &Rc<Self>) {
        let intent = self.state.borrow_mut().retry();
        if let Some(intent) = intent {
            self.start_request(intent);
        }
    }

    fn start_request(self: &Rc<Self>, intent: RequestIntent) {
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

    fn apply_response(&self, response: LyricsResponse) {
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
                if let Some(view) = self.view() {
                    view.show_error(&error);
                }
            }
        }
    }

    fn apply_body(&self, body: &LyricsBody) {
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
    }

    fn render_current(&self) {
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
            (Some(query), None) => view.show_loading(&query.title, &query.artist),
            (None, None) => view.show_empty(),
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
    pub(in crate::ui) fn set_lyrics_view(&self, view: &Rc<LyricsView>) {
        self.lyrics.set_view(view);
    }

    pub(in crate::ui) fn sync_lyrics_track(&self, query: Option<LyricsQuery>) {
        self.lyrics.set_track(query);
    }

    pub(in crate::ui) fn sync_lyrics_position(&self, position_ms: i64) {
        self.lyrics.set_position(position_ms);
    }
}
