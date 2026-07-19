//! Main library/sidebar composition, including the contextual end panel.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use rusqlite::Connection;

use super::album_view::AlbumView;
use super::artist_news_worker::ArtistNewsRuntime;
use super::artist_portrait_worker::ArtistPortraitRuntime;
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
use crate::ui::nav_history::{NavHistory, NavPlace};
use reprise_core::queries::ArtistAlbum;
use reprise_core::view_source::ViewSource;

pub(in crate::ui) const SIDEBAR_BREAKPOINT_WIDTH: i32 = 800;
pub(in crate::ui) const LIBRARY_VIEW_TRACKS: &str = "tracks";
pub(in crate::ui) const LIBRARY_VIEW_ALBUMS: &str = "albums";
pub(in crate::ui) const LIBRARY_VIEW_ARTISTS: &str = "artists";
const SMOKE_LIBRARY_VIEW_ENV: &str = "REPRISE_SMOKE_LIBRARY_VIEW";

pub(in crate::ui) struct LibraryShell {
    pub sidebar_page: adw::NavigationPage,
    pub split_view: adw::OverlaySplitView,
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
    // Size to the visible page, not the widest one: a homogeneous stack would
    // reserve the (wide) track table's minimum width even while the Artists or
    // Albums page is shown, forcing the whole content — and the full-width
    // player bar below it — past the window edge (QA #3/#4).
    let stack = gtk4::Stack::builder()
        .hhomogeneous(false)
        .transition_type(gtk4::StackTransitionType::Crossfade)
        .transition_duration(crate::ui::motion::STANDARD_MS)
        .build();
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
    artist_view: &ArtistView,
    track_list: &Rc<TrackList>,
    nav_history: &Rc<NavHistory>,
) {
    // Card click / Enter → switch to Tracks tab with album source.
    let track_list_clone = track_list.clone();
    let stack = views.stack.downgrade();
    let nav_history_activate = nav_history.clone();
    album_view.set_on_activate(move |album| {
        let source = ViewSource::Album {
            album: album.album.clone(),
            album_artist: album.album_artist.clone(),
        };
        // NAV-2: cross-navigation bypasses the sidebar choke point, so it
        // records its route itself — Back must return to the album grid.
        nav_history_activate.record_route(&NavPlace::source(
            source.clone(),
            Some(LIBRARY_VIEW_TRACKS.to_owned()),
        ));
        track_list_clone.set_source(source);
        if let Some(stack) = stack.upgrade() {
            stack.set_visible_child_name(LIBRARY_VIEW_TRACKS);
        }
        // Keyboard flow: hand focus to the track table once the stack switch
        // has mapped it, so Enter → arrow keys works without a Tab detour.
        // Best-effort (idle so the new page is mapped); a `false` return is
        // logged, matching every other focus move in this codebase.
        let track_list_focus = Rc::downgrade(&track_list_clone);
        gtk4::glib::idle_add_local_once(move || {
            if let Some(track_list) = track_list_focus.upgrade() {
                if !track_list.focus_track_list() {
                    tracing::debug!("album activate: track list did not take focus");
                }
            }
        });
    });

    // Artist label click → switch to Artists tab.
    let stack = views.stack.downgrade();
    let nav_history_artist = nav_history.clone();
    let select_artist = artist_view.select_artist_callback();
    album_view.set_on_artist_activate(move |artist| {
        if artist.trim().is_empty() {
            return;
        }
        // A tab-only navigation: same source, Albums → Artists tab.
        nav_history_artist.record_tab_route(LIBRARY_VIEW_ARTISTS);
        if let Some(stack) = stack.upgrade() {
            stack.set_visible_child_name(LIBRARY_VIEW_ARTISTS);
            select_artist(&artist);
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
    nav_history: &Rc<NavHistory>,
) {
    // "Show all tracks" for the selected artist opens the track table.
    {
        let track_list = Rc::downgrade(track_list);
        let stack = views.stack.clone();
        let nav_history = nav_history.clone();
        artist_view.set_on_show_all_tracks(move |artist| {
            let Some(track_list) = track_list.upgrade() else {
                return;
            };
            let source = ViewSource::Artist(artist);
            // NAV-2: cross-navigation records its own route (see
            // `wire_album_view`) so Back returns to the Artists view.
            nav_history.record_route(&NavPlace::source(
                source.clone(),
                Some(LIBRARY_VIEW_TRACKS.to_owned()),
            ));
            track_list.set_source(source);
            stack.set_visible_child_name(LIBRARY_VIEW_TRACKS);
        });
    }

    // Activating an album card opens that album. `ArtistAlbum` carries no
    // album_artist, so the detail pane hands us the artist name as the second
    // argument.
    {
        let track_list = Rc::downgrade(track_list);
        let stack = views.stack.clone();
        let nav_history = nav_history.clone();
        artist_view.set_on_album_activate(move |album: ArtistAlbum, artist: String| {
            let Some(track_list) = track_list.upgrade() else {
                return;
            };
            let source = ViewSource::Album {
                album: album.album,
                album_artist: artist,
            };
            nav_history.record_route(&NavPlace::source(
                source.clone(),
                Some(LIBRARY_VIEW_TRACKS.to_owned()),
            ));
            track_list.set_source(source);
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
    let sidebar_for_select = sidebar.clone();
    let show_content_on_select = show_content.clone();
    let nav_history = nav_history.clone();
    // NAV-2: the Tracks/Albums/Artists switcher is a mode switch, not a
    // history entry — but the CURRENT place's tab must stay fresh so the
    // next push remembers the tab the user actually left (e.g. the album
    // grid).
    {
        let nav_history = nav_history.clone();
        views.stack.connect_visible_child_name_notify(move |stack| {
            if let Some(name) = stack.visible_child_name() {
                nav_history.note_library_tab(&name);
            }
        });
    }
    sidebar.set_on_select(move |source, source_name| {
        // NAV-2: every routed switch records the place it leaves. Back
        // re-routes through here too, silenced by its suppression flag.
        // Routed sources always land on the Tracks tab (set below); the
        // pushed PREVIOUS place carries whatever tab `note_library_tab`
        // last observed there.
        nav_history.record_route(&NavPlace::source(
            source.clone(),
            Some(LIBRARY_VIEW_TRACKS.to_owned()),
        ));
        let viewed = {
            let conn = conn.borrow();
            crate::ui::view_session::record_issue_viewed(
                &conn,
                &source,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            )
        };
        match viewed {
            Ok(true) => sidebar_for_select.refresh("issue view opened"),
            Ok(false) => {}
            Err(error) => tracing::error!(%error, "failed to record issue view as viewed"),
        }
        let is_library = matches!(source, ViewSource::Library);
        if let ViewSource::Device { serial } = &source {
            device_view.show_device(serial);
            content_stack.set_visible_child_name("device");
        } else if matches!(source, ViewSource::MyStats) {
            stats_view.refresh(&conn);
            content_stack.set_visible_child_name("stats");
        } else {
            content_stack.set_visible_child_name("library");
            track_list.set_source(source);
            library_stack.set_visible_child_name(LIBRARY_VIEW_TRACKS);
        }
        title.set_library_navigation_visible(is_library);
        source_title.set_title(&source_name);
        show_content_on_select();
    });
    sidebar.set_on_show_content(move || show_content());
}

/// Builds the library split view in its collapsed default. It starts with the
/// sidebar hidden rather than expanded: the wide breakpoint (and its
/// `collapsed`-notify wiring) reveals the sidebar column once the window is at
/// least [`SIDEBAR_BREAKPOINT_WIDTH`] wide, so a narrow restored width simply
/// leaves the sidebar closed instead of overlaying the content underneath it.
fn build_split_view(
    sidebar_page: &adw::NavigationPage,
    content_nav: &adw::NavigationView,
) -> adw::OverlaySplitView {
    sidebar_page.add_css_class("reprise-library-sidebar");
    let split = adw::OverlaySplitView::builder()
        .sidebar(sidebar_page)
        .content(content_nav)
        .sidebar_position(gtk4::PackType::Start)
        .show_sidebar(false)
        .collapsed(true)
        .build();
    split.add_css_class("reprise-library-split");
    split
}

/// NAV-2/NAV-9a/GRID-5: routes to a remembered place — the re-entrant twin of
/// `wire_source_routing`'s `on_select` body, used by Back, Forward, and
/// the now-playing jump. Row-backed sources go through the sidebar
/// (keeping highlight/title/adaptive nav in sync) with the remembered
/// library tab restored afterwards; the row-less detail sources
/// (`Album`/`Artist`) re-drive the cross-navigation path directly, because
/// `sidebar::rebuild` would substitute Library for a source without a row.
pub(in crate::ui) fn route_to_place(
    place: &NavPlace,
    sidebar: &Rc<Sidebar>,
    track_list: &Rc<TrackList>,
    content_stack: &gtk4::Stack,
    library_stack: &gtk4::Stack,
    album_grid: &gtk4::GridView,
    reason: &str,
) {
    tracing::debug!(
        source = %place.source.label(),
        tab = place.library_tab.as_deref().unwrap_or(""),
        reason,
        "history nav: routing to place"
    );
    if place.is_new_releases() {
        content_stack.set_visible_child_name("new-releases");
        let content_stack = content_stack.downgrade();
        gtk4::glib::idle_add_local_once(move || {
            let granted = content_stack
                .upgrade()
                .and_then(|stack| stack.visible_child())
                .is_some_and(|child| child.child_focus(gtk4::DirectionType::TabForward));
            if !granted {
                tracing::debug!("history nav: New Releases digest did not take focus");
            }
        });
        return;
    }
    match &place.source {
        ViewSource::Album { .. } | ViewSource::Artist(_) => {
            content_stack.set_visible_child_name("library");
            track_list.set_source(place.source.clone());
            library_stack.set_visible_child_name(
                place.library_tab.as_deref().unwrap_or(LIBRARY_VIEW_TRACKS),
            );
            crate::ui::sidebar_session::sync_current_source(&sidebar.shared, &place.source);
        }
        _ => {
            sidebar.refresh_and_select(place.source.clone(), reason);
            if let Some(tab) = &place.library_tab {
                library_stack.set_visible_child_name(tab);
            }
        }
    }
    // Keyboard flow: hand focus to the restored view (the Back/Forward twin
    // of `wire_album_view`'s focus-follow), so arrows / Enter / Menu key
    // keep working without a Tab detour. Best-effort in an idle (the stack
    // switch must map the page first); a `false` is logged like every other
    // focus move in this codebase.
    let restored_tab = place
        .library_tab
        .clone()
        .unwrap_or_else(|| LIBRARY_VIEW_TRACKS.to_owned());
    let track_list_focus = Rc::downgrade(track_list);
    let album_grid_focus = album_grid.downgrade();
    let library_stack_focus = library_stack.downgrade();
    gtk4::glib::idle_add_local_once(move || {
        let granted = if restored_tab == LIBRARY_VIEW_TRACKS {
            track_list_focus
                .upgrade()
                .is_some_and(|track_list| track_list.focus_track_list())
        } else if restored_tab == LIBRARY_VIEW_ALBUMS {
            // The album GRID, not the page's first focusable (that would be
            // the sort dropdown): focus returns to the focused card so
            // arrows / Enter / Menu key keep working immediately.
            album_grid_focus
                .upgrade()
                .is_some_and(|grid| grid.grab_focus())
        } else {
            // Artists page: focus its first focusable child (the artist
            // master list).
            library_stack_focus
                .upgrade()
                .and_then(|stack| stack.visible_child())
                .is_some_and(|child| child.child_focus(gtk4::DirectionType::TabForward))
        };
        if !granted {
            tracing::debug!("history nav: restored view did not take focus");
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(in crate::ui) fn build(
    window: &adw::ApplicationWindow,
    conn: &Rc<RefCell<Connection>>,
    sidebar: &Sidebar,
    content: &impl IsA<gtk4::Widget>,
    track_list: &Rc<TrackList>,
    player: Option<&Rc<PlayerController>>,
    runtime: &Rc<ArtistNewsRuntime>,
    portraits: &Rc<ArtistPortraitRuntime>,
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
        portraits,
        track_list.shared_cover_loader(),
    );
    if let Some(player) = player {
        player.set_lyrics_view(&info_panel.lyrics_view());
    }
    let content_nav = now_playing_wiring::build_content_nav(
        info_panel.widget(),
        &strings::text(strings::APP_NAME),
    );
    let split_view = build_split_view(&sidebar_page, &content_nav);
    super::sidebar_presentation::style_overlay_split_view(&split_view);
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
    fn sidebar_split_view_starts_closed_so_narrow_restores_never_overlay_content() {
        gtk4::init().unwrap();
        let sidebar_page = adw::NavigationPage::builder()
            .title("Sidebar")
            .child(&gtk4::Label::new(Some("Sidebar")))
            .build();
        let content = adw::NavigationView::new();

        let split = build_split_view(&sidebar_page, &content);

        assert!(split.is_collapsed());
        assert_eq!(split.sidebar_position(), gtk4::PackType::Start);
        assert!(
            !split.shows_sidebar(),
            "a narrow (sub-breakpoint) restored window must not start with the \
             sidebar overlaid on top of the content"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn library_split_is_scoped_for_chrome_separators() {
        gtk4::init().unwrap();
        let sidebar_page = adw::NavigationPage::builder()
            .title("Sidebar")
            .child(&gtk4::Label::new(Some("Sidebar")))
            .build();
        let content = adw::NavigationView::new();

        let split = build_split_view(&sidebar_page, &content);

        assert!(split.has_css_class("reprise-library-split"));
        assert!(sidebar_page.has_css_class("reprise-library-sidebar"));
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
        assert_eq!(
            views.stack.transition_type(),
            gtk4::StackTransitionType::Crossfade
        );
        assert_eq!(
            views.stack.transition_duration(),
            crate::ui::motion::STANDARD_MS
        );
    }
}
