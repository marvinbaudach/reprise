use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::BreakpointBinExt;
use reprise_core::playback::{PlaybackState, SpectrumFrame};
use rusqlite::Connection;

use super::cover_loader::CoverLoader;
use super::lyrics_strings;
use super::now_playing_column::NowPlayingColumn;
#[cfg(test)]
use super::now_playing_column::PANEL_WIDTH;
use super::panel_state::*;
use super::song_visualizer::SongVisualizer;
use super::strings;
use super::up_next_panel::UpNextPanel;
use crate::ui::artist_news_worker::ArtistNewsRuntime;
use crate::ui::lyrics_view::LyricsView;
use crate::ui::player_controller::NowPlaying;
use crate::ui::style::tokens;

type OnVoid = Rc<dyn Fn()>;

#[path = "now_playing_effects.rs"]
mod now_playing_effects;

struct PanelWidgets {
    column: NowPlayingColumn,
    stage: gtk4::Box,
    track_content: gtk4::Box,
    lyrics: Rc<LyricsView>,
    up_next: Rc<UpNextPanel>,
    visualizer: SongVisualizer,
    visual_page: adw::ViewStackPage,
    cover: gtk4::Image,
    title: gtk4::Label,
    artist: gtk4::Label,
    album: gtk4::Label,
    // Retained for the tab-session behavior and NPP-10's structural
    // regression test, which proves the active tab stays outside the
    // track-identity crossfade.
    tab_stack: adw::ViewStack,
    #[cfg(test)]
    tab_switcher: adw::InlineViewSwitcher,
    footer: gtk4::Label,
    footers: Rc<RefCell<TabFooters>>,
    session: Rc<TabSession>,
}

fn build_widgets(
    content: &impl IsA<gtk4::Widget>,
    visible: bool,
    conn: Rc<RefCell<Connection>>,
    cover_loader: &Rc<CoverLoader>,
) -> PanelWidgets {
    TAB_SESSION
        .with(|session| build_widgets_for_session(content, visible, session, conn, cover_loader))
}

fn build_widgets_for_session(
    content: &impl IsA<gtk4::Widget>,
    visible: bool,
    session: &Rc<TabSession>,
    conn: Rc<RefCell<Connection>>,
    cover_loader: &Rc<CoverLoader>,
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
    let artist = gtk4::Label::builder()
        .xalign(0.5)
        .justify(gtk4::Justification::Center)
        .wrap(true)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    artist.add_css_class("reprise-now-playing-subtitle");
    let album = gtk4::Label::builder()
        .xalign(0.5)
        .justify(gtk4::Justification::Center)
        .wrap(true)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    album.add_css_class("reprise-now-playing-subtitle");

    let metadata = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    metadata.set_halign(gtk4::Align::Fill);
    metadata.append(&title);
    metadata.append(&artist);
    metadata.append(&album);
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
    let up_next = UpNextPanel::new(conn, cover_loader);
    let visualizer = SongVisualizer::new();
    let tab_stack = adw::ViewStack::builder().vexpand(true).build();
    tab_stack.add_titled_with_icon(
        up_next.widget(),
        Some(UP_NEXT_PAGE),
        &strings::text(strings::UP_NEXT),
        "view-list-symbolic",
    );
    tab_stack.add_titled_with_icon(
        lyrics.widget(),
        Some(LYRICS_PAGE),
        &lyrics_strings::text(lyrics_strings::LYRICS),
        "document-edit-symbolic",
    );
    let visual_page = tab_stack.add_titled_with_icon(
        visualizer.widget(),
        Some(VISUAL_PAGE),
        &strings::text(strings::VISUAL),
        "audio-speakers-symbolic",
    );
    tab_stack.set_visible_child_name(session.selected.get().page_name());
    lyrics.set_tab_open(session.selected.get() == PanelTab::Lyrics);
    let tab_switcher = adw::InlineViewSwitcher::builder()
        .stack(&tab_stack)
        .display_mode(adw::InlineViewSwitcherDisplayMode::Labels)
        .can_shrink(true)
        .homogeneous(true)
        .build();
    tab_switcher.add_css_class("reprise-now-playing-tabs");
    let tabs = adw::BreakpointBin::new();
    tabs.set_child(Some(&tab_switcher));
    let narrow = adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        320.0,
        adw::LengthUnit::Px,
    );
    let breakpoint = adw::Breakpoint::new(narrow);
    breakpoint.add_setter(
        &tab_switcher,
        "display-mode",
        Some(&adw::InlineViewSwitcherDisplayMode::Icons.to_value()),
    );
    tabs.add_breakpoint(breakpoint);

    let footer = gtk4::Label::new(None);
    footer.add_css_class("reprise-now-playing-footer");
    let footers = Rc::new(RefCell::new(TabFooters {
        up_next: super::up_next_panel::format_up_next_footer(&[]),
        lyrics: String::new(),
        visual: String::new(),
    }));
    let initial_footer = match session.selected.get() {
        PanelTab::UpNext => footers.borrow().up_next.clone(),
        PanelTab::Lyrics => footers.borrow().lyrics.clone(),
        PanelTab::Visual => footers.borrow().visual.clone(),
    };
    footer.set_label(&initial_footer);

    {
        let lyrics_weak = Rc::downgrade(&lyrics);
        let footer = footer.clone();
        let footers = footers.clone();
        let session = session.clone();
        lyrics.set_on_footer_changed(move || {
            let Some(lyrics) = lyrics_weak.upgrade() else {
                return;
            };
            let text = lyrics.footer_text();
            footers.borrow_mut().lyrics = text.clone();
            if session.selected.get() == PanelTab::Lyrics {
                footer.set_label(&text);
            }
        });
    }

    {
        let session = session.clone();
        let footer = footer.clone();
        let footers = footers.clone();
        let lyrics = lyrics.clone();
        tab_stack.connect_visible_child_name_notify(move |stack| {
            let Some(name) = stack.visible_child_name() else {
                return;
            };
            let Some(tab) = PanelTab::from_page_name(&name) else {
                return;
            };
            session.selected.set(tab);
            lyrics.set_tab_open(tab == PanelTab::Lyrics);
            let footers = footers.borrow();
            let text = match tab {
                PanelTab::UpNext => &footers.up_next,
                PanelTab::Lyrics => &footers.lyrics,
                PanelTab::Visual => &footers.visual,
            };
            footer.set_label(text);
        });
    }

    let stage = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    stage.add_css_class("reprise-now-playing-stage");
    stage.add_css_class("reprise-now-playing-idle");
    stage.set_vexpand(true);
    let track_content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    track_content.add_css_class("reprise-now-playing-track-content");
    track_content.append(&head_overlay);
    stage.append(&track_content);
    stage.append(&tabs);
    stage.append(&tab_stack);
    stage.append(&footer);

    let toolbar = adw::ToolbarView::new();
    toolbar.set_content(Some(&stage));
    let column = NowPlayingColumn::new(content, &toolbar, visible);
    PanelWidgets {
        column,
        stage,
        track_content,
        lyrics,
        up_next,
        visualizer,
        visual_page,
        cover,
        title,
        artist,
        album,
        tab_stack,
        #[cfg(test)]
        tab_switcher,
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
    on_up_next_refresh: RefCell<Option<OnVoid>>,
    track_animation: RefCell<Option<adw::TimedAnimation>>,
    track_animation_generation: Cell<u64>,
    on_track_reveal: crate::ui::link_activation::ActivationSlot,
    song_visuals_enabled: Cell<bool>,
    on_album_reveal: crate::ui::link_activation::ActivationSlot,
    on_artist_reveal: crate::ui::link_activation::ActivationSlot,
}

impl NowPlayingPanel {
    pub(in crate::ui) fn new(
        content: &impl IsA<gtk4::Widget>,
        conn: Rc<RefCell<Connection>>,
        _runtime: Rc<ArtistNewsRuntime>,
        cover_loader: Rc<CoverLoader>,
    ) -> Rc<Self> {
        let visible = reprise_core::library::settings::get_info_panel_visible(&conn.borrow());
        let song_visuals_enabled = reprise_core::modules::is_enabled(
            &conn.borrow(),
            &reprise_core::modules::SONG_VISUALS_MODULE,
        )
        .unwrap_or(reprise_core::modules::SONG_VISUALS_MODULE.default_enabled);
        let panel = Rc::new(Self {
            widgets: build_widgets(content, visible, conn.clone(), &cover_loader),
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
            on_up_next_refresh: RefCell::new(None),
            track_animation: RefCell::new(None),
            track_animation_generation: Cell::new(0),
            on_track_reveal: Rc::new(RefCell::new(None)),
            song_visuals_enabled: Cell::new(song_visuals_enabled),
            on_album_reveal: Rc::new(RefCell::new(None)),
            on_artist_reveal: Rc::new(RefCell::new(None)),
        });
        crate::ui::link_activation::arm_slot(
            &panel.widgets.cover,
            &crate::ui::strings::text(crate::ui::strings::GO_TO_PLAYING_ALBUM),
            &panel.on_album_reveal,
        );
        crate::ui::link_activation::arm_slot(
            &panel.widgets.title,
            &crate::ui::strings::text(crate::ui::strings::REVEAL_PLAYING_TRACK),
            &panel.on_track_reveal,
        );
        crate::ui::link_activation::arm_slot(
            &panel.widgets.artist,
            &crate::ui::strings::text(crate::ui::strings::GO_TO_PLAYING_ARTIST),
            &panel.on_artist_reveal,
        );
        crate::ui::link_activation::arm_slot(
            &panel.widgets.album,
            &crate::ui::strings::text(crate::ui::strings::GO_TO_PLAYING_ALBUM),
            &panel.on_album_reveal,
        );
        panel.set_song_visuals_enabled(song_visuals_enabled);
        panel.wire();
        panel.sync_visual_activity();
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
        self.widgets.tab_stack.set_visible_child_name(LYRICS_PAGE);
        self.widgets.column.set_visible(true);
    }

    pub(in crate::ui) fn apply_persisted_visibility(&self, visible: bool) {
        self.syncing_visibility.set(true);
        self.widgets.column.set_visible(visible);
        self.toggle.set_active(visible);
        self.syncing_visibility.set(false);
        self.request_up_next_refresh_if_visible();
        self.sync_visual_activity();
    }

    pub(in crate::ui) fn set_loaded_track(self: &Rc<Self>, track: Option<NowPlaying>) {
        let (changed, id_changed) = {
            let current = self.loaded_track.borrow();
            match (current.as_ref(), track.as_ref()) {
                (Some(current), Some(next)) => (
                    current.id != next.id || current.path != next.path,
                    current.id != next.id,
                ),
                (None, None) => (false, false),
                _ => (true, true),
            }
        };
        *self.loaded_track.borrow_mut() = track;
        if id_changed {
            // A new track started: reset the visual engine's clock, water
            // surface, and impact overlay so ripples/sparks from the
            // previous track don't bleed into the new one.
            self.widgets.visualizer.note_track_changed();
        }
        if !changed {
            if self.track_animation.borrow().is_none() {
                self.render_track();
            }
            return;
        }
        if !crate::ui::motion::animations_enabled() {
            self.cancel_track_animation();
            self.widgets.track_content.set_opacity(1.0);
            self.render_track();
            return;
        }
        self.animate_track_change();
    }

    pub(in crate::ui) fn set_playback_state(&self, state: PlaybackState) {
        self.playback_state.set(state);
        self.widgets.visualizer.set_playback_state(state);
    }

    pub(in crate::ui) fn set_spectrum(&self, frame: SpectrumFrame) {
        if self.song_visuals_enabled.get() {
            self.widgets.visualizer.set_spectrum(frame);
        }
    }

    pub(in crate::ui) fn set_song_visuals_enabled(&self, enabled: bool) {
        self.song_visuals_enabled.set(enabled);
        self.widgets.visual_page.set_visible(enabled);
        if !enabled {
            self.widgets.visualizer.set_active(false);
            if self.widgets.session.selected.get() == PanelTab::Visual {
                self.widgets.tab_stack.set_visible_child_name(UP_NEXT_PAGE);
            }
        }
        self.sync_visual_activity();
    }

    pub(in crate::ui) fn set_up_next_model(
        &self,
        model: &crate::ui::track_list::queue_sections::QueueViewModel,
    ) {
        let text = self.widgets.up_next.set_queue_model(model);
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

    pub(in crate::ui) fn set_on_up_next_remove(
        &self,
        callback: impl Fn(crate::ui::track_list::queue_row_mapping::QueueRow) + 'static,
    ) {
        self.widgets.up_next.set_on_remove(callback);
    }

    pub(in crate::ui) fn set_on_up_next_reorder(
        &self,
        callback: impl Fn(
                crate::ui::track_list::queue_row_mapping::QueueRow,
                crate::ui::track_list::queue_row_mapping::QueueRow,
            ) + 'static,
    ) {
        self.widgets.up_next.set_on_reorder(callback);
    }

    pub(in crate::ui) fn set_on_up_next_refresh(&self, callback: impl Fn() + 'static) {
        *self.on_up_next_refresh.borrow_mut() = Some(Rc::new(callback));
        self.request_up_next_refresh_if_visible();
    }

    pub(in crate::ui) fn is_up_next_visible(&self) -> bool {
        should_render_up_next(
            self.widgets.column.is_visible(),
            self.widgets.session.selected.get(),
        )
    }

    pub(in crate::ui) fn set_on_album_reveal(&self, callback: impl Fn() + 'static) {
        *self.on_album_reveal.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_track_reveal(&self, callback: impl Fn() + 'static) {
        *self.on_track_reveal.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_artist_reveal(&self, callback: impl Fn() + 'static) {
        *self.on_artist_reveal.borrow_mut() = Some(Rc::new(callback));
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
            .tab_stack
            .connect_visible_child_name_notify(move |stack| {
                if let Some(panel) = weak.upgrade() {
                    panel.sync_visual_activity();
                    if stack.visible_child_name().as_deref() == Some(UP_NEXT_PAGE) {
                        panel.request_up_next_refresh_if_visible();
                    }
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
                panel.request_up_next_refresh_if_visible();
                panel.sync_visual_activity();
            });
    }

    fn sync_visual_activity(&self) {
        self.widgets.visualizer.set_active(
            self.song_visuals_enabled.get()
                && self.widgets.column.is_visible()
                && self.widgets.session.selected.get() == PanelTab::Visual,
        );
    }

    fn request_up_next_refresh_if_visible(&self) {
        if !self.is_up_next_visible() {
            return;
        }
        let callback = self.on_up_next_refresh.borrow().clone();
        if let Some(callback) = callback {
            callback();
        }
    }
}

pub(in crate::ui) fn css() -> String {
    super::surface_css::css()
}

#[cfg(test)]
#[path = "now_playing_tests.rs"]
mod tests;
