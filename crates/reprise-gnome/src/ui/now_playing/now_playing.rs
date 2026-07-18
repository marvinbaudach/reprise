use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::cover::ThumbnailSize;
use reprise_core::playback::PlaybackState;
use rusqlite::Connection;

use super::artist_portrait_worker::ArtistPortraitRuntime;
use super::cover_loader::CoverLoader;
use super::now_playing_column::NowPlayingColumn;
#[cfg(test)]
use super::now_playing_column::PANEL_WIDTH;
use super::strings;
use crate::ui::artist_news_worker::ArtistNewsRuntime;
use crate::ui::lyrics_view::LyricsView;
use crate::ui::player_controller::NowPlaying;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PanelPresentation {
    title: String,
    subtitle: String,
    idle: bool,
}

fn panel_presentation(
    track: Option<&NowPlaying>,
    _playback_state: PlaybackState,
) -> PanelPresentation {
    let Some(track) = track else {
        return PanelPresentation {
            title: strings::text(strings::NOW_PLAYING_NOTHING),
            subtitle: String::new(),
            idle: true,
        };
    };
    let subtitle = match (track.artist.trim(), track.album.trim()) {
        ("", "") => String::new(),
        (artist, "") => artist.to_owned(),
        ("", album) => album.to_owned(),
        (artist, album) => format!("{artist} · {album}"),
    };
    PanelPresentation {
        title: track.title.clone(),
        subtitle,
        idle: false,
    }
}

struct PanelWidgets {
    column: NowPlayingColumn,
    lyrics: Rc<LyricsView>,
    cover: gtk4::Image,
    title: gtk4::Label,
    subtitle: gtk4::Label,
}

fn build_widgets(content: &impl IsA<gtk4::Widget>, visible: bool) -> PanelWidgets {
    let cover = gtk4::Image::builder()
        .pixel_size(96)
        .width_request(96)
        .height_request(96)
        .build();
    CoverLoader::set_placeholder(&cover);

    let title = gtk4::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    title.add_css_class("title-4");
    let subtitle = gtk4::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    subtitle.add_css_class("dim-label");

    let metadata = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    metadata.set_hexpand(true);
    metadata.append(&title);
    metadata.append(&subtitle);
    let head = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    head.add_css_class("card");
    head.set_margin_top(12);
    head.set_margin_bottom(6);
    head.set_margin_start(12);
    head.set_margin_end(12);
    head.append(&cover);
    head.append(&metadata);

    let lyrics = LyricsView::new();
    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    body.set_vexpand(true);
    body.append(&head);
    body.append(lyrics.widget());

    let toolbar = adw::ToolbarView::new();
    toolbar.set_content(Some(&body));
    let column = NowPlayingColumn::new(content, &toolbar, visible);
    PanelWidgets {
        column,
        lyrics,
        cover,
        title,
        subtitle,
    }
}

pub(in crate::ui) struct NowPlayingPanel {
    widgets: PanelWidgets,
    toggle: gtk4::ToggleButton,
    conn: Rc<RefCell<Connection>>,
    cover_loader: Rc<CoverLoader>,
    cover_generation: Rc<Cell<u64>>,
    loaded_track: RefCell<Option<NowPlaying>>,
    playback_state: Cell<PlaybackState>,
    syncing_visibility: Cell<bool>,
}

impl NowPlayingPanel {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::ui) fn new(
        content: &impl IsA<gtk4::Widget>,
        _window: &adw::ApplicationWindow,
        conn: Rc<RefCell<Connection>>,
        _runtime: Rc<ArtistNewsRuntime>,
        _portraits: &Rc<ArtistPortraitRuntime>,
        cover_loader: Rc<CoverLoader>,
    ) -> Rc<Self> {
        let visible = reprise_core::library::settings::get_info_panel_visible(&conn.borrow());
        let panel = Rc::new(Self {
            widgets: build_widgets(content, visible),
            toggle: gtk4::ToggleButton::builder()
                .icon_name("sidebar-show-right-symbolic")
                .tooltip_text(strings::text(strings::INFO_PANEL_TOGGLE))
                .css_classes(["flat", "reprise-panel-toggle"])
                .active(visible)
                .build(),
            conn,
            cover_loader,
            cover_generation: Rc::new(Cell::new(0)),
            loaded_track: RefCell::new(None),
            playback_state: Cell::new(PlaybackState::Stopped),
            syncing_visibility: Cell::new(false),
        });
        panel.wire();
        panel.render_track();
        panel
    }

    pub(in crate::ui) fn widget(&self) -> &adw::OverlaySplitView {
        self.widgets.column.widget()
    }

    pub(in crate::ui) fn toggle_button(&self) -> gtk4::ToggleButton {
        self.toggle.clone()
    }

    pub(in crate::ui) fn lyrics_view(&self) -> Rc<LyricsView> {
        self.widgets.lyrics.clone()
    }

    pub(in crate::ui) fn show_lyrics(&self) {
        self.widgets.column.set_visible(true);
    }

    pub(in crate::ui) fn apply_persisted_visibility(&self, visible: bool) {
        self.syncing_visibility.set(true);
        self.widgets.column.set_visible(visible);
        self.toggle.set_active(visible);
        self.syncing_visibility.set(false);
    }

    pub(in crate::ui) fn set_loaded_track(&self, track: Option<NowPlaying>) {
        *self.loaded_track.borrow_mut() = track;
        self.render_track();
    }

    pub(in crate::ui) fn set_playback_state(&self, state: PlaybackState) {
        self.playback_state.set(state);
        self.render_track();
    }

    /// Keeps the callback owner alive for exactly as long as the window.
    pub(in crate::ui) fn retain_for_window(self: &Rc<Self>, window: &adw::ApplicationWindow) {
        let panel = self.clone();
        window.connect_destroy(move |_| {
            let _keep_alive_until_destroy = &panel;
        });
    }

    fn wire(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.toggle.connect_toggled(move |button| {
            let Some(panel) = weak.upgrade() else { return };
            if !panel.syncing_visibility.get() {
                panel.widgets.column.set_visible(button.is_active());
            }
        });

        let weak = Rc::downgrade(self);
        self.widgets
            .column
            .widget()
            .connect_show_sidebar_notify(move |split| {
                let Some(panel) = weak.upgrade() else { return };
                let visible = split.shows_sidebar();
                let was_syncing = panel.syncing_visibility.get();
                panel.syncing_visibility.set(true);
                panel.toggle.set_active(visible);
                panel.syncing_visibility.set(was_syncing);
                if was_syncing {
                    return;
                }
                let saved = {
                    let conn = panel.conn.borrow();
                    reprise_core::library::settings::set_info_panel_visible(&conn, visible)
                };
                if let Err(error) = saved {
                    tracing::warn!(%error, "could not save now-playing panel visibility");
                }
            });
    }

    fn render_track(&self) {
        let track = self.loaded_track.borrow().clone();
        let presentation = panel_presentation(track.as_ref(), self.playback_state.get());
        self.widgets.title.set_label(&presentation.title);
        self.widgets.subtitle.set_label(&presentation.subtitle);
        self.widgets.subtitle.set_visible(!presentation.idle);

        let generation = self.cover_generation.get().wrapping_add(1);
        self.cover_generation.set(generation);
        CoverLoader::set_placeholder(&self.widgets.cover);
        if let Some(track) = track {
            self.cover_loader.load_into(
                &self.widgets.cover,
                &track.path,
                ThumbnailSize::Bar,
                generation,
                &self.cover_generation,
            );
        }
    }
}

#[cfg(test)]
#[path = "now_playing_tests.rs"]
mod tests;
