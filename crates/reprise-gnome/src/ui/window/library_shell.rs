//! Main library/sidebar composition, including the contextual end panel.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use rusqlite::Connection;

use super::album_view::AlbumView;
use super::artist_news_worker::ArtistNewsRuntime;
use super::artist_view::ArtistView;
use super::device_view::DeviceViewPage;
use super::info_panel::InfoPanel;
use super::library_chrome::LibraryTitle;
use super::now_playing_wiring;
use super::player_controller::PlayerController;
use super::sidebar::Sidebar;
use super::stats_view::StatsView;
use super::strings;
use super::track_list::TrackList;
use reprise_core::queries::ArtistAlbum;
use reprise_core::view_source::ViewSource;

pub(in crate::ui) const SIDEBAR_BREAKPOINT_WIDTH: i32 = 800;
pub(in crate::ui) const LIBRARY_VIEW_TRACKS: &str = "tracks";
pub(in crate::ui) const LIBRARY_VIEW_ALBUMS: &str = "albums";
pub(in crate::ui) const LIBRARY_VIEW_ARTISTS: &str = "artists";
const SMOKE_LIBRARY_VIEW_ENV: &str = "REPRISE_SMOKE_LIBRARY_VIEW";

pub(in crate::ui) struct LibraryShell {
    pub sidebar_page: adw::NavigationPage,
    pub split_view: adw::NavigationSplitView,
    pub content_nav: adw::NavigationView,
    pub info_panel: Rc<InfoPanel>,
}

pub(in crate::ui) struct LibraryViews {
    pub(in crate::ui) stack: gtk4::Stack,
}

pub(in crate::ui) fn build_views(
    tracks: &impl IsA<gtk4::Widget>,
    albums: &impl IsA<gtk4::Widget>,
    artists: &impl IsA<gtk4::Widget>,
) -> LibraryViews {
    let stack = gtk4::Stack::new();
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

pub(in crate::ui) fn wire_album_view(
    views: &LibraryViews,
    album_view: &AlbumView,
    track_list: &Rc<TrackList>,
) {
    // Card click → switch to Tracks tab with album source.
    let track_list_clone = track_list.clone();
    let stack = views.stack.downgrade();
    album_view.set_on_activate(move |album| {
        let source = ViewSource::Album {
            album: album.album.clone(),
            album_artist: album.album_artist.clone(),
        };
        track_list_clone.set_source(source);
        if let Some(stack) = stack.upgrade() {
            stack.set_visible_child_name(LIBRARY_VIEW_TRACKS);
        }
    });

    // Artist label click → switch to Artists tab.
    let stack = views.stack.downgrade();
    album_view.set_on_artist_activate(move |_artist| {
        if let Some(stack) = stack.upgrade() {
            stack.set_visible_child_name(LIBRARY_VIEW_ARTISTS);
        }
    });

    // Refresh on tab show.
    let refresh = album_view.refresh_callback();
    views.stack.connect_visible_child_name_notify(move |stack| {
        if stack.visible_child_name().as_deref() == Some(LIBRARY_VIEW_ALBUMS) {
            refresh();
        }
    });
}

pub(in crate::ui) fn wire_artist_view(
    views: &LibraryViews,
    artist_view: &ArtistView,
    track_list: &Rc<TrackList>,
) {
    // "Show all tracks" for the selected artist opens the track table.
    {
        let track_list = Rc::downgrade(track_list);
        let stack = views.stack.clone();
        artist_view.set_on_show_all_tracks(move |artist| {
            let Some(track_list) = track_list.upgrade() else {
                return;
            };
            track_list.set_source(ViewSource::Artist(artist));
            stack.set_visible_child_name(LIBRARY_VIEW_TRACKS);
        });
    }

    // Activating an album card opens that album. `ArtistAlbum` carries no
    // album_artist, so the detail pane hands us the artist name as the second
    // argument.
    {
        let track_list = Rc::downgrade(track_list);
        let stack = views.stack.clone();
        artist_view.set_on_album_activate(move |album: ArtistAlbum, artist: String| {
            let Some(track_list) = track_list.upgrade() else {
                return;
            };
            track_list.set_source(ViewSource::Album {
                album: album.album,
                album_artist: artist,
            });
            stack.set_visible_child_name(LIBRARY_VIEW_TRACKS);
        });
    }

    // Task 9: wire play_all/shuffle/add-to-queue + deep-link (needs PlayerController)

    let refresh = artist_view.refresh_callback();
    views.stack.connect_visible_child_name_notify(move |stack| {
        if stack.visible_child_name().as_deref() == Some(LIBRARY_VIEW_ARTISTS) {
            refresh();
        }
    });
}

fn smoke_library_view_name(value: &str) -> Option<&'static str> {
    match value {
        "tracks" => Some(LIBRARY_VIEW_TRACKS),
        "albums" => Some(LIBRARY_VIEW_ALBUMS),
        "artists" => Some(LIBRARY_VIEW_ARTISTS),
        _ => None,
    }
}

pub(in crate::ui) fn arm_smoke_library_view(views: &LibraryViews) {
    let Ok(value) = std::env::var(SMOKE_LIBRARY_VIEW_ENV) else {
        return;
    };
    let Some(name) = smoke_library_view_name(&value) else {
        tracing::warn!(value, "invalid library-view smoke target");
        return;
    };
    let stack = views.stack.clone();
    gtk4::glib::timeout_add_seconds_local_once(2, move || {
        stack.set_visible_child_name(name);
        tracing::info!(view = name, "smoke: opened library view");
    });
}

#[allow(clippy::too_many_arguments)]
pub(in crate::ui) fn wire_source_routing(
    sidebar: &Rc<Sidebar>,
    nav_history: &Rc<crate::ui::nav_history::NavHistory>,
    track_list: &Rc<TrackList>,
    stats_view: StatsView,
    conn: &Rc<RefCell<Connection>>,
    content_stack: &gtk4::Stack,
    device_view: &Rc<DeviceViewPage>,
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
    let device_view = device_view.clone();
    let conn = conn.clone();
    let show_content_on_select = show_content.clone();
    let nav_history = nav_history.clone();
    sidebar.set_on_select(move |source, source_name| {
        // NAV-2: every routed switch records the place it leaves. Back
        // re-routes through here too, silenced by its suppression flag.
        nav_history.record_route(&source);
        let is_library = matches!(source, ViewSource::Library);
        if let ViewSource::Device { serial } = &source {
            device_view.show_device(serial);
            content_stack.set_visible_child_name("device");
        } else if matches!(source, ViewSource::MyStats) {
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

pub(in crate::ui) fn build(
    window: &adw::ApplicationWindow,
    conn: &Rc<RefCell<Connection>>,
    sidebar: &Sidebar,
    content: &impl IsA<gtk4::Widget>,
    track_list: &Rc<TrackList>,
    player: Option<&Rc<PlayerController>>,
    runtime: &Rc<ArtistNewsRuntime>,
) -> LibraryShell {
    let sidebar_page = adw::NavigationPage::builder()
        .title(strings::text(strings::APP_NAME))
        .child(sidebar.widget())
        .build();
    let info_panel = InfoPanel::new(
        content,
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
    fn smoke_library_view_names_are_strict() {
        assert_eq!(smoke_library_view_name("tracks"), Some(LIBRARY_VIEW_TRACKS));
        assert_eq!(smoke_library_view_name("albums"), Some(LIBRARY_VIEW_ALBUMS));
        assert_eq!(
            smoke_library_view_name("artists"),
            Some(LIBRARY_VIEW_ARTISTS)
        );
        assert_eq!(smoke_library_view_name("Albums"), None);
        assert_eq!(smoke_library_view_name("unknown"), None);
    }

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
