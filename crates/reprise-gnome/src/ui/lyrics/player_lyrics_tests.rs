use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use gtk4::prelude::*;
use reprise_core::lyrics::{LyricsBody, LyricsQuery, TimedLine};
use reprise_core::playback::{AudioEffects, PlaybackBackend, PlaybackError, PlaybackState};
use reprise_core::queries::TrackSummary;

use super::lyrics_view::{LyricsView, ACTIVE_LINE_CLASS};
use super::lyrics_worker::LyricsRuntime;
use super::player_lyrics::{start_track_for_lyrics, PlayerLyrics};

struct FakePlayback {
    result: RefCell<Option<Result<(), PlaybackError>>>,
    play_calls: Cell<usize>,
}

impl FakePlayback {
    fn succeeding() -> Self {
        Self {
            result: RefCell::new(Some(Ok(()))),
            play_calls: Cell::new(0),
        }
    }

    fn failing() -> Self {
        Self {
            result: RefCell::new(Some(Err(PlaybackError::Backend("synthetic".into())))),
            play_calls: Cell::new(0),
        }
    }
}

impl PlaybackBackend for FakePlayback {
    fn play(&self, _path: &str) -> Result<(), PlaybackError> {
        self.play_calls.set(self.play_calls.get() + 1);
        self.result.borrow_mut().take().unwrap()
    }

    fn play_uri(&self, _uri: &str) -> Result<(), PlaybackError> {
        unreachable!()
    }

    fn toggle_pause(&self) -> Result<PlaybackState, PlaybackError> {
        unreachable!()
    }

    fn seek_to(&self, _position_ms: i64) -> Result<(), PlaybackError> {
        unreachable!()
    }

    fn set_volume(&self, _volume: f64) {}

    fn set_audio_effects(&self, _effects: AudioEffects) -> Result<(), PlaybackError> {
        unreachable!()
    }

    fn stop(&self) -> Result<(), PlaybackError> {
        unreachable!()
    }

    fn set_next(&self, _path: Option<&str>) {}

    fn set_transition(
        &self,
        _mode: reprise_core::library::settings::TrackTransition,
        _crossfade_seconds: u8,
    ) {
    }
}

fn summary() -> TrackSummary {
    TrackSummary {
        path: "/synthetic/song.flac".into(),
        title: "Exact title".into(),
        artist: "Exact artist".into(),
        album: "Exact album".into(),
        album_artist: String::new(),
        genre: String::new(),
        artist_mbid: None,
        year: Some(2026),
        duration_ms: 123_456,
    }
}

fn lyrics_query(title: &str) -> LyricsQuery {
    LyricsQuery {
        title: title.into(),
        artist: "Synthetic artist".into(),
        album: "Synthetic album".into(),
        duration_ms: 10_000,
    }
}

#[test]
fn lyr_2_fetch_only_when_enabled() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = LyricsRuntime::setup_with_lookup(Arc::new({
        let calls = calls.clone();
        move |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(LyricsBody::Plain("synthetic lyrics".into()))
        }
    }));
    let lyrics = PlayerLyrics::setup_with_runtime(runtime, false);
    lyrics.set_tab_open(true);

    lyrics.set_track(Some(lyrics_query("Disabled")));
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    lyrics.set_enabled(true);
    for _ in 0..20 {
        if calls.load(Ordering::SeqCst) == 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    lyrics.set_tab_open(false);
    lyrics.set_track(Some(lyrics_query("Closed")));
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    lyrics.set_tab_open(true);
    for _ in 0..20 {
        if calls.load(Ordering::SeqCst) == 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn successful_backend_start_builds_one_exact_lyrics_query() {
    let backend = FakePlayback::succeeding();
    let query = start_track_for_lyrics(&backend, &summary()).unwrap();

    assert_eq!(backend.play_calls.get(), 1);
    assert_eq!(query.title, "Exact title");
    assert_eq!(query.artist, "Exact artist");
    assert_eq!(query.album, "Exact album");
    assert_eq!(query.duration_ms, 123_456);
}

#[test]
fn failed_backend_start_never_produces_a_lyrics_query() {
    let backend = FakePlayback::failing();
    assert!(start_track_for_lyrics(&backend, &summary()).is_err());
    assert_eq!(backend.play_calls.get(), 1);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn runtime_result_tracks_position_and_stop_clears_the_view() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let runtime = LyricsRuntime::setup_with_lookup(Arc::new(|_| {
        Ok(LyricsBody::Synced(vec![
            TimedLine::new(1_000, "first synthetic line"),
            TimedLine::new(2_000, "second synthetic line"),
        ]))
    }));
    let lyrics = PlayerLyrics::setup_with_runtime(runtime, true);
    let view = LyricsView::new();
    view.set_tab_open(true);
    lyrics.set_view(&view);
    lyrics.set_track(Some(LyricsQuery {
        title: "Synthetic title".into(),
        artist: "Synthetic artist".into(),
        album: "Synthetic album".into(),
        duration_ms: 10_000,
    }));

    for _ in 0..100 {
        while gtk4::glib::MainContext::default().iteration(false) {}
        if view.line_labels().len() == 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(view.line_labels().len(), 2);

    lyrics.set_position(1_500);
    let labels = view.line_labels();
    assert!(labels[0].has_css_class(ACTIVE_LINE_CLASS));
    assert!(!labels[1].has_css_class(ACTIVE_LINE_CLASS));
    lyrics.set_position(2_500);
    assert!(!labels[0].has_css_class(ACTIVE_LINE_CLASS));
    assert!(labels[1].has_css_class(ACTIVE_LINE_CLASS));

    lyrics.set_track(None);
    assert!(view.line_labels().is_empty());
    assert_eq!(view.visible_state_name().as_deref(), Some("status"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn playing_synced_lyrics_advance_at_the_line_boundary_without_a_player_tick() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let runtime = LyricsRuntime::setup_with_lookup(Arc::new(|_| {
        Ok(LyricsBody::Synced(vec![
            TimedLine::new(0, "first synthetic line"),
            TimedLine::new(120, "second synthetic line"),
        ]))
    }));
    let lyrics = PlayerLyrics::setup_with_runtime(runtime, true);
    let view = LyricsView::new();
    view.set_tab_open(true);
    lyrics.set_view(&view);
    lyrics.set_track(Some(lyrics_query("Precisely timed")));

    let load_deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while view.line_labels().len() != 2 && std::time::Instant::now() < load_deadline {
        while gtk4::glib::MainContext::default().iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(view.line_labels().len(), 2);

    lyrics.set_position(0);
    lyrics.set_playback_state(PlaybackState::Playing);
    let labels = view.line_labels();
    assert!(labels[0].has_css_class(ACTIVE_LINE_CLASS));

    let line_deadline = std::time::Instant::now() + std::time::Duration::from_millis(400);
    while !labels[1].has_css_class(ACTIVE_LINE_CLASS) && std::time::Instant::now() < line_deadline {
        while gtk4::glib::MainContext::default().iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!labels[0].has_css_class(ACTIVE_LINE_CLASS));
    assert!(labels[1].has_css_class(ACTIVE_LINE_CLASS));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn paused_synced_lyrics_do_not_advance_at_a_scheduled_line_boundary() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let runtime = LyricsRuntime::setup_with_lookup(Arc::new(|_| {
        Ok(LyricsBody::Synced(vec![
            TimedLine::new(0, "first synthetic line"),
            TimedLine::new(150, "second synthetic line"),
        ]))
    }));
    let lyrics = PlayerLyrics::setup_with_runtime(runtime, true);
    let view = LyricsView::new();
    view.set_tab_open(true);
    lyrics.set_view(&view);
    lyrics.set_track(Some(lyrics_query("Paused timing")));

    let load_deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while view.line_labels().len() != 2 && std::time::Instant::now() < load_deadline {
        while gtk4::glib::MainContext::default().iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(view.line_labels().len(), 2);

    lyrics.set_position(0);
    lyrics.set_playback_state(PlaybackState::Playing);
    lyrics.set_playback_state(PlaybackState::Paused);
    let labels = view.line_labels();
    let pause_deadline = std::time::Instant::now() + std::time::Duration::from_millis(300);
    while std::time::Instant::now() < pause_deadline {
        while gtk4::glib::MainContext::default().iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(labels[0].has_css_class(ACTIVE_LINE_CLASS));
    assert!(!labels[1].has_css_class(ACTIVE_LINE_CLASS));
}
