//! Main library/sidebar composition, including the contextual end panel.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use rusqlite::Connection;

use super::artist_news_worker::ArtistNewsRuntime;
use super::info_panel::InfoPanel;
use super::library_chrome::LibraryTitle;
use super::now_playing_wiring;
use super::player_controller::PlayerController;
use super::sidebar::Sidebar;
use super::stats_view::StatsView;
use super::strings;
use super::track_list::TrackList;
use reprise_core::view_source::ViewSource;

pub(super) const SIDEBAR_BREAKPOINT_WIDTH: i32 = 800;
pub(super) const LIBRARY_VIEW_TRACKS: &str = "tracks";
pub(super) const LIBRARY_VIEW_ALBUMS: &str = "albums";
pub(super) const LIBRARY_VIEW_ARTISTS: &str = "artists";

pub(super) struct LibraryShell {
    pub sidebar_page: adw::NavigationPage,
    pub split_view: adw::NavigationSplitView,
    pub content_nav: adw::NavigationView,
    pub info_panel: Rc<InfoPanel>,
}

pub(super) struct LibraryViews {
    pub(super) stack: adw::ViewStack,
}

pub(super) fn build_views(
    tracks: &impl IsA<gtk4::Widget>,
    albums: &impl IsA<gtk4::Widget>,
    artists: &impl IsA<gtk4::Widget>,
) -> LibraryViews {
    let stack = adw::ViewStack::new();
    stack.add_titled(
        tracks,
        Some(LIBRARY_VIEW_TRACKS),
        &strings::text(strings::LIBRARY_VIEW_TRACKS),
    );
    stack.add_titled(
        albums,
        Some(LIBRARY_VIEW_ALBUMS),
        &strings::text(strings::LIBRARY_VIEW_ALBUMS),
    );
    stack.add_titled(
        artists,
        Some(LIBRARY_VIEW_ARTISTS),
        &strings::text(strings::LIBRARY_VIEW_ARTISTS),
    );
    stack.set_visible_child_name(LIBRARY_VIEW_TRACKS);
    LibraryViews { stack }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn wire_source_routing(
    sidebar: &Rc<Sidebar>,
    track_list: &Rc<TrackList>,
    stats_view: StatsView,
    conn: &Rc<RefCell<Connection>>,
    content_stack: &gtk4::Stack,
    views: &LibraryViews,
    title: &Rc<LibraryTitle>,
    source_title: &adw::WindowTitle,
    show_content: Rc<dyn Fn()>,
) {
    let track_list = track_list.clone();
    let content_stack = content_stack.clone();
    let library_stack = views.stack.clone();
    let title = title.clone();
    let source_title = source_title.clone();
    let stats_view = Rc::new(stats_view);
    let conn = conn.clone();
    let show_content_on_select = show_content.clone();
    sidebar.set_on_select(move |source, source_name| {
        let is_library = matches!(source, ViewSource::Library);
        if matches!(source, ViewSource::MyStats) {
            stats_view.refresh(&conn);
            content_stack.set_visible_child_name("stats");
        } else {
            content_stack.set_visible_child_name("library");
            library_stack.set_visible_child_name(LIBRARY_VIEW_TRACKS);
            track_list.set_source(source);
        }
        title.set_library_navigation_visible(is_library);
        source_title.set_title(&source_name);
        show_content_on_select();
    });
    sidebar.set_on_show_content(move || show_content());
}

pub(super) fn build(
    window: &adw::ApplicationWindow,
    conn: &Rc<RefCell<Connection>>,
    sidebar: &Sidebar,
    toast_overlay: &adw::ToastOverlay,
    track_list: &Rc<TrackList>,
    player: Option<&Rc<PlayerController>>,
    runtime: &Rc<ArtistNewsRuntime>,
) -> LibraryShell {
    let sidebar_page = adw::NavigationPage::builder()
        .title(strings::text(strings::APP_NAME))
        .child(sidebar.widget())
        .build();
    let info_panel = InfoPanel::new(
        toast_overlay,
        window,
        conn.clone(),
        runtime.clone(),
        track_list.shared_cover_loader(),
    );
    if let Some(player) = player {
        player.set_lyrics_view(&info_panel.lyrics_view());
    }
    let content_nav = now_playing_wiring::build_content_nav(
        info_panel.widget(),
        player.map(|controller| controller.now_playing_widget()),
        &strings::text(strings::APP_NAME),
    );
    let content_page = adw::NavigationPage::builder()
        .title(strings::text(strings::APP_NAME))
        .child(&content_nav)
        .build();
    let split_view = adw::NavigationSplitView::builder()
        .sidebar(&sidebar_page)
        .content(&content_page)
        .collapsed(true)
        .build();
    super::sidebar_presentation::style_split_view(&split_view);
    let condition = adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MinWidth,
        f64::from(SIDEBAR_BREAKPOINT_WIDTH),
        adw::LengthUnit::Px,
    );
    let breakpoint = adw::Breakpoint::new(condition);
    breakpoint.add_setter(&split_view, "collapsed", Some(&false.to_value()));
    window.add_breakpoint(breakpoint);
    LibraryShell {
        sidebar_page,
        split_view,
        content_nav,
        info_panel,
    }
}

#[cfg(test)]
mod tests {
    use libadwaita::prelude::*;

    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn library_views_expose_tracks_albums_and_artists_in_order() {
        if gtk4::init().is_err() {
            return;
        }
        let tracks = gtk4::Label::new(Some("Track table"));
        let albums = gtk4::Label::new(Some("Album grid"));
        let artists = gtk4::Label::new(Some("Artist grid"));

        let views = build_views(&tracks, &albums, &artists);

        assert_eq!(
            views.stack.visible_child_name().as_deref(),
            Some(LIBRARY_VIEW_TRACKS)
        );
        assert_eq!(views.stack.pages().n_items(), 3);
        assert_eq!(
            views.stack.child_by_name(LIBRARY_VIEW_TRACKS),
            Some(tracks.upcast())
        );
        assert_eq!(
            views.stack.child_by_name(LIBRARY_VIEW_ALBUMS),
            Some(albums.upcast())
        );
        assert_eq!(
            views.stack.child_by_name(LIBRARY_VIEW_ARTISTS),
            Some(artists.upcast())
        );
    }
}
