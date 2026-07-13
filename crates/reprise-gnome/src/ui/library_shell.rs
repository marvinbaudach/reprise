//! Main library/sidebar composition, including the contextual end panel.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use rusqlite::Connection;

use super::artist_news_worker::ArtistNewsRuntime;
use super::info_panel::InfoPanel;
use super::now_playing_wiring;
use super::player_controller::PlayerController;
use super::sidebar::Sidebar;
use super::strings;
use super::track_list::TrackList;

pub(super) const SIDEBAR_BREAKPOINT_WIDTH: i32 = 800;

pub(super) struct LibraryShell {
    pub sidebar_page: adw::NavigationPage,
    pub split_view: adw::NavigationSplitView,
    pub content_nav: adw::NavigationView,
    pub info_panel: Rc<InfoPanel>,
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
