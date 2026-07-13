use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::artist_news::{AlbumNews, ArtistNews, NewsError, NewsKind};
use reprise_core::cover::ThumbnailSize;
use rusqlite::Connection;

use super::artist_news_worker::{ArtistNewsRequest, ArtistNewsResponse, ArtistNewsRuntime};
use super::cover_loader::CoverLoader;
use super::info_panel_state::{PanelContext, PanelState, RequestIntent};
use super::strings;

const PANEL_WIDTH: f64 = 340.0;
const PINNED_MIN_WIDTH: f64 = 1200.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct PanelMetrics {
    width: f64,
    pinned: bool,
    collapsed: bool,
}

fn panel_metrics(narrow: bool) -> PanelMetrics {
    PanelMetrics {
        width: PANEL_WIDTH,
        pinned: !narrow,
        collapsed: narrow,
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderKind {
    Disabled,
    Loading,
    Error,
    NoNews,
    News(usize),
    CachedNews(usize),
}

#[cfg(test)]
fn render_kind(
    enabled: bool,
    loading: bool,
    result: Option<Result<&ArtistNews, &str>>,
) -> RenderKind {
    if !enabled {
        return RenderKind::Disabled;
    }
    if loading {
        return RenderKind::Loading;
    }
    match result {
        Some(Ok(news)) if news.stale => RenderKind::CachedNews(news.items.len()),
        Some(Ok(news)) if news.items.is_empty() => RenderKind::NoNews,
        Some(Ok(news)) => RenderKind::News(news.items.len()),
        Some(Err(_)) | None => RenderKind::Error,
    }
}

fn release_accessible_name(item: &AlbumNews) -> String {
    let kind = match item.kind {
        NewsKind::Upcoming => strings::text(strings::NEWS_UPCOMING),
        NewsKind::New => strings::text(strings::NEWS_NEW),
    };
    format!(
        "{kind}: {}, {}, {}",
        item.title, item.primary_type, item.first_release_date
    )
}

fn release_group_uri(mbid: &str) -> Option<String> {
    let valid = mbid.len() == 36
        && mbid
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-');
    valid.then(|| format!("https://musicbrainz.org/release-group/{mbid}"))
}

struct PanelWidgets {
    split: adw::OverlaySplitView,
    body: gtk4::Box,
    local: gtk4::Box,
    cover: gtk4::Image,
    title: gtk4::Label,
    artist: gtk4::Label,
    album: gtk4::Label,
    refresh: gtk4::Button,
    close: gtk4::Button,
    enable: adw::SwitchRow,
}

fn build_widgets(content: &impl IsA<gtk4::Widget>, visible: bool) -> PanelWidgets {
    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    body.set_margin_top(12);
    body.set_margin_bottom(18);
    body.set_margin_start(12);
    body.set_margin_end(12);

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
    let artist = metadata_label();
    let album = metadata_label();
    let metadata = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    metadata.set_hexpand(true);
    metadata.append(&title);
    metadata.append(&artist);
    metadata.append(&album);
    let local = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    local.add_css_class("card");
    local.set_margin_bottom(6);
    local.append(&cover);
    local.append(&metadata);
    body.append(&local);

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&body)
        .build();
    let refresh = gtk4::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text(strings::text(strings::NEWS_REFRESH))
        .build();
    let close = gtk4::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text(strings::text(strings::CLOSE))
        .build();
    let heading = adw::WindowTitle::new(&strings::text(strings::INFORMATION), "");
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&heading));
    header.pack_start(&refresh);
    header.pack_end(&close);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&scrolled));
    // Start in the safe overlay shape before the first allocation. The
    // breakpoint below promotes this to the pinned desktop shape when wide;
    // doing the inverse briefly over-constrains narrow startup windows.
    let metrics = panel_metrics(true);
    let split = adw::OverlaySplitView::builder()
        .content(content)
        .sidebar(&toolbar)
        .sidebar_position(gtk4::PackType::End)
        .min_sidebar_width(metrics.width)
        .max_sidebar_width(metrics.width)
        .pin_sidebar(metrics.pinned)
        .collapsed(metrics.collapsed)
        .show_sidebar(visible)
        .build();
    let enable = adw::SwitchRow::builder()
        .title(strings::text(strings::ARTIST_NEWS))
        .subtitle(strings::text(strings::ARTIST_NEWS_PRIVACY))
        .use_markup(false)
        .build();
    PanelWidgets {
        split,
        body,
        local,
        cover,
        title,
        artist,
        album,
        refresh,
        close,
        enable,
    }
}

fn metadata_label() -> gtk4::Label {
    let label = gtk4::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    label.add_css_class("dim-label");
    label
}

pub(super) struct InfoPanel {
    widgets: PanelWidgets,
    toggle: gtk4::ToggleButton,
    conn: Rc<RefCell<Connection>>,
    runtime: Rc<ArtistNewsRuntime>,
    cover_loader: Rc<CoverLoader>,
    cover_generation: Rc<Cell<u64>>,
    state: RefCell<PanelState>,
    syncing_visibility: Cell<bool>,
    syncing_enabled: Cell<bool>,
    window: glib::WeakRef<adw::ApplicationWindow>,
}

impl InfoPanel {
    pub(super) fn new(
        content: &impl IsA<gtk4::Widget>,
        window: &adw::ApplicationWindow,
        conn: Rc<RefCell<Connection>>,
        runtime: Rc<ArtistNewsRuntime>,
        cover_loader: Rc<CoverLoader>,
    ) -> Rc<Self> {
        let visible = reprise_core::library::settings::get_info_panel_visible(&conn.borrow());
        let widgets = build_widgets(content, visible);
        install_breakpoint(window, &widgets.split);
        let toggle = gtk4::ToggleButton::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text(strings::text(strings::INFORMATION))
            .active(visible)
            .build();
        let panel = Rc::new(Self {
            widgets,
            toggle,
            conn,
            state: RefCell::new(PanelState::new(runtime.enabled.get())),
            runtime,
            cover_loader,
            cover_generation: Rc::new(Cell::new(0)),
            syncing_visibility: Cell::new(false),
            syncing_enabled: Cell::new(false),
            window: glib::WeakRef::new(),
        });
        panel.window.set(Some(window));
        panel.wire();
        panel.render_context();
        panel
    }

    pub(super) fn widget(&self) -> &adw::OverlaySplitView {
        &self.widgets.split
    }

    pub(super) fn toggle_button(&self) -> gtk4::ToggleButton {
        self.toggle.clone()
    }

    pub(super) fn set_context(self: &Rc<Self>, context: PanelContext) {
        self.apply_local_context(&context);
        let intent = self.state.borrow_mut().set_context(context);
        self.start_or_render(intent);
    }

    fn wire(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.toggle.connect_toggled(move |button| {
            let Some(panel) = weak.upgrade() else { return };
            if !panel.syncing_visibility.get() {
                panel.widgets.split.set_show_sidebar(button.is_active());
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets
            .split
            .connect_show_sidebar_notify(move |split| {
                let Some(panel) = weak.upgrade() else { return };
                panel.syncing_visibility.set(true);
                panel.toggle.set_active(split.shows_sidebar());
                panel.syncing_visibility.set(false);
                let saved = {
                    let conn = panel.conn.borrow();
                    reprise_core::library::settings::set_info_panel_visible(
                        &conn,
                        split.shows_sidebar(),
                    )
                };
                if let Err(error) = saved {
                    tracing::warn!(%error, "could not save information panel visibility");
                }
            });
        let split = self.widgets.split.clone();
        self.widgets
            .close
            .connect_clicked(move |_| split.set_show_sidebar(false));
        let weak = Rc::downgrade(self);
        self.widgets.refresh.connect_clicked(move |_| {
            if let Some(panel) = weak.upgrade() {
                let intent = panel.state.borrow_mut().refresh();
                panel.start_or_render(intent);
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.enable.connect_active_notify(move |row| {
            let Some(panel) = weak.upgrade() else { return };
            if panel.syncing_enabled.get() {
                return;
            }
            let requested = row.is_active();
            let saved = {
                let conn = panel.conn.borrow();
                panel.runtime.set_enabled(&conn, requested)
            };
            if let Err(error) = saved {
                tracing::warn!(%error, "could not save Artist News plugin state");
                panel.syncing_enabled.set(true);
                row.set_active(!requested);
                panel.syncing_enabled.set(false);
            }
        });
        let alive = Rc::downgrade(self);
        let callback = Rc::downgrade(self);
        self.runtime.subscribe_enabled(
            move || alive.upgrade().is_some(),
            move |enabled| {
                let Some(panel) = callback.upgrade() else {
                    return;
                };
                panel.syncing_enabled.set(true);
                panel.widgets.enable.set_active(enabled);
                panel.syncing_enabled.set(false);
                let intent = panel.state.borrow_mut().set_enabled(enabled);
                panel.start_or_render(intent);
            },
        );
    }

    fn start_or_render(self: &Rc<Self>, intent: Option<RequestIntent>) {
        match intent {
            Some(intent) => {
                self.render_loading();
                self.dispatch(intent);
            }
            None => self.render_context(),
        }
    }

    fn dispatch(self: &Rc<Self>, intent: RequestIntent) {
        let local_albums = {
            let conn = self.conn.borrow();
            reprise_core::queries::query_artist_albums(&conn, &intent.artist)
        };
        let local_albums = match local_albums {
            Ok(albums) => albums,
            Err(error) => {
                tracing::warn!(%error, "could not query local albums for Artist News");
                self.render_error(strings::text(strings::NEWS_ERROR));
                return;
            }
        };
        let (sender, receiver) = async_channel::bounded(1);
        self.runtime.request(ArtistNewsRequest {
            generation: intent.generation,
            artist: intent.artist,
            local_albums,
            force: intent.force,
            response: sender,
        });
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(response) = receiver.recv().await else {
                return;
            };
            if let Some(panel) = weak.upgrade() {
                panel.apply_response(response);
            }
        });
    }

    fn apply_response(&self, response: ArtistNewsResponse) {
        if !self.state.borrow().accepts(response.generation) {
            return;
        }
        match response.result {
            Ok(news) => self.render_news(&news),
            Err(error) => self.render_error(news_error_text(&error)),
        }
    }

    fn apply_local_context(&self, context: &PanelContext) {
        let generation = self.cover_generation.get().wrapping_add(1);
        self.cover_generation.set(generation);
        CoverLoader::set_placeholder(&self.widgets.cover);
        match context {
            PanelContext::Empty => {
                self.widgets.local.set_visible(false);
            }
            PanelContext::Multiple(count) => {
                self.widgets.local.set_visible(true);
                self.widgets.cover.set_visible(false);
                self.widgets
                    .title
                    .set_text(&strings::tracks_selected(*count));
                self.widgets.artist.set_text("");
                self.widgets.album.set_text("");
            }
            PanelContext::Track(track) => {
                self.widgets.local.set_visible(true);
                self.widgets.cover.set_visible(true);
                self.widgets.title.set_text(&track.title);
                self.widgets.artist.set_text(&track.artist);
                self.widgets.album.set_text(&track.album);
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

    fn render_context(&self) {
        if !self.runtime.enabled.get() {
            self.render_disabled();
            return;
        }
        match self.state.borrow().context() {
            PanelContext::Empty => self.render_status(strings::text(strings::NEWS_SELECT_TRACK)),
            PanelContext::Multiple(_) => {
                self.render_status(strings::text(strings::NEWS_MULTIPLE_SELECTION));
            }
            PanelContext::Track(track) if track.artist.trim().is_empty() => {
                self.render_status(strings::text(strings::NEWS_NO_ARTIST));
            }
            PanelContext::Track(_) => self.render_loading(),
        }
    }

    fn render_disabled(&self) {
        self.clear_body_after_local();
        let card = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        card.add_css_class("card");
        let title = status_label(strings::text(strings::NEWS_DISABLED_TITLE));
        title.add_css_class("title-4");
        card.append(&title);
        card.append(&status_label(strings::text(strings::ARTIST_NEWS_PRIVACY)));
        card.append(&self.widgets.enable);
        self.widgets.body.append(&card);
    }

    fn render_loading(&self) {
        self.clear_body_after_local();
        let spinner = gtk4::Spinner::new();
        spinner.start();
        self.widgets.body.append(&spinner);
        self.widgets
            .body
            .append(&status_label(strings::text(strings::NEWS_LOADING)));
    }

    fn render_status(&self, text: String) {
        self.clear_body_after_local();
        self.widgets.body.append(&status_label(text));
    }

    fn render_error(&self, text: String) {
        self.clear_body_after_local();
        let label = status_label(text);
        label.add_css_class("error");
        self.widgets.body.append(&label);
    }

    fn render_news(&self, news: &ArtistNews) {
        self.clear_body_after_local();
        if news.items.is_empty() {
            self.widgets
                .body
                .append(&status_label(strings::text(strings::NEWS_NONE)));
        }
        for item in &news.items {
            self.widgets.body.append(&self.release_card(item));
        }
        let source = if news.stale {
            strings::news_cached(news.fetched_at)
        } else {
            strings::news_updated(news.fetched_at)
        };
        let source = status_label(source);
        source.add_css_class("dim-label");
        self.widgets.body.append(&source);
    }

    fn release_card(&self, item: &AlbumNews) -> gtk4::Box {
        let card = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        card.add_css_class("card");
        card.update_property(
            &[gtk4::accessible::Property::Label(&release_accessible_name(
                item,
            ))],
        );
        let kind = match item.kind {
            NewsKind::Upcoming => strings::text(strings::NEWS_UPCOMING),
            NewsKind::New => strings::text(strings::NEWS_NEW),
        };
        let kind = status_label(kind);
        kind.add_css_class("accent");
        card.append(&kind);
        let title = status_label(item.title.clone());
        title.add_css_class("title-4");
        card.append(&title);
        card.append(&status_label(strings::news_release_meta(
            &item.primary_type,
            &item.first_release_date,
        )));
        if let Some(uri) = release_group_uri(&item.release_group_mbid) {
            let button = gtk4::Button::with_label(&strings::text(strings::NEWS_OPEN_MUSICBRAINZ));
            let window = self.window.clone();
            button.connect_clicked(move |_| {
                let launcher = gtk4::UriLauncher::new(&uri);
                launcher.launch(
                    window.upgrade().as_ref(),
                    None::<&gtk4::gio::Cancellable>,
                    |result| {
                        if let Err(error) = result {
                            tracing::warn!(%error, "could not open MusicBrainz release group");
                        }
                    },
                );
            });
            card.append(&button);
        }
        card
    }

    fn clear_body_after_local(&self) {
        let mut child = self.widgets.local.next_sibling();
        while let Some(current) = child {
            child = current.next_sibling();
            self.widgets.body.remove(&current);
        }
    }
}

fn install_breakpoint(window: &adw::ApplicationWindow, split: &adw::OverlaySplitView) {
    let condition = adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MinWidth,
        PINNED_MIN_WIDTH,
        adw::LengthUnit::Px,
    );
    let breakpoint = adw::Breakpoint::new(condition);
    breakpoint.add_setter(split, "collapsed", Some(&false.to_value()));
    breakpoint.add_setter(split, "pin-sidebar", Some(&true.to_value()));
    window.add_breakpoint(breakpoint);
}

fn status_label(text: String) -> gtk4::Label {
    gtk4::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .build()
}

fn news_error_text(error: &NewsError) -> String {
    match error {
        NewsError::Unmatched => strings::text(strings::NEWS_UNMATCHED),
        NewsError::Ambiguous => strings::text(strings::NEWS_AMBIGUOUS),
        NewsError::InvalidResponse | NewsError::Fetch(_) => strings::text(strings::NEWS_ERROR),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::artist_news::{AlbumNews, ArtistNews, NewsKind};

    fn release(title: &str, kind: NewsKind) -> AlbumNews {
        AlbumNews {
            release_group_mbid: "11111111-1111-1111-1111-111111111111".into(),
            title: title.into(),
            first_release_date: "2026-10-01".into(),
            primary_type: "Album".into(),
            kind,
        }
    }

    #[test]
    fn panel_metrics_are_pinned_wide_and_overlay_narrow() {
        assert_eq!(
            panel_metrics(false),
            PanelMetrics {
                width: 340.0,
                pinned: true,
                collapsed: false
            }
        );
        assert_eq!(
            panel_metrics(true),
            PanelMetrics {
                width: 340.0,
                pinned: false,
                collapsed: true
            }
        );
    }

    #[test]
    fn information_panel_visibility_round_trips_through_settings() {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        assert!(reprise_core::library::settings::get_info_panel_visible(
            &conn
        ));
        reprise_core::library::settings::set_info_panel_visible(&conn, false).unwrap();
        assert!(!reprise_core::library::settings::get_info_panel_visible(
            &conn
        ));
    }

    #[test]
    fn disabled_plugin_always_renders_privacy_card() {
        assert_eq!(render_kind(false, false, None), RenderKind::Disabled);
    }

    #[test]
    fn pending_failure_and_cached_results_have_distinct_render_states() {
        assert_eq!(render_kind(true, true, None), RenderKind::Loading);
        assert_eq!(
            render_kind(true, false, Some(Err("offline"))),
            RenderKind::Error
        );
        let cached = ArtistNews {
            artist: "Artist".into(),
            artist_mbid: "id".into(),
            fetched_at: 1,
            items: vec![release("Album", NewsKind::New)],
            stale: true,
        };
        assert_eq!(
            render_kind(true, false, Some(Ok(&cached))),
            RenderKind::CachedNews(1)
        );
    }

    #[test]
    fn fresh_empty_and_populated_results_render_separately() {
        let empty = ArtistNews {
            artist: "Artist".into(),
            artist_mbid: "id".into(),
            fetched_at: 1,
            items: vec![],
            stale: false,
        };
        let news = ArtistNews {
            items: vec![
                release("Soon", NewsKind::Upcoming),
                release("New", NewsKind::New),
            ],
            ..empty.clone()
        };
        assert_eq!(
            render_kind(true, false, Some(Ok(&empty))),
            RenderKind::NoNews
        );
        assert_eq!(
            render_kind(true, false, Some(Ok(&news))),
            RenderKind::News(2)
        );
    }

    #[test]
    fn release_accessible_name_contains_status_title_type_and_date() {
        assert_eq!(
            release_accessible_name(&release("Future Album", NewsKind::Upcoming)),
            "Upcoming: Future Album, Album, 2026-10-01"
        );
    }

    #[test]
    fn release_group_uri_accepts_only_a_musicbrainz_mbid() {
        assert_eq!(
            release_group_uri("11111111-1111-1111-1111-111111111111").as_deref(),
            Some("https://musicbrainz.org/release-group/11111111-1111-1111-1111-111111111111")
        );
        assert_eq!(release_group_uri("../outside"), None);
    }

    #[test]
    fn provider_errors_have_specific_match_copy_and_generic_network_copy() {
        assert_eq!(
            news_error_text(&NewsError::Unmatched),
            strings::text(strings::NEWS_UNMATCHED)
        );
        assert_eq!(
            news_error_text(&NewsError::Ambiguous),
            strings::text(strings::NEWS_AMBIGUOUS)
        );
        assert_eq!(
            news_error_text(&NewsError::Fetch(
                reprise_core::musicbrainz::FetchError::Timeout
            )),
            strings::text(strings::NEWS_ERROR)
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn widget_exposes_information_sidebar_metrics() {
        gtk4::init().unwrap();
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let widgets = build_widgets(&content, true);
        assert_eq!(widgets.split.sidebar_position(), gtk4::PackType::End);
        assert_eq!(widgets.split.min_sidebar_width(), 340.0);
        assert_eq!(widgets.split.max_sidebar_width(), 340.0);
        assert!(!widgets.split.is_pin_sidebar());
        assert!(widgets.split.is_collapsed());
    }
}
