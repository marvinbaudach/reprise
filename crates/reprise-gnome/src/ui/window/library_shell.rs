//! Main library/sidebar composition, including the contextual end panel.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use rusqlite::Connection;

use super::artist_news_worker::ArtistNewsRuntime;
use super::device_view::DeviceViewPage;
use super::info_panel::InfoPanel;
use super::now_playing_wiring;
use super::player_controller::PlayerController;
use super::scan::audio_analysis_runtime::AudioAnalysisRuntime;
use super::sidebar::Sidebar;
use super::stats_view::StatsView;
use super::strings;
use super::track_list::TrackList;
use crate::ui::nav_history::NavPlace;
use reprise_core::view_source::ViewSource;

pub(in crate::ui) const SIDEBAR_BREAKPOINT_WIDTH: i32 = 800;

pub(in crate::ui) struct LibraryShell {
    pub sidebar_page: adw::NavigationPage,
    pub split_view: adw::OverlaySplitView,
    pub content_nav: adw::NavigationView,
    pub info_panel: Rc<InfoPanel>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum ActiveContentTarget {
    Tracks,
    Stats,
    Device,
}

fn active_content_target(content_name: Option<&str>) -> Option<ActiveContentTarget> {
    match content_name {
        Some("stats") => Some(ActiveContentTarget::Stats),
        Some("device") => Some(ActiveContentTarget::Device),
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
    source_title: &adw::WindowTitle,
    show_content: Rc<dyn Fn()>,
    active_content_focus: &ActiveContentFocus,
) {
    let track_list = track_list.clone();
    let content_stack = content_stack.clone();
    let source_title = source_title.clone();
    let stats_view = Rc::new(stats_view);
    let device_view = device_view.clone();
    let conn = conn.clone();
    let sidebar_for_select = sidebar.clone();
    let show_content_on_select = show_content.clone();
    let nav_history = nav_history.clone();
    let active_content_focus = active_content_focus.clone();
    sidebar.set_on_select(move |source, source_name| {
        // NAV-2: every routed switch records the place it leaves. Back
        // re-routes through here too, silenced by its suppression flag.
        nav_history.record_route_from(
            &NavPlace::source(source.clone()),
            track_list.browser_place(),
        );
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
        if let ViewSource::Device { serial } = &source {
            device_view.show_device(serial);
            content_stack.set_visible_child_name("device");
        } else if matches!(source, ViewSource::MyStats) {
            stats_view.refresh(&conn);
            content_stack.set_visible_child_name("stats");
        } else if matches!(source, ViewSource::Conversions) {
            // INST-13: the conversion/staging view lives on its own page. Ensure
            // it is installed (under the same experimental gate as the sidebar
            // row) BEFORE selecting it — the row can appear after a live
            // toggle-on, so without this the selection would land on a missing
            // page and the content would silently stay put.
            crate::ui::instrumental::conversion_wiring::ensure_page_installed();
            content_stack.set_visible_child_name("conversions");
        } else {
            content_stack.set_visible_child_name("library");
            track_list.set_source(source);
        }
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

/// NAV-2/NAV-9b: routes to a remembered place — the re-entrant twin of
/// `wire_source_routing`'s `on_select` body, used by Back, Forward, and
/// the now-playing jump. Row-backed sources go through the sidebar to keep
/// highlight/title/adaptive navigation in sync. Album and Artist scopes have
/// no sidebar row, so they route directly into the same TrackList.
pub(in crate::ui) fn route_to_place(
    place: &NavPlace,
    sidebar: &Rc<Sidebar>,
    track_list: &Rc<TrackList>,
    content_stack: &gtk4::Stack,
    active_content_focus: &ActiveContentFocus,
    reason: &str,
) {
    tracing::debug!(
        source = %place.view_source().label(),
        reason,
        "history nav: routing to place"
    );
    let source = place.view_source();
    match &source {
        ViewSource::Album { .. } | ViewSource::Artist(_) => {
            content_stack.set_visible_child_name("library");
            let _ = track_list.restore_browser_place(place.browser_place());
            crate::ui::sidebar_session::sync_current_source(&sidebar.shared, &source);
        }
        _ => {
            crate::ui::sidebar_session::prepare_history_reroute(&sidebar.shared, &source);
            sidebar.refresh_and_select(source, reason);
            let _ = track_list.restore_browser_place(place.browser_place());
        }
    }
    // Keyboard flow: hand focus to the restored view (the Back/Forward twin
    // of direct scope navigation), so arrows / Enter / Menu key
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
    fn active_content_focus_resolves_every_shell_view() {
        assert_eq!(
            active_content_target(Some("library")),
            Some(ActiveContentTarget::Tracks)
        );
        assert_eq!(
            active_content_target(Some("stats")),
            Some(ActiveContentTarget::Stats)
        );
        assert_eq!(
            active_content_target(Some("device")),
            Some(ActiveContentTarget::Device)
        );
        assert_eq!(active_content_target(Some("unknown")), None);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_escape_focuses_the_current_shell_view() {
        gtk4::init().unwrap();
        let tracks = gtk4::Button::with_label("Tracks focus");
        let stats = gtk4::Button::with_label("Stats focus");
        let device = gtk4::Button::with_label("Device focus");
        let content = gtk4::Stack::new();
        content.add_named(&tracks, Some("library"));
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
        let focus = ActiveContentFocus::from_focus_action(&content, tracks_focus);

        for (content_name, expected) in [
            ("library", tracks.upcast_ref::<gtk4::Widget>()),
            ("stats", stats.upcast_ref()),
            ("device", device.upcast_ref()),
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
}
