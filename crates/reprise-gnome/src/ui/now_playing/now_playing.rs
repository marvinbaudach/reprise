use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::AnimationExt;
use reprise_core::cover::ThumbnailSize;
use reprise_core::playback::PlaybackState;
use rusqlite::Connection;

use super::artist_portrait_worker::ArtistPortraitRuntime;
use super::cover_loader::CoverLoader;
use super::lyrics_strings;
use super::now_playing_column::NowPlayingColumn;
#[cfg(test)]
use super::now_playing_column::PANEL_WIDTH;
use super::panel_state::*;
use super::strings;
use super::up_next_panel::UpNextPanel;
use crate::ui::artist_news_worker::ArtistNewsRuntime;
use crate::ui::lyrics_view::LyricsView;
use crate::ui::player_controller::NowPlaying;
use crate::ui::style::tokens;

type OnVoid = Rc<dyn Fn()>;

struct PanelWidgets {
    column: NowPlayingColumn,
    stage: gtk4::Box,
    track_content: gtk4::Box,
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
    let up_next = UpNextPanel::new(conn, cover_loader);
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
    let track_content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    track_content.add_css_class("reprise-now-playing-track-content");
    track_content.set_vexpand(true);
    track_content.append(&head_overlay);
    track_content.append(&tabs);
    track_content.append(&tab_stack);
    track_content.append(&footer);
    stage.append(&track_content);

    let toolbar = adw::ToolbarView::new();
    toolbar.set_content(Some(&stage));
    let column = NowPlayingColumn::new(content, &toolbar, visible);
    PanelWidgets {
        column,
        stage,
        track_content,
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
    on_up_next_refresh: RefCell<Option<OnVoid>>,
    track_animation: RefCell<Option<adw::TimedAnimation>>,
    track_animation_generation: Cell<u64>,
    on_album_reveal: crate::ui::link_activation::ActivationSlot,
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
            on_album_reveal: Rc::new(RefCell::new(None)),
        });
        crate::ui::link_activation::arm_slot(&panel.widgets.cover, &panel.on_album_reveal);
        crate::ui::link_activation::arm_slot(&panel.widgets.title, &panel.on_album_reveal);
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
        self.request_up_next_refresh_if_visible();
    }

    pub(in crate::ui) fn set_loaded_track(self: &Rc<Self>, track: Option<NowPlaying>) {
        let changed = {
            let current = self.loaded_track.borrow();
            match (current.as_ref(), track.as_ref()) {
                (Some(current), Some(next)) => current.id != next.id || current.path != next.path,
                (None, None) => false,
                _ => true,
            }
        };
        *self.loaded_track.borrow_mut() = track;
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
        self.widgets.tab_buttons[0].connect_toggled(move |button| {
            if button.is_active() {
                if let Some(panel) = weak.upgrade() {
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
            });
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

    fn animate_track_change(self: &Rc<Self>) {
        self.cancel_track_animation();
        let generation = self.track_animation_generation.get().wrapping_add(1);
        self.track_animation_generation.set(generation);
        let target = adw::CallbackAnimationTarget::new({
            let content = self.widgets.track_content.clone();
            move |value| content.set_opacity(value)
        });
        let fade_out = crate::ui::motion::timed(
            &self.widgets.track_content,
            self.widgets.track_content.opacity(),
            0.0,
            crate::ui::motion::STANDARD,
            target,
        );
        fade_out.set_duration(crate::ui::motion::half(crate::ui::motion::STANDARD));
        let panel = Rc::downgrade(self);
        fade_out.connect_done(move |_| {
            let Some(panel) = panel.upgrade() else {
                return;
            };
            if panel.track_animation_generation.get() != generation {
                return;
            }
            panel.render_track();
            let target = adw::CallbackAnimationTarget::new({
                let content = panel.widgets.track_content.clone();
                move |value| content.set_opacity(value)
            });
            let fade_in = crate::ui::motion::timed(
                &panel.widgets.track_content,
                0.0,
                1.0,
                crate::ui::motion::STANDARD,
                target,
            );
            fade_in.set_duration(crate::ui::motion::half(crate::ui::motion::STANDARD));
            let panel_for_done = Rc::downgrade(&panel);
            fade_in.connect_done(move |_| {
                let Some(panel) = panel_for_done.upgrade() else {
                    return;
                };
                if panel.track_animation_generation.get() == generation {
                    panel.track_animation.borrow_mut().take();
                    panel.widgets.track_content.set_opacity(1.0);
                }
            });
            *panel.track_animation.borrow_mut() = Some(fade_in.clone());
            fade_in.play();
        });
        *self.track_animation.borrow_mut() = Some(fade_out.clone());
        fade_out.play();
    }

    fn cancel_track_animation(&self) {
        self.track_animation_generation
            .set(self.track_animation_generation.get().wrapping_add(1));
        if let Some(animation) = self.track_animation.borrow_mut().take() {
            animation.pause();
        }
    }

    #[cfg(test)]
    fn has_track_animation(&self) -> bool {
        self.track_animation.borrow().is_some()
    }
}

pub(in crate::ui) fn css() -> String {
    use tokens::{
        NOW_PLAYING_FOOTER_ALPHA, NOW_PLAYING_FOOTER_SIZE, NOW_PLAYING_GLOW_ALPHA,
        NOW_PLAYING_PILL_ACTIVE_ALPHA, NOW_PLAYING_PILL_BG_ALPHA, NOW_PLAYING_PILL_RADIUS,
        NOW_PLAYING_SUBTITLE_ALPHA, NOW_PLAYING_SUBTITLE_SIZE, NOW_PLAYING_TITLE_SIZE,
        RADIUS_SURFACE,
    };

    format!(
        ".reprise-now-playing-stage {{ \
       background-color: @sidebar_bg_color; color: #ffffff; min-width: 300px; \
       border-left: 1px solid rgba(255, 255, 255, 0.06); }}\n\
     .reprise-now-playing-glow {{ \
       min-height: 300px; \
       background-image: radial-gradient(ellipse at center, \
         alpha(@reprise_player_accent, {NOW_PLAYING_GLOW_ALPHA}) 0%, \
         alpha(@sidebar_bg_color, 0) 70%); }}\n\
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
