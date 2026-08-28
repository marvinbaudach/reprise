//! Main library/sidebar composition, including the contextual end panel.

use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;

use super::artist_news_worker::ArtistNewsRuntime;
use super::info_panel::InfoPanel;
use super::now_playing_wiring;
use super::player_controller::PlayerController;
use super::sidebar::Sidebar;
use super::stats_view::StatsView;
use super::strings;
use super::track_list::TrackList;
use crate::ui::nav_history::NavPlace;
use reprise_core::view_source::ViewSource;

pub(in crate::ui) const SIDEBAR_BREAKPOINT_WIDTH: i32 = 800;
pub(in crate::ui) const SIDEBAR_COLLAPSE_WIDTH: i32 = SIDEBAR_BREAKPOINT_WIDTH - 1;

pub(in crate::ui) struct LibraryShell {
    pub root: adw::BreakpointBin,
    pub sidebar_page: adw::NavigationPage,
    pub split_view: adw::OverlaySplitView,
    pub content_nav: adw::NavigationView,
    pub info_panel: Rc<InfoPanel>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum ActiveContentTarget {
    Tracks,
    Stats,
    Concerts,
    Releases,
    Podcasts,
    Youtube,
    Radio,
    LibraryDoctor,
}

fn active_content_target(content_name: Option<&str>) -> Option<ActiveContentTarget> {
    match content_name {
        Some("stats") => Some(ActiveContentTarget::Stats),
        Some("concerts") => Some(ActiveContentTarget::Concerts),
        Some("releases") => Some(ActiveContentTarget::Releases),
        Some("podcasts") => Some(ActiveContentTarget::Podcasts),
        Some("youtube") => Some(ActiveContentTarget::Youtube),
        Some("radio") => Some(ActiveContentTarget::Radio),
        Some("library-doctor") => Some(ActiveContentTarget::LibraryDoctor),
        Some("library") => Some(ActiveContentTarget::Tracks),
        _ => None,
    }
}

#[derive(Clone)]
pub(in crate::ui) struct ActiveContentFocus {
    content_stack: glib::WeakRef<gtk4::Stack>,
    focus_tracks: Rc<dyn Fn() -> bool>,
}

impl ActiveContentFocus {
    pub(in crate::ui) fn new(content_stack: &gtk4::Stack, track_list: &Rc<TrackList>) -> Self {
        let track_list = Rc::downgrade(track_list);
        let focus_tracks = Rc::new(move || {
            track_list
                .upgrade()
                .is_some_and(|track_list| track_list.focus_visible_content())
        });
        Self::from_focus_action(content_stack, focus_tracks)
    }

    fn from_focus_action(content_stack: &gtk4::Stack, focus_tracks: Rc<dyn Fn() -> bool>) -> Self {
        Self {
            content_stack: content_stack.downgrade(),
            focus_tracks,
        }
    }

    pub(in crate::ui) fn focus(&self) -> bool {
        let Some(content_stack) = self.content_stack.upgrade() else {
            return false;
        };
        let content_name = content_stack.visible_child_name();
        match active_content_target(content_name.as_deref()) {
            Some(ActiveContentTarget::Tracks) => (self.focus_tracks)(),
            Some(
                ActiveContentTarget::Stats
                | ActiveContentTarget::Concerts
                | ActiveContentTarget::Releases
                | ActiveContentTarget::Podcasts
                | ActiveContentTarget::Youtube
                | ActiveContentTarget::Radio
                | ActiveContentTarget::LibraryDoctor,
            ) => content_stack
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

/// Dev/verification hook for non-track content views.
fn arm_smoke_detail_view(sidebar: &Rc<Sidebar>) {
    let Ok(value) = std::env::var(crate::ui::track_list::track_list_smoke::SMOKE_SOURCE_ENV_VAR)
    else {
        return;
    };
    let Some(
        source @ (ViewSource::MyStats
        | ViewSource::Concerts
        | ViewSource::Releases
        | ViewSource::Podcasts
        | ViewSource::Youtube
        | ViewSource::Radio),
    ) = crate::ui::track_list::track_list_smoke::parse_smoke_source(&value)
    else {
        return;
    };
    let sidebar = sidebar.clone();
    gtk4::glib::idle_add_local_once(move || {
        tracing::info!(source = %source.label(), "smoke: opening detail view through sidebar source routing");
        sidebar.refresh_and_select(source, "smoke detail source");
    });
}

#[allow(clippy::too_many_arguments)]
pub(in crate::ui) fn wire_source_routing(
    sidebar: &Rc<Sidebar>,
    nav_history: &Rc<crate::ui::nav_history::NavHistory>,
    track_list: &Rc<TrackList>,
    stats_view: &super::content_stack::DeferredPage<StatsView>,
    concerts_view: &super::content_stack::DeferredPage<crate::ui::concerts::ConcertsView>,
    releases_view: &Rc<crate::ui::releases::ReleasesView>,
    podcasts_view: &super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
    youtube_view: &super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
    radio_view: &super::content_stack::DeferredPage<crate::ui::radio::RadioView>,
    conn: &Rc<Db>,
    content_navigation: &adw::NavigationView,
    content_stack: &gtk4::Stack,
    source_title: &adw::WindowTitle,
    show_content: Rc<dyn Fn()>,
    active_content_focus: &ActiveContentFocus,
    section_search: &Rc<super::section_search::SectionSearch>,
) {
    sidebar.bind_content_stack(content_stack);
    let track_list = track_list.clone();
    let content_navigation = content_navigation.clone();
    let content_stack = content_stack.clone();
    let source_title = source_title.clone();
    let stats_view = stats_view.clone();
    let concerts_view = concerts_view.clone();
    let releases_view = releases_view.clone();
    let podcasts_view = podcasts_view.clone();
    let youtube_view = youtube_view.clone();
    let radio_view = radio_view.clone();
    let conn = conn.clone();
    let sidebar_for_select = sidebar.clone();
    let show_content_on_select = show_content.clone();
    let nav_history = nav_history.clone();
    let active_content_focus = active_content_focus.clone();
    let section_search = section_search.clone();
    sidebar.set_on_select(move |source, source_name| {
        // NAV-2: every routed switch records the place it leaves. Back
        // re-routes through here too, silenced by its suppression flag.
        nav_history.record_route_from(
            &NavPlace::source(source.clone()),
            track_list.browser_place(),
        );
        let viewed = {
            let conn = &conn;
            crate::ui::view_session::record_issue_viewed(
                conn,
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
        // SEARCH-8a: switch the query sink before routing the view. This
        // clears an ordinary destination, while a Back route subsequently
        // restores its complete history-owned BrowserPlace through the
        // track list. Search itself keeps no origin or per-view memory.
        section_search.activate_source(&source, &source_name);
        if matches!(source, ViewSource::MyStats) {
            super::window_navigation::show_content_page(
                &content_navigation,
                &content_stack,
                "stats",
            );
            let stats_view = stats_view.materialize();
            stats_view.prepare_entrance();
            stats_view.refresh(&conn);
        } else if matches!(source, ViewSource::Concerts) {
            super::window_navigation::show_content_page(
                &content_navigation,
                &content_stack,
                "concerts",
            );
            concerts_view.materialize().refresh();
        } else if matches!(source, ViewSource::Releases) {
            releases_view.refresh();
            super::window_navigation::show_content_page(
                &content_navigation,
                &content_stack,
                "releases",
            );
        } else if matches!(source, ViewSource::Podcasts) {
            super::window_navigation::show_content_page(
                &content_navigation,
                &content_stack,
                "podcasts",
            );
            let podcasts_view = podcasts_view.materialize();
            podcasts_view.refresh();
            podcasts_view.request_tab_open_refresh();
        } else if matches!(source, ViewSource::Youtube) {
            super::window_navigation::show_content_page(
                &content_navigation,
                &content_stack,
                "youtube",
            );
            let youtube_view = youtube_view.materialize();
            youtube_view.refresh();
            youtube_view.request_tab_open_refresh();
        } else if matches!(source, ViewSource::Radio) {
            super::window_navigation::show_content_page(
                &content_navigation,
                &content_stack,
                "radio",
            );
            radio_view.materialize().refresh();
        } else {
            super::window_navigation::show_content_page(
                &content_navigation,
                &content_stack,
                "library",
            );
            track_list.set_source(source.clone());
        }
        source_title.set_title(&source_name);
        show_content_on_select();
        active_content_focus.focus_later();
    });
    sidebar.set_on_show_content(move || show_content());
    arm_smoke_detail_view(sidebar);
}

struct LibrarySplit {
    root: adw::BreakpointBin,
    split: adw::OverlaySplitView,
}

/// Builds the library split with its wide, pinned state as the default.
///
/// The local breakpoint bin owns the narrow overlay mode. Unlike a window
/// breakpoint, it cannot lose arbitration to another responsive concern, and
/// unapplying it restores the captured wide-state value.
fn build_split_view(
    sidebar_page: &adw::NavigationPage,
    content_nav: &adw::NavigationView,
) -> LibrarySplit {
    sidebar_page.add_css_class("reprise-library-sidebar");
    let split = adw::OverlaySplitView::builder()
        .sidebar(sidebar_page)
        .content(content_nav)
        .sidebar_position(gtk4::PackType::Start)
        .show_sidebar(false)
        .collapsed(false)
        .pin_sidebar(true)
        .build();
    split.add_css_class("reprise-library-split");
    let condition = adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        f64::from(SIDEBAR_COLLAPSE_WIDTH),
        adw::LengthUnit::Px,
    );
    let breakpoint = adw::Breakpoint::new(condition);
    breakpoint.add_setter(&split, "collapsed", Some(&true.to_value()));
    let root = adw::BreakpointBin::new();
    root.set_size_request(1, 1);
    root.set_child(Some(&split));
    root.add_breakpoint(breakpoint);
    LibrarySplit { root, split }
}

/// NAV-2/NAV-9b: routes to a remembered place — the re-entrant twin of
/// `wire_source_routing`'s `on_select` body, used by Back, Forward, and
/// the now-playing jump. Row-backed sources go through the sidebar to keep
/// highlight/title/adaptive navigation in sync. Album, Artist, and Genre scopes have
/// no sidebar row, so they route directly into the same TrackList.
#[derive(Clone, Copy)]
pub(in crate::ui) struct ContentPages<'a> {
    navigation: &'a adw::NavigationView,
    stack: &'a gtk4::Stack,
}

impl<'a> ContentPages<'a> {
    pub(in crate::ui) fn new(navigation: &'a adw::NavigationView, stack: &'a gtk4::Stack) -> Self {
        Self { navigation, stack }
    }

    fn show(self, name: &str) {
        super::window_navigation::show_content_page(self.navigation, self.stack, name);
    }
}

pub(in crate::ui) fn route_to_place(
    place: &NavPlace,
    sidebar: &Rc<Sidebar>,
    track_list: &Rc<TrackList>,
    content_pages: ContentPages<'_>,
    source_title: &adw::WindowTitle,
    active_content_focus: &ActiveContentFocus,
    reason: &str,
) {
    route_to_place_with_viewport(
        place,
        sidebar,
        track_list,
        content_pages,
        source_title,
        active_content_focus,
        reason,
        crate::ui::view_session::BrowserPlaceViewport::PreserveAnchor,
    );
}

pub(in crate::ui) fn route_to_place_centering_anchor(
    place: &NavPlace,
    sidebar: &Rc<Sidebar>,
    track_list: &Rc<TrackList>,
    content_pages: ContentPages<'_>,
    source_title: &adw::WindowTitle,
    active_content_focus: &ActiveContentFocus,
    reason: &str,
) {
    route_to_place_with_viewport(
        place,
        sidebar,
        track_list,
        content_pages,
        source_title,
        active_content_focus,
        reason,
        crate::ui::view_session::BrowserPlaceViewport::CenterAnchor,
    );
}

// These window-owned collaborators stay explicit so this routing seam does not
// create a second state holder.
#[allow(clippy::too_many_arguments)]
fn route_to_place_with_viewport(
    place: &NavPlace,
    sidebar: &Rc<Sidebar>,
    track_list: &Rc<TrackList>,
    content_pages: ContentPages<'_>,
    source_title: &adw::WindowTitle,
    active_content_focus: &ActiveContentFocus,
    reason: &str,
    viewport: crate::ui::view_session::BrowserPlaceViewport,
) {
    tracing::debug!(
        source = %place.view_source().label(),
        reason,
        "history nav: routing to place"
    );
    // History navigation (Back/Forward) is a deliberate destination change and
    // must release the reveal intent so the restore writers can land the user
    // where they were, not where a reveal left the viewport. Session restore
    // also routes through here and clears at that point, which is harmless:
    // `center_loaded_track()` runs after the restore completes.
    track_list
        .shared
        .scroll_glide
        .clear_deliberate_destination();
    let source = place.view_source();
    match &source {
        ViewSource::Album { .. } | ViewSource::Artist(_) | ViewSource::Genre(_) => {
            sidebar.ensure_startup_build();
            content_pages.show("library");
            let _ = crate::ui::view_session::restore_browser_place_with_viewport(
                track_list,
                place.browser_place(),
                viewport,
            );
            crate::ui::sidebar_session::sync_current_source(&sidebar.shared, &source);
            source_title.set_title(&scope_title(&source));
        }
        _ => {
            crate::ui::sidebar_session::prepare_history_reroute(&sidebar.shared, &source);
            sidebar.refresh_and_select(source, reason);
            let _ = crate::ui::view_session::restore_browser_place_with_viewport(
                track_list,
                place.browser_place(),
                viewport,
            );
        }
    }
    // Keyboard flow: hand focus to the restored view (the Back/Forward twin
    // of direct scope navigation), so arrows / Enter / Menu key
    // keep working without a Tab detour. Best-effort in an idle (the stack
    // switch must map the page first); a `false` is logged like every other
    // focus move in this codebase.
    active_content_focus.focus_later();
}

fn scope_title(source: &ViewSource) -> String {
    crate::ui::browse::filter_restriction::place_pill_label(source)
        .unwrap_or_else(|| source.label())
}

#[allow(clippy::too_many_arguments)]
pub(in crate::ui) fn build(
    _window: &adw::ApplicationWindow,
    conn: &Rc<Db>,
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
    let library_split = build_split_view(&sidebar_page, &content_nav);
    let split_view = library_split.split;
    super::sidebar_presentation::style_overlay_split_view(&split_view);
    LibraryShell {
        root: library_split.root,
        sidebar_page,
        split_view,
        content_nav,
        info_panel,
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn window_attached_collapsed_setters(source: &str) -> Vec<String> {
        let compact: String = source
            .chars()
            .filter(|char| !char.is_whitespace())
            .collect();
        compact
            .split(';')
            .filter(|statement| {
                statement.contains(".add_setter(") && statement.contains("\"collapsed\"")
            })
            .filter_map(|statement| {
                let prefix = statement.split_once(".add_setter(")?.0;
                prefix
                    .rsplit(|char: char| !(char.is_ascii_alphanumeric() || char == '_'))
                    .next()
                    .map(str::to_owned)
            })
            .filter(|breakpoint| compact.contains(&format!("window.add_breakpoint({breakpoint})")))
            .collect()
    }

    #[test]
    fn browse_1_music_builds_only_the_canonical_track_surface() {
        let window = include_str!("window.rs");
        let chrome = include_str!("library_chrome.rs");

        for obsolete in [
            "AlbumView::new",
            "ArtistView::new",
            "build_views(",
            "build_library_title(",
        ] {
            assert!(
                !window.contains(obsolete),
                "the main Music shell still constructs the obsolete `{obsolete}` surface"
            );
        }
        assert!(
            !chrome.contains("InlineViewSwitcher"),
            "the global header must not expose parallel Tracks/Albums/Artists modes"
        );
    }

    #[test]
    fn window_breakpoints_never_own_split_view_collapse() {
        for (name, source) in [
            ("library shell", include_str!("library_shell.rs")),
            (
                "responsive side panels",
                include_str!("responsive_side_panels.rs"),
            ),
            (
                "compact mode suggestion",
                include_str!("../compact/compact_mode_suggestion.rs"),
            ),
        ] {
            let offenders = window_attached_collapsed_setters(source);
            assert!(
                offenders.is_empty(),
                "{name} attaches collapsed-setter breakpoints {offenders:?} to the window"
            );
        }
    }

    #[test]
    fn breakpoint_guard_recognizes_a_multiline_collapsed_setter() {
        let broken = concat!(
            "legacy.add_setter(&split_view, \"collapsed\", Some(&false.to_value()));\n",
            "window.",
            "add_breakpoint(legacy);\n",
            "breakpoint.add_setter(\n",
            "    &split_view,\n",
            "    \"collapsed\",\n",
            "    Some(&false.to_value()),\n",
            ");\n",
            "window.",
            "add_breakpoint(breakpoint);\n",
        );

        assert_eq!(
            window_attached_collapsed_setters(broken),
            ["legacy", "breakpoint"]
        );
    }

    #[test]
    fn active_content_focus_resolves_every_shell_view() {
        assert_eq!(
            active_content_target(Some("library")),
            Some(ActiveContentTarget::Tracks)
        );
        assert_eq!(
            active_content_target(Some("stats")),
            Some(ActiveContentTarget::Stats)
        );
        assert_eq!(active_content_target(Some("device")), None);
        assert_eq!(
            active_content_target(Some("concerts")),
            Some(ActiveContentTarget::Concerts)
        );
        assert_eq!(
            active_content_target(Some("releases")),
            Some(ActiveContentTarget::Releases)
        );
        assert_eq!(
            active_content_target(Some("podcasts")),
            Some(ActiveContentTarget::Podcasts)
        );
        assert_eq!(
            active_content_target(Some("radio")),
            Some(ActiveContentTarget::Radio)
        );
        assert_eq!(
            active_content_target(Some("library-doctor")),
            Some(ActiveContentTarget::LibraryDoctor)
        );
        assert_eq!(active_content_target(Some("unknown")), None);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_escape_focuses_the_current_shell_view() {
        gtk4::init().unwrap();
        let tracks = gtk4::Button::with_label("Tracks focus");
        let stats = gtk4::Button::with_label("Stats focus");
        let content = gtk4::Stack::new();
        content.add_named(&tracks, Some("library"));
        content.add_named(&stats, Some("stats"));
        content.set_visible_child_name("library");
        let window = gtk4::Window::builder().child(&content).build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let tracks_focus = {
            let tracks = tracks.downgrade();
            Rc::new(move || tracks.upgrade().is_some_and(|widget| widget.grab_focus()))
        };
        let focus = ActiveContentFocus::from_focus_action(&content, tracks_focus);

        for (content_name, expected) in [
            ("library", tracks.upcast_ref::<gtk4::Widget>()),
            ("stats", stats.upcast_ref()),
        ] {
            content.set_visible_child_name(content_name);
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
    fn style_7_sidebar_reserves_a_real_slot_at_1024_px() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let sidebar_page = adw::NavigationPage::builder()
            .title("Sidebar")
            .child(&gtk4::Label::new(Some("Sidebar")))
            .build();
        let content_label = gtk4::Label::new(Some("Content"));
        let content = adw::NavigationView::new();
        content.add(
            &adw::NavigationPage::builder()
                .title("Content")
                .child(&content_label)
                .build(),
        );

        let shell = build_split_view(&sidebar_page, &content);
        shell.split.set_show_sidebar(true);
        let window = gtk4::Window::builder().child(&shell.root).build();
        window.set_default_size(1_024, 768);
        window.set_size_request(1_024, 768);
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let content_bounds = content
            .compute_bounds(&window)
            .expect("content must share the window coordinate space");
        assert!(
            !shell.split.is_collapsed(),
            "1024 px library split entered overlay mode: window={} root={} content={content_bounds:?}",
            window.width(),
            shell.root.width(),
        );
        assert!(
            content_bounds.x() > 100.0,
            "the open library sidebar did not reserve a content slot: {content_bounds:?}"
        );
        window.close();
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

        let split = build_split_view(&sidebar_page, &content).split;

        assert!(split.has_css_class("reprise-library-split"));
        assert!(sidebar_page.has_css_class("reprise-library-sidebar"));
    }
}
