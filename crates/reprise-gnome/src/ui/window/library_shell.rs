//! Main library/sidebar composition, including the contextual end panel.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
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
use super::scan::audio_analysis_runtime::AudioAnalysisRuntime;
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
const LIBRARY_VIEW_TRACKS_ICON: &str = "view-list-symbolic";
const LIBRARY_VIEW_ALBUMS_ICON: &str = "media-optical-cd-audio-symbolic";
const LIBRARY_VIEW_ARTISTS_ICON: &str = "avatar-default-symbolic";
const SMOKE_LIBRARY_VIEW_ENV: &str = "REPRISE_SMOKE_LIBRARY_VIEW";

pub(in crate::ui) struct LibraryShell {
    pub sidebar_page: adw::NavigationPage,
    pub split_view: adw::OverlaySplitView,
    pub content_nav: adw::NavigationView,
    pub info_panel: Rc<InfoPanel>,
}

pub(in crate::ui) struct LibraryViews {
    pub(in crate::ui) stack: adw::ViewStack,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum ActiveContentTarget {
    Tracks,
    Albums,
    Artists,
    Stats,
    Device,
}

fn active_content_target(
    content_name: Option<&str>,
    library_name: Option<&str>,
) -> Option<ActiveContentTarget> {
    match content_name {
        Some("stats") => Some(ActiveContentTarget::Stats),
        Some("device") => Some(ActiveContentTarget::Device),
        Some("library") => match library_name {
            Some(LIBRARY_VIEW_TRACKS) => Some(ActiveContentTarget::Tracks),
            Some(LIBRARY_VIEW_ALBUMS) => Some(ActiveContentTarget::Albums),
            Some(LIBRARY_VIEW_ARTISTS) => Some(ActiveContentTarget::Artists),
            _ => None,
        },
        _ => None,
    }
}

#[derive(Clone)]
pub(in crate::ui) struct ActiveContentFocus {
    content_stack: glib::WeakRef<gtk4::Stack>,
    library_stack: glib::WeakRef<adw::ViewStack>,
    focus_tracks: Rc<dyn Fn() -> bool>,
    focus_albums: Rc<dyn Fn() -> bool>,
}

impl ActiveContentFocus {
    pub(in crate::ui) fn new(
        content_stack: &gtk4::Stack,
        library_stack: &adw::ViewStack,
        track_list: &Rc<TrackList>,
        album_grid: &gtk4::GridView,
    ) -> Self {
        let track_list = Rc::downgrade(track_list);
        let focus_tracks = Rc::new(move || {
            track_list
                .upgrade()
                .is_some_and(|track_list| track_list.focus_visible_content())
        });
        let album_grid = album_grid.downgrade();
        let focus_albums =
            Rc::new(move || album_grid.upgrade().is_some_and(|grid| grid.grab_focus()));
        Self::from_focus_actions(content_stack, library_stack, focus_tracks, focus_albums)
    }

    fn from_focus_actions(
        content_stack: &gtk4::Stack,
        library_stack: &adw::ViewStack,
        focus_tracks: Rc<dyn Fn() -> bool>,
        focus_albums: Rc<dyn Fn() -> bool>,
    ) -> Self {
        Self {
            content_stack: content_stack.downgrade(),
            library_stack: library_stack.downgrade(),
            focus_tracks,
            focus_albums,
        }
    }

    pub(in crate::ui) fn focus(&self) -> bool {
        let (Some(content_stack), Some(library_stack)) =
            (self.content_stack.upgrade(), self.library_stack.upgrade())
        else {
            return false;
        };
        let content_name = content_stack.visible_child_name();
        let library_name = library_stack.visible_child_name();
        match active_content_target(content_name.as_deref(), library_name.as_deref()) {
            Some(ActiveContentTarget::Tracks) => (self.focus_tracks)(),
            Some(ActiveContentTarget::Albums) => (self.focus_albums)(),
            Some(ActiveContentTarget::Artists) => library_stack
                .visible_child()
                .is_some_and(|child| focus_widget_or_descendant(&child)),
            Some(ActiveContentTarget::Stats | ActiveContentTarget::Device) => content_stack
                .visible_child()
                .is_some_and(|child| focus_widget_or_descendant(&child)),
            None => false,
        }
    }

    pub(in crate::ui) fn focus_later(&self) {
        let focus = self.clone();
        glib::idle_add_local_once(move || {
            if !focus.focus() {
                tracing::debug!("active content did not take focus");
            }
        });
    }

    pub(in crate::ui) fn focus_later_if_unset(&self, window: &adw::ApplicationWindow) {
        let focus = self.clone();
        let window = window.downgrade();
        glib::idle_add_local_once(move || {
            let Some(window) = window.upgrade() else {
                return;
            };
            if window.is_active()
                && gtk4::prelude::GtkWindowExt::focus(&window).is_none()
                && !focus.focus()
            {
                tracing::debug!("startup content did not take focus");
            }
        });
    }
}

fn focus_widget_or_descendant(widget: &gtk4::Widget) -> bool {
    widget.grab_focus() || widget.child_focus(gtk4::DirectionType::TabForward)
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
    let stack = adw::ViewStack::builder()
        .hhomogeneous(false)
        .transition_duration(crate::ui::motion::STANDARD_MS)
        .build();
    stack.add_titled_with_icon(
        tracks,
        Some(LIBRARY_VIEW_TRACKS),
        &strings::text(strings::LIBRARY_VIEW_TRACKS),
        LIBRARY_VIEW_TRACKS_ICON,
    );
    stack.add_titled_with_icon(
        albums,
        Some(LIBRARY_VIEW_ALBUMS),
        &strings::text(strings::LIBRARY_VIEW_ALBUMS),
        LIBRARY_VIEW_ALBUMS_ICON,
    );
    stack.add_titled_with_icon(
        artists,
        Some(LIBRARY_VIEW_ARTISTS),
        &strings::text(strings::LIBRARY_VIEW_ARTISTS),
        LIBRARY_VIEW_ARTISTS_ICON,
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
                if !track_list.focus_visible_content() {
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

/// Dev/verification hook for the row-less My Stats content view. The shared
/// `REPRISE_SMOKE_SOURCE=my_stats` spelling stays consistent with track-list
/// sources, but this is armed only after source routing exists so it exercises
/// the same sidebar callback that refreshes and reveals the stats stack page.
fn arm_smoke_my_stats(sidebar: &Rc<Sidebar>) {
    let Ok(value) = std::env::var(crate::ui::track_list::track_list_smoke::SMOKE_SOURCE_ENV_VAR)
    else {
        return;
    };
    if !matches!(
        crate::ui::track_list::track_list_smoke::parse_smoke_source(&value),
        Some(ViewSource::MyStats)
    ) {
        return;
    }
    let sidebar = sidebar.clone();
    gtk4::glib::idle_add_local_once(move || {
        tracing::info!("smoke: opening My Stats through sidebar source routing");
        sidebar.refresh_and_select(ViewSource::MyStats, "smoke My Stats source");
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
    active_content_focus: &ActiveContentFocus,
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
    let active_content_focus = active_content_focus.clone();
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
        active_content_focus.focus_later();
    });
    sidebar.set_on_show_content(move || show_content());
    arm_smoke_my_stats(sidebar);
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

/// NAV-2/NAV-9b/GRID-5: routes to a remembered place — the re-entrant twin of
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
    library_stack: &adw::ViewStack,
    active_content_focus: &ActiveContentFocus,
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
            crate::ui::sidebar_session::prepare_history_reroute(&sidebar.shared, &place.source);
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
    active_content_focus.focus_later();
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
    audio_analysis: Option<&AudioAnalysisRuntime>,
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
        audio_analysis,
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
    fn active_content_focus_resolves_every_shell_view() {
        assert_eq!(
            active_content_target(Some("library"), Some("tracks")),
            Some(ActiveContentTarget::Tracks)
        );
        assert_eq!(
            active_content_target(Some("library"), Some("albums")),
            Some(ActiveContentTarget::Albums)
        );
        assert_eq!(
            active_content_target(Some("library"), Some("artists")),
            Some(ActiveContentTarget::Artists)
        );
        assert_eq!(
            active_content_target(Some("stats"), Some("tracks")),
            Some(ActiveContentTarget::Stats)
        );
        assert_eq!(
            active_content_target(Some("device"), Some("tracks")),
            Some(ActiveContentTarget::Device)
        );
        assert_eq!(active_content_target(Some("unknown"), Some("tracks")), None);
        assert_eq!(
            active_content_target(Some("library"), Some("unknown")),
            None
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_escape_focuses_the_current_shell_view() {
        gtk4::init().unwrap();
        let tracks = gtk4::Button::with_label("Tracks focus");
        let albums = gtk4::Button::with_label("Albums focus");
        let artists = gtk4::Button::with_label("Artists focus");
        let stats = gtk4::Button::with_label("Stats focus");
        let device = gtk4::Button::with_label("Device focus");
        let views = build_views(&tracks, &albums, &artists);
        let content = gtk4::Stack::new();
        content.add_named(&views.stack, Some("library"));
        content.add_named(&stats, Some("stats"));
        content.add_named(&device, Some("device"));
        content.set_visible_child_name("library");
        let window = gtk4::Window::builder().child(&content).build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let tracks_focus = {
            let tracks = tracks.downgrade();
            Rc::new(move || tracks.upgrade().is_some_and(|widget| widget.grab_focus()))
        };
        let albums_focus = {
            let albums = albums.downgrade();
            Rc::new(move || albums.upgrade().is_some_and(|widget| widget.grab_focus()))
        };
        let focus = ActiveContentFocus::from_focus_actions(
            &content,
            &views.stack,
            tracks_focus,
            albums_focus,
        );

        for (content_name, library_name, expected) in [
            ("library", "tracks", tracks.upcast_ref::<gtk4::Widget>()),
            ("library", "albums", albums.upcast_ref()),
            ("library", "artists", artists.upcast_ref()),
            ("stats", "tracks", stats.upcast_ref()),
            ("device", "tracks", device.upcast_ref()),
        ] {
            content.set_visible_child_name(content_name);
            views.stack.set_visible_child_name(library_name);
            assert!(focus.focus());
            assert_eq!(
                gtk4::prelude::GtkWindowExt::focus(&window).as_ref(),
                Some(expected)
            );
        }

        window.close();
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
            views.stack.transition_duration(),
            crate::ui::motion::STANDARD_MS
        );
    }
}
