use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::cover::ThumbnailSize;
use reprise_core::playback::PlaybackState;
use rusqlite::Connection;

use super::artist_portrait_worker::ArtistPortraitRuntime;
use super::cover_loader::CoverLoader;
use super::lyrics_strings;
use super::now_playing_column::NowPlayingColumn;
#[cfg(test)]
use super::now_playing_column::PANEL_WIDTH;
use super::strings;
use super::up_next_panel::{UpNextEntry, UpNextPanel};
use crate::ui::artist_news_worker::ArtistNewsRuntime;
use crate::ui::lyrics_view::LyricsView;
use crate::ui::player_controller::NowPlaying;
use crate::ui::style::tokens;

const UP_NEXT_PAGE: &str = "up-next";
const LYRICS_PAGE: &str = "lyrics";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum PanelTab {
    #[default]
    UpNext,
    Lyrics,
}

impl PanelTab {
    fn page_name(self) -> &'static str {
        match self {
            Self::UpNext => UP_NEXT_PAGE,
            Self::Lyrics => LYRICS_PAGE,
        }
    }
}

#[derive(Default)]
struct TabSession {
    selected: Cell<PanelTab>,
}

#[derive(Default)]
struct TabFooters {
    up_next: String,
    lyrics: String,
}

thread_local! {
    static TAB_SESSION: Rc<TabSession> = Rc::new(TabSession::default());
}

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
    stage: gtk4::Box,
    lyrics: Rc<LyricsView>,
    up_next: Rc<UpNextPanel>,
    cover: gtk4::Image,
    title: gtk4::Label,
    subtitle: gtk4::Label,
    // Retained for T9's shared track-content crossfade; the T5 acceptance
    // test also inspects the selected page directly.
    #[allow(dead_code)]
    tab_stack: gtk4::Stack,
    tab_buttons: [gtk4::ToggleButton; 2],
    footer: gtk4::Label,
    footers: Rc<RefCell<TabFooters>>,
    session: Rc<TabSession>,
}

fn build_widgets(content: &impl IsA<gtk4::Widget>, visible: bool) -> PanelWidgets {
    TAB_SESSION.with(|session| build_widgets_for_session(content, visible, session))
}

fn build_widgets_for_session(
    content: &impl IsA<gtk4::Widget>,
    visible: bool,
    session: &Rc<TabSession>,
) -> PanelWidgets {
    let cover = gtk4::Image::builder()
        .pixel_size(tokens::NOW_PLAYING_COVER_SIZE)
        .width_request(tokens::NOW_PLAYING_COVER_SIZE)
        .height_request(tokens::NOW_PLAYING_COVER_SIZE)
        .build();
    cover.add_css_class("reprise-now-playing-cover");
    CoverLoader::set_placeholder(&cover);

    let title = gtk4::Label::builder()
        .xalign(0.5)
        .justify(gtk4::Justification::Center)
        .wrap(true)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    title.add_css_class("reprise-now-playing-title");
    let subtitle = gtk4::Label::builder()
        .xalign(0.5)
        .justify(gtk4::Justification::Center)
        .wrap(true)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    subtitle.add_css_class("reprise-now-playing-subtitle");

    let metadata = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    metadata.set_halign(gtk4::Align::Fill);
    metadata.append(&title);
    metadata.append(&subtitle);
    let head = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    head.add_css_class("reprise-now-playing-head");
    head.set_halign(gtk4::Align::Center);
    head.set_valign(gtk4::Align::Center);
    head.append(&cover);
    head.append(&metadata);

    let glow = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    glow.add_css_class("reprise-now-playing-glow");
    glow.set_can_target(false);
    let head_overlay = gtk4::Overlay::new();
    head_overlay.set_child(Some(&glow));
    head_overlay.add_overlay(&head);

    let lyrics = LyricsView::new();
    let up_next = UpNextPanel::new();
    let tab_stack = gtk4::Stack::builder()
        .transition_type(gtk4::StackTransitionType::Crossfade)
        .transition_duration(crate::ui::motion::STANDARD_MS)
        .vexpand(true)
        .build();
    tab_stack.add_named(up_next.widget(), Some(UP_NEXT_PAGE));
    tab_stack.add_named(lyrics.widget(), Some(LYRICS_PAGE));
    tab_stack.set_visible_child_name(session.selected.get().page_name());

    let up_next_button = gtk4::ToggleButton::with_label(&strings::text(strings::UP_NEXT));
    let lyrics_button =
        gtk4::ToggleButton::with_label(&lyrics_strings::text(lyrics_strings::LYRICS));
    lyrics_button.set_group(Some(&up_next_button));
    for button in [&up_next_button, &lyrics_button] {
        button.add_css_class("reprise-now-playing-tab");
    }
    match session.selected.get() {
        PanelTab::UpNext => up_next_button.set_active(true),
        PanelTab::Lyrics => lyrics_button.set_active(true),
    }
    let tabs = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    tabs.set_homogeneous(true);
    tabs.set_halign(gtk4::Align::Center);
    tabs.add_css_class("reprise-now-playing-tabs");
    tabs.append(&up_next_button);
    tabs.append(&lyrics_button);

    let footer = gtk4::Label::new(None);
    footer.add_css_class("reprise-now-playing-footer");
    let footers = Rc::new(RefCell::new(TabFooters {
        up_next: super::up_next_panel::format_up_next_footer(&[]),
        lyrics: String::new(),
    }));
    let initial_footer = match session.selected.get() {
        PanelTab::UpNext => footers.borrow().up_next.clone(),
        PanelTab::Lyrics => footers.borrow().lyrics.clone(),
    };
    footer.set_label(&initial_footer);

    {
        let stack = tab_stack.clone();
        let session = session.clone();
        let footer = footer.clone();
        let footers = footers.clone();
        up_next_button.connect_toggled(move |button| {
            if button.is_active() {
                session.selected.set(PanelTab::UpNext);
                stack.set_visible_child_name(UP_NEXT_PAGE);
                let text = footers.borrow().up_next.clone();
                footer.set_label(&text);
            }
        });
    }
    {
        let stack = tab_stack.clone();
        let session = session.clone();
        let footer = footer.clone();
        let footers = footers.clone();
        lyrics_button.connect_toggled(move |button| {
            if button.is_active() {
                session.selected.set(PanelTab::Lyrics);
                stack.set_visible_child_name(LYRICS_PAGE);
                let text = footers.borrow().lyrics.clone();
                footer.set_label(&text);
            }
        });
    }

    let stage = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    stage.add_css_class("reprise-now-playing-stage");
    stage.add_css_class("reprise-now-playing-idle");
    stage.set_vexpand(true);
    stage.append(&head_overlay);
    stage.append(&tabs);
    stage.append(&tab_stack);
    stage.append(&footer);

    let toolbar = adw::ToolbarView::new();
    toolbar.set_content(Some(&stage));
    let column = NowPlayingColumn::new(content, &toolbar, visible);
    PanelWidgets {
        column,
        stage,
        lyrics,
        up_next,
        cover,
        title,
        subtitle,
        tab_stack,
        tab_buttons: [up_next_button, lyrics_button],
        footer,
        footers,
        session: session.clone(),
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
        self.widgets.tab_buttons[1].set_active(true);
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

    pub(in crate::ui) fn set_up_next_entries(&self, entries: &[UpNextEntry]) {
        let text = self
            .widgets
            .up_next
            .set_entries(entries, &self.cover_loader);
        self.widgets.footers.borrow_mut().up_next = text.clone();
        if self.widgets.session.selected.get() == PanelTab::UpNext {
            self.widgets.footer.set_label(&text);
        }
    }

    pub(in crate::ui) fn set_on_up_next_jump(
        &self,
        callback: impl Fn(crate::ui::track_list::queue_row_mapping::QueueRow) + 'static,
    ) {
        self.widgets.up_next.set_on_jump(callback);
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
        if presentation.idle {
            self.widgets.stage.add_css_class("reprise-now-playing-idle");
        } else {
            self.widgets
                .stage
                .remove_css_class("reprise-now-playing-idle");
        }
        let generation = self.cover_generation.get().wrapping_add(1);
        self.cover_generation.set(generation);
        CoverLoader::set_placeholder(&self.widgets.cover);
        if let Some(track) = track {
            self.cover_loader.load_into(
                &self.widgets.cover,
                &track.path,
                ThumbnailSize::Full,
                generation,
                &self.cover_generation,
            );
        }
    }
}

pub(in crate::ui) fn css() -> String {
    use tokens::{
        NOW_PLAYING_FOOTER_ALPHA, NOW_PLAYING_FOOTER_SIZE, NOW_PLAYING_GLOW_ALPHA,
        NOW_PLAYING_PILL_ACTIVE_ALPHA, NOW_PLAYING_PILL_BG_ALPHA, NOW_PLAYING_PILL_RADIUS,
        NOW_PLAYING_STAGE_BG, NOW_PLAYING_SUBTITLE_ALPHA, NOW_PLAYING_SUBTITLE_SIZE,
        NOW_PLAYING_TITLE_SIZE, RADIUS_SURFACE,
    };

    format!(
        ".reprise-now-playing-stage {{ \
       background-color: {NOW_PLAYING_STAGE_BG}; color: #ffffff; min-width: 300px; }}\n\
     .reprise-now-playing-glow {{ \
       min-height: 300px; \
       background-image: radial-gradient(ellipse at center, \
         alpha(@reprise_player_accent, {NOW_PLAYING_GLOW_ALPHA}) 0%, \
         alpha({NOW_PLAYING_STAGE_BG}, 0) 70%); }}\n\
     .reprise-now-playing-idle .reprise-now-playing-glow {{ \
       background-image: none; }}\n\
     .reprise-now-playing-head {{ padding: 22px 18px 16px; }}\n\
     .reprise-now-playing-cover {{ \
       border-radius: {RADIUS_SURFACE}; \
       box-shadow: 0 10px 30px rgba(0, 0, 0, 0.45), \
                   inset 0 0 0 1px alpha(#ffffff, 0.12); }}\n\
     .reprise-now-playing-title {{ \
       color: #ffffff; font-size: {NOW_PLAYING_TITLE_SIZE}; font-weight: 700; }}\n\
     .reprise-now-playing-subtitle {{ \
       color: alpha(#ffffff, {NOW_PLAYING_SUBTITLE_ALPHA}); \
       font-size: {NOW_PLAYING_SUBTITLE_SIZE}; }}\n\
     .reprise-now-playing-tabs {{ \
       background-color: alpha(#ffffff, {NOW_PLAYING_PILL_BG_ALPHA}); \
       border-radius: {NOW_PLAYING_PILL_RADIUS}; \
       padding: 2px; margin: 0 18px 12px; }}\n\
     .reprise-now-playing-tabs > .reprise-now-playing-tab {{ \
       background-color: transparent; background-image: none; \
       border: none; border-radius: {NOW_PLAYING_PILL_RADIUS}; box-shadow: none; \
       color: alpha(#ffffff, {NOW_PLAYING_SUBTITLE_ALPHA}); min-height: 0; \
       padding: 5px 18px; }}\n\
     .reprise-now-playing-tabs > .reprise-now-playing-tab:checked {{ \
       background-color: alpha(#ffffff, {NOW_PLAYING_PILL_ACTIVE_ALPHA}); \
       color: #ffffff; font-weight: 700; }}\n\
     .reprise-now-playing-footer {{ \
       color: alpha(#ffffff, {NOW_PLAYING_FOOTER_ALPHA}); \
       font-size: {NOW_PLAYING_FOOTER_SIZE}; \
       min-height: 14px; margin: 8px 12px 12px; }}"
    )
}

#[cfg(test)]
#[path = "now_playing_tests.rs"]
mod tests;
