use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::playback::{PlaybackState, SpectrumFrame};

use super::cover_bloom;
use super::cover_loader::CoverLoader;
use super::cover_shimmer;
use super::lyrics_strings;
use super::now_playing_column::NowPlayingColumn;
#[cfg(test)]
use super::now_playing_column::PANEL_WIDTH;
use super::panel_state::*;
use super::song_visualizer::SongVisualizer;
use super::strings;
use super::up_next_panel::UpNextPanel;
use crate::ui::artist_news_worker::ArtistNewsRuntime;
use crate::ui::cover_lift::CoverLift;
use crate::ui::lyrics_view::LyricsView;
use crate::ui::playback::external_media::ExternalPlaybackSnapshot;
use crate::ui::player_controller::NowPlaying;
use crate::ui::playing_links::LinkLabels;
use crate::ui::style::tokens;
use crate::ui::swell::Swell;

type OnVoid = Rc<dyn Fn()>;
const TAB_SWITCHER_MIN_HEIGHT: i32 = 50;

#[path = "now_playing_effects.rs"]
mod now_playing_effects;

pub(super) struct PanelWidgets {
    pub(super) column: NowPlayingColumn,
    stage: gtk4::Box,
    #[cfg(test)]
    track_content: gtk4::Box,
    lyrics: Rc<LyricsView>,
    up_next: Rc<UpNextPanel>,
    pub(super) visualizer: SongVisualizer,
    pub(super) bloom: cover_bloom::CoverBloom,
    pub(super) shimmer: cover_shimmer::CoverShimmer,
    lyrics_page: adw::ViewStackPage,
    pub(super) visual_page: adw::ViewStackPage,
    cover_stack: gtk4::Stack,
    pub(super) cover_lift: CoverLift,
    external_cover: gtk4::Box,
    cover: gtk4::Image,
    outgoing_cover: gtk4::Image,
    title: gtk4::Label,
    artist: gtk4::Label,
    album: gtk4::Label,
    // Retained for the tab session and NPP-13 acceptance test, which prove
    // the active tab stays outside the cover transition.
    pub(super) tab_stack: adw::ViewStack,
    #[cfg(test)]
    tab_switcher: adw::InlineViewSwitcher,
    footer: gtk4::Label,
    footers: Rc<RefCell<TabFooters>>,
    pub(super) session: Rc<TabSession>,
}

fn build_widgets(
    content: &impl IsA<gtk4::Widget>,
    visible: bool,
    conn: &Rc<Db>,
    cover_loader: &Rc<CoverLoader>,
) -> PanelWidgets {
    TAB_SESSION
        .with(|session| build_widgets_for_session(content, visible, session, conn, cover_loader))
}

fn build_widgets_for_session(
    content: &impl IsA<gtk4::Widget>,
    visible: bool,
    session: &Rc<TabSession>,
    conn: &Rc<Db>,
    cover_loader: &Rc<CoverLoader>,
) -> PanelWidgets {
    let cover = gtk4::Image::builder()
        .pixel_size(tokens::NOW_PLAYING_COVER_SIZE)
        .width_request(tokens::NOW_PLAYING_COVER_SIZE)
        .height_request(tokens::NOW_PLAYING_COVER_SIZE)
        .build();
    cover.set_accessible_role(gtk4::AccessibleRole::Link);
    cover.add_css_class("reprise-now-playing-cover");
    CoverLoader::set_placeholder(&cover);
    let outgoing_cover = gtk4::Image::builder()
        .pixel_size(tokens::NOW_PLAYING_COVER_SIZE)
        .width_request(tokens::NOW_PLAYING_COVER_SIZE)
        .height_request(tokens::NOW_PLAYING_COVER_SIZE)
        .can_target(false)
        .opacity(0.0)
        .visible(false)
        .build();
    outgoing_cover.add_css_class("reprise-now-playing-cover");
    outgoing_cover.set_accessible_role(gtk4::AccessibleRole::Presentation);
    let cover_transition = gtk4::Overlay::new();
    cover_transition.set_child(Some(&cover));
    cover_transition.add_overlay(&outgoing_cover);
    let cover_lift = CoverLift::new(&cover_transition, tokens::NOW_PLAYING_COVER_SIZE);
    let external_cover = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    external_cover.set_size_request(
        tokens::NOW_PLAYING_COVER_SIZE,
        tokens::NOW_PLAYING_COVER_SIZE,
    );
    external_cover.set_halign(gtk4::Align::Center);
    external_cover.set_valign(gtk4::Align::Center);
    let cover_stack = gtk4::Stack::new();
    cover_stack.add_named(cover_lift.widget(), Some("track"));
    cover_stack.add_named(&external_cover, Some("external"));
    cover_stack.set_visible_child_name("track");

    let title = gtk4::Label::builder()
        .xalign(0.5)
        .justify(gtk4::Justification::Center)
        .wrap(true)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    title.set_accessible_role(gtk4::AccessibleRole::Link);
    title.add_css_class("reprise-now-playing-title");
    let artist = gtk4::Label::builder()
        .xalign(0.5)
        .justify(gtk4::Justification::Center)
        .wrap(true)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    artist.set_accessible_role(gtk4::AccessibleRole::Link);
    artist.add_css_class("reprise-now-playing-subtitle");
    let album = gtk4::Label::builder()
        .xalign(0.5)
        .justify(gtk4::Justification::Center)
        .wrap(true)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    album.set_accessible_role(gtk4::AccessibleRole::Link);
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
    head.append(&cover_stack);
    head.append(&metadata);

    let glow = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    glow.add_css_class("reprise-now-playing-glow");
    glow.set_can_target(false);
    let head_overlay = gtk4::Overlay::new();
    head_overlay.set_child(Some(&glow));
    let bloom = cover_bloom::CoverBloom::new();
    let shimmer = cover_shimmer::CoverShimmer::new();
    // Bottom to top: the static ellipse, the blurred cover, the cover-palette
    // sweep, then the cover and the title block over all three.
    head_overlay.add_overlay(bloom.widget());
    head_overlay.add_overlay(shimmer.widget());
    head_overlay.add_overlay(&head);

    let lyrics = LyricsView::new();
    let up_next = UpNextPanel::new(conn.clone(), cover_loader);
    let visualizer = SongVisualizer::new();
    let visual_viewport = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .child(visualizer.widget())
        .build();
    let tab_stack = adw::ViewStack::builder().vexpand(true).build();
    tab_stack.add_titled_with_icon(
        up_next.widget(),
        Some(UP_NEXT_PAGE),
        &strings::text(strings::UP_NEXT),
        PanelTab::UpNext.icon_name(),
    );
    let lyrics_page = tab_stack.add_titled_with_icon(
        lyrics.widget(),
        Some(LYRICS_PAGE),
        &lyrics_strings::text(lyrics_strings::LYRICS),
        PanelTab::Lyrics.icon_name(),
    );
    let visual_page = tab_stack.add_titled_with_icon(
        &visual_viewport,
        Some(VISUAL_PAGE),
        &strings::text(strings::VISUAL),
        PanelTab::Visual.icon_name(),
    );
    tab_stack.set_visible_child_name(session.selected.get().page_name());
    lyrics.set_tab_open(session.selected.get() == PanelTab::Lyrics);
    let tab_switcher = adw::InlineViewSwitcher::builder()
        .stack(&tab_stack)
        .display_mode(adw::InlineViewSwitcherDisplayMode::Icons)
        .can_shrink(true)
        .homogeneous(true)
        .build();
    tab_switcher.add_css_class("reprise-now-playing-tabs");
    tab_switcher.set_size_request(1, TAB_SWITCHER_MIN_HEIGHT);

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
    stage.append(&tab_switcher);
    stage.append(&tab_stack);
    stage.append(&footer);

    let toolbar = adw::ToolbarView::new();
    toolbar.set_content(Some(&stage));
    let column = NowPlayingColumn::new(content, &toolbar, visible);
    PanelWidgets {
        column,
        stage,
        #[cfg(test)]
        track_content,
        lyrics,
        up_next,
        visualizer,
        bloom,
        shimmer,
        lyrics_page,
        visual_page,
        cover_stack,
        cover_lift,
        external_cover,
        cover,
        outgoing_cover,
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
    pub(super) widgets: PanelWidgets,
    toggle: gtk4::ToggleButton,
    pub(super) conn: Rc<Db>,
    cover_loader: Rc<CoverLoader>,
    cover_generation: Rc<Cell<u64>>,
    pub(super) loaded_track: RefCell<Option<NowPlaying>>,
    pub(super) external_snapshot: RefCell<Option<ExternalPlaybackSnapshot>>,
    pub(super) playback_state: Cell<PlaybackState>,
    syncing_visibility: Cell<bool>,
    on_up_next_refresh: RefCell<Option<OnVoid>>,
    cover_animation: RefCell<Option<adw::TimedAnimation>>,
    cover_animation_generation: Cell<u64>,
    cover_transition_active: Cell<bool>,
    on_track_reveal: crate::ui::link_activation::ActivationSlot,
    pub(super) song_visuals_enabled: Cell<bool>,
    pub(super) swell: RefCell<Swell>,
    pub(super) swell_pressure: Cell<f64>,
    pub(super) swell_last_frame_us: Cell<i64>,
    on_album_reveal: crate::ui::link_activation::ActivationSlot,
    on_artist_reveal: crate::ui::link_activation::ActivationSlot,
}

impl NowPlayingPanel {
    pub(in crate::ui) fn new(
        content: &impl IsA<gtk4::Widget>,
        conn: Rc<Db>,
        _runtime: Rc<ArtistNewsRuntime>,
        cover_loader: Rc<CoverLoader>,
    ) -> Rc<Self> {
        let visible = reprise_core::library::settings::get_info_panel_visible(&conn);
        let song_visuals_enabled =
            reprise_core::modules::is_enabled(&conn, &reprise_core::modules::SONG_VISUALS_MODULE)
                .unwrap_or(reprise_core::modules::SONG_VISUALS_MODULE.default_enabled);
        let panel = Rc::new(Self {
            widgets: build_widgets(content, visible, &conn, &cover_loader),
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
            external_snapshot: RefCell::new(None),
            playback_state: Cell::new(PlaybackState::Stopped),
            syncing_visibility: Cell::new(false),
            on_up_next_refresh: RefCell::new(None),
            cover_animation: RefCell::new(None),
            cover_animation_generation: Cell::new(0),
            cover_transition_active: Cell::new(false),
            on_track_reveal: Rc::new(RefCell::new(None)),
            song_visuals_enabled: Cell::new(song_visuals_enabled),
            swell: RefCell::new(Swell::default()),
            swell_pressure: Cell::new(0.0),
            swell_last_frame_us: Cell::new(0),
            on_album_reveal: Rc::new(RefCell::new(None)),
            on_artist_reveal: Rc::new(RefCell::new(None)),
        });
        panel.widgets.bloom.set_on_frame({
            let weak = Rc::downgrade(&panel);
            move |frame_time_us| {
                if let Some(panel) = weak.upgrade() {
                    panel.advance_swell(frame_time_us);
                }
            }
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
        panel.sync_bloom_activity();
        panel.render_track();
        panel
    }

    pub(in crate::ui) fn widget(&self) -> &adw::OverlaySplitView {
        self.widgets.column.widget()
    }

    pub(in crate::ui) fn toggle_button(&self) -> gtk4::ToggleButton {
        self.toggle.clone()
    }

    pub(in crate::ui) fn set_link_labels(&self, labels: LinkLabels) {
        let cover = strings::text(labels.cover);
        crate::ui::link_activation::relabel(&self.widgets.cover, &cover);
        crate::ui::link_activation::relabel(&self.widgets.album, &cover);
        crate::ui::link_activation::relabel(&self.widgets.title, &strings::text(labels.title));
        crate::ui::link_activation::relabel(&self.widgets.artist, &strings::text(labels.subtitle));
    }

    pub(in crate::ui) fn lyrics_view(&self) -> Rc<LyricsView> {
        self.widgets.lyrics.clone()
    }

    pub(in crate::ui) fn show_lyrics(&self) {
        if self.widgets.lyrics_page.is_visible() {
            self.widgets.tab_stack.set_visible_child_name(LYRICS_PAGE);
        } else {
            self.widgets.tab_stack.set_visible_child_name(UP_NEXT_PAGE);
        }
        self.widgets.column.set_visible(true);
    }

    pub(in crate::ui) fn apply_persisted_visibility(&self, visible: bool) {
        self.set_transient_visibility(visible);
    }

    pub(in crate::ui) fn is_panel_visible(&self) -> bool {
        self.widgets.column.is_visible()
    }

    pub(in crate::ui) fn set_transient_visibility(&self, visible: bool) {
        self.syncing_visibility.set(true);
        self.widgets.column.set_visible(visible);
        self.toggle.set_active(visible);
        self.syncing_visibility.set(false);
        self.request_up_next_refresh_if_visible();
        self.sync_visual_activity();
        self.sync_bloom_activity();
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
        self.sync_media_presence();
        if id_changed {
            // A new track started: reset the visual engine's clock, water
            // surface, and impact overlay so ripples/sparks from the
            // previous track don't bleed into the new one.
            self.widgets.visualizer.note_track_changed();
        }
        if !changed {
            if !self.cover_transition_active.get() {
                self.render_track();
            }
            return;
        }
        if !crate::ui::motion::animations_enabled() {
            self.cancel_cover_animation();
            self.render_track();
            return;
        }
        self.animate_cover_change();
    }

    pub(in crate::ui) fn set_external_snapshot(&self, snapshot: Option<ExternalPlaybackSnapshot>) {
        let external_active = snapshot.is_some();
        *self.external_snapshot.borrow_mut() = snapshot;
        self.widgets.lyrics_page.set_visible(!external_active);
        if let Some(page) = page_after_tab_hidden(
            self.widgets.session.selected.get(),
            PanelTab::Lyrics,
            !external_active,
        ) {
            self.widgets.tab_stack.set_visible_child_name(page);
        }
        self.sync_visual_page_visibility();
        self.sync_media_presence();
        self.sync_visual_activity();
        self.sync_bloom_activity();
        self.render_track();
    }

    pub(in crate::ui) fn set_playback_state(&self, state: PlaybackState) {
        self.playback_state.set(state);
        if state != PlaybackState::Playing {
            self.swell_pressure.set(0.0);
        }
        self.widgets.visualizer.set_playback_state(state);
        self.sync_bloom_activity();
    }

    pub(in crate::ui) fn set_spectrum(&self, frame: SpectrumFrame) {
        if self.song_visuals_active_for_media() {
            let bass = frame.bass_pressure();
            let playing = self.playback_state.get() == PlaybackState::Playing;
            let pressure = if playing {
                f64::from(bass.pressure)
            } else {
                0.0
            };
            self.swell_pressure.set(pressure);
            self.advance_swell(gtk4::glib::monotonic_time());
            self.widgets.visualizer.set_spectrum(frame);
        }
    }

    pub(in crate::ui) fn set_song_visuals_enabled(&self, enabled: bool) {
        self.song_visuals_enabled.set(enabled);
        if !enabled {
            self.widgets.visualizer.set_active(false);
            self.advance_swell(0);
        }
        self.sync_visual_page_visibility();
        self.sync_bloom_activity();
        self.sync_visual_activity();
    }

    pub(in crate::ui) fn set_up_next_model(
        &self,
        model: &crate::ui::track_list::queue_sections::QueueViewModel,
        context_window: &Rc<dyn crate::ui::track_list::queue_sections::ContextWindow>,
    ) {
        let text = self.widgets.up_next.set_queue_model(model, context_window);
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

    pub(in crate::ui) fn set_on_up_next_enqueue(
        &self,
        callback: impl Fn(&[reprise_core::up_next::QueueItem]) -> bool + 'static,
    ) {
        self.widgets.up_next.set_on_enqueue(callback);
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
                    panel.sync_bloom_activity();
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
                    let conn = &panel.conn;
                    reprise_core::library::settings::set_info_panel_visible(conn, visible)
                };
                if let Err(error) = saved {
                    tracing::warn!(%error, "could not save now-playing panel visibility");
                }
                panel.request_up_next_refresh_if_visible();
                panel.sync_visual_activity();
                panel.sync_bloom_activity();
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

    #[cfg(test)]
    fn bloom_widget(&self) -> &gtk4::Widget {
        self.widgets.bloom.widget().upcast_ref()
    }

    #[cfg(test)]
    fn stage_for_test(&self) -> &gtk4::Box {
        &self.widgets.stage
    }
}

pub(in crate::ui) fn css() -> String {
    super::surface_css::css()
}

#[cfg(test)]
#[path = "now_playing_external_tests.rs"]
mod external_tests;
#[cfg(test)]
#[path = "now_playing_reactive_tests.rs"]
mod reactive_tests;
#[cfg(test)]
#[path = "now_playing_tab_tests.rs"]
mod tab_tests;
#[cfg(test)]
#[path = "now_playing_tests.rs"]
mod tests;
