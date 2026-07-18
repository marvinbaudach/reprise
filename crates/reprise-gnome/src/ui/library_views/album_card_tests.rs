use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::playback::PlaybackState;
use reprise_core::queries::AlbumSummary;

use super::album_card::*;
use super::album_card_css as css;
use super::album_card_state::AlbumCardIdentityRegistry;
use crate::ui::cover_loader::CoverLoader;
use crate::ui::eq_bars;

fn wait_for_layout() {
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    gtk4::glib::timeout_add_local_once(Duration::from_millis(50), move || quit.quit());
    main_loop.run();
}

fn descendants_with_class(root: &gtk4::Widget, class: &str) -> Vec<gtk4::Widget> {
    let mut matches = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(widget) = pending.pop() {
        if widget.has_css_class(class) {
            matches.push(widget.clone());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            pending.push(current.clone());
            child = current.next_sibling();
        }
    }
    matches
}

fn shared(now_playing: Option<(&str, &str)>) -> Rc<AlbumCardShared> {
    let (worker, _receiver) = async_channel::unbounded();
    let cover_loader = CoverLoader::new(crate::ui::cover_download_worker::CoverDownloadRuntime {
        enabled: false,
        worker,
    });
    Rc::new(AlbumCardShared {
        cover_loader,
        generation: Rc::new(Cell::new(0)),
        identity_generation: Rc::new(Cell::new(0)),
        identities: Rc::new(RefCell::new(AlbumCardIdentityRegistry::default())),
        playback_state: Rc::new(Cell::new(PlaybackState::Paused)),
        now_playing_album: Rc::new(RefCell::new(
            now_playing.map(|(album, artist)| (album.into(), artist.into())),
        )),
        on_play: Rc::new(RefCell::new(None)),
        on_primary: Rc::new(RefCell::new(None)),
        on_artist_activate: Rc::new(RefCell::new(None)),
    })
}

fn album(title: &str) -> AlbumSummary {
    AlbumSummary {
        album: title.into(),
        album_artist: "Artist".into(),
        representative_path: format!("/nonexistent/{title}.flac"),
        track_count: 1,
        year: None,
        total_duration_ms: 60_000,
        max_added_at: 0,
        total_play_count: 0,
    }
}

#[test]
fn derive_initial_strips_leading_articles() {
    assert_eq!(derive_initial("The Wall"), "W");
    assert_eq!(derive_initial("A Rush of Blood"), "R");
    assert_eq!(derive_initial("Die Ärzte"), "Ä");
}

#[test]
fn derive_initial_uses_first_alphanumeric() {
    assert_eq!(derive_initial("  123 Album"), "1");
    assert_eq!(derive_initial("...Trails"), "T");
}

#[test]
fn derive_initial_fallback_is_music_note() {
    assert_eq!(derive_initial(""), "♪");
    assert_eq!(derive_initial("---"), "♪");
}

#[test]
fn placeholder_gradient_class_is_consistent() {
    let class1 = placeholder_class_for_album("Album", "Artist");
    let class2 = placeholder_class_for_album("Album", "Artist");
    assert_eq!(class1, class2);
    assert!(class1.starts_with("album-placeholder-gradient-"));
}

#[test]
fn placeholder_gradient_class_stays_in_palette() {
    for artist in ["Artist A", "Artist B", "Artist C", "Artist D"] {
        let class = placeholder_class_for_album("Greatest Hits", artist);
        let index = class
            .strip_prefix("album-placeholder-gradient-")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        assert!(index < css::PLACEHOLDER_GRADIENT_COUNT);
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn grid_1_playing_badge_persists_without_hover() {
    gtk4::init().unwrap();
    let settings = gtk4::Settings::default().unwrap();
    let animations_before = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(false);

    let shared = shared(Some(("Playing Album", "Artist")));
    let store = gtk4::gio::ListStore::new::<glib::BoxedAnyObject>();
    for title in ["Playing Album", "Normal Album"] {
        store.append(&glib::BoxedAnyObject::new(album(title)));
    }
    let selection = gtk4::NoSelection::new(Some(store));
    let grid = gtk4::GridView::new(Some(selection), Some(build_factory(&shared)));
    grid.add_css_class("playback-paused");
    let window = gtk4::Window::builder()
        .default_width(500)
        .default_height(300)
        .child(&grid)
        .build();
    window.present();
    wait_for_layout();

    let cards = descendants_with_class(grid.upcast_ref(), css::CARD_CLASS);
    assert_eq!(cards.len(), 2);
    let playing = cards
        .iter()
        .find(|card| {
            descendants_with_class(card, css::TITLE_CLASS)
                .first()
                .and_then(|label| label.clone().downcast::<gtk4::Label>().ok())
                .is_some_and(|label| label.text() == "Playing Album")
        })
        .unwrap();
    let normal = cards.iter().find(|card| *card != playing).unwrap();

    let playing_layers = descendants_with_class(playing, css::PLAYING_LAYER_CLASS);
    assert_eq!(playing_layers.len(), 1);
    assert!(playing_layers[0].is_visible());
    assert_eq!(
        descendants_with_class(playing, css::PLAYING_FRAME_CLASS).len(),
        1
    );
    assert_eq!(
        descendants_with_class(&playing_layers[0], eq_bars::EQ_BARS_CLASS).len(),
        1
    );
    assert!(playing.tooltip_text().is_none());
    assert!(normal.tooltip_text().is_none());
    assert!(descendants_with_class(normal, css::PLAYING_LAYER_CLASS)
        .first()
        .is_none_or(|layer| !layer.is_visible()));
    assert!(grid.has_css_class("playback-paused"));
    assert!(!settings.is_gtk_enable_animations());

    window.close();
    settings.set_gtk_enable_animations(animations_before);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn grid_3_focus_ring_and_overlay_on_focus() {
    gtk4::init().unwrap();
    let shared = shared(Some(("Album 1", "Artist")));
    let store = gtk4::gio::ListStore::new::<glib::BoxedAnyObject>();
    for title in ["Album 1", "Album 2", "Album 3", "Album 4"] {
        store.append(&glib::BoxedAnyObject::new(album(title)));
    }
    let selection = gtk4::NoSelection::new(Some(store));
    let grid = gtk4::GridView::new(Some(selection), Some(build_factory(&shared)));
    grid.add_css_class("library-grid");
    grid.set_min_columns(2);
    grid.set_max_columns(2);
    let window = gtk4::Window::builder()
        .default_width(500)
        .default_height(600)
        .child(&grid)
        .build();
    window.present();
    wait_for_layout();

    grid.scroll_to(0, gtk4::ListScrollFlags::FOCUS, None);
    wait_for_layout();
    let focused_cell = grid.focus_child().expect("native focused grid child");
    let focused_card = descendants_with_class(&focused_cell, css::CARD_CLASS)
        .into_iter()
        .next()
        .expect("focused album card");
    assert_eq!(
        descendants_with_class(&focused_card, css::FOCUS_FRAME_CLASS).len(),
        1
    );
    assert_eq!(
        descendants_with_class(&focused_card, css::HOVER_OVERLAY_CLASS).len(),
        1
    );
    assert_eq!(
        descendants_with_class(&focused_card, css::PLAYING_FRAME_CLASS).len(),
        1,
        "playing and focus frames remain separate on the loaded album"
    );
    assert_eq!(
        crate::ui::album_view_actions::album_key_action(
            gtk4::gdk::Key::Right,
            gtk4::gdk::ModifierType::empty(),
        ),
        crate::ui::album_view_actions::AlbumKeyAction::Propagate,
        "arrow navigation stays native to the two-column GridView"
    );

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn tip_1a_album_card_play_overlay_has_tooltip() {
    if gtk4::init().is_err() {
        return;
    }
    let shared = shared(None);
    let store = gtk4::gio::ListStore::new::<glib::BoxedAnyObject>();
    store.append(&glib::BoxedAnyObject::new(album("Test Album")));
    let selection = gtk4::NoSelection::new(Some(store));
    let grid = gtk4::GridView::new(Some(selection), Some(build_factory(&shared)));
    let window = gtk4::Window::builder().child(&grid).build();
    window.present();
    wait_for_layout();

    let violations = crate::ui::tooltip_discipline::tooltip_violations(grid.upcast_ref());
    assert!(violations.is_empty(), "{violations:?}");
    let play_button = descendants_with_class(grid.upcast_ref(), css::PLAY_BUTTON_CLASS)
        .into_iter()
        .next()
        .and_downcast::<gtk4::Button>()
        .unwrap();
    assert!(!play_button.is_focusable());
    assert_eq!(
        play_button.tooltip_text().as_deref(),
        Some("Play album (Ctrl+Enter)")
    );
    window.close();
}
