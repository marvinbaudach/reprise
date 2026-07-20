//! Global Back/Forward actions and their header/mouse affordances.

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

use super::album_view::AlbumView;
use super::library_shell::{ActiveContentFocus, LibraryViews};
use super::nav_history::NavHistory;
use super::sidebar::Sidebar;
use super::track_list::TrackList;

#[derive(Clone, Copy)]
pub(in crate::ui) struct HistoryWiring<'a> {
    pub(in crate::ui) app: &'a adw::Application,
    pub(in crate::ui) window: &'a adw::ApplicationWindow,
    pub(in crate::ui) nav_history: &'a Rc<NavHistory>,
    pub(in crate::ui) sidebar: &'a Rc<Sidebar>,
    pub(in crate::ui) track_list: &'a Rc<TrackList>,
    pub(in crate::ui) content_stack: &'a gtk4::Stack,
    pub(in crate::ui) library_views: &'a LibraryViews,
    pub(in crate::ui) active_content_focus: &'a ActiveContentFocus,
    pub(in crate::ui) album_view: &'a AlbumView,
}

pub(in crate::ui) fn install(args: HistoryWiring<'_>) {
    let HistoryWiring {
        app,
        window,
        nav_history,
        sidebar,
        track_list,
        content_stack,
        library_views,
        active_content_focus,
        album_view,
    } = args;

    let back_action = gtk4::gio::SimpleAction::new("nav-back", None);
    {
        let back_action = back_action.downgrade();
        nav_history.connect_can_go_back_changed(move |available| {
            if let Some(back_action) = back_action.upgrade() {
                back_action.set_enabled(available);
            }
        });
    }
    {
        let nav_history = nav_history.clone();
        let sidebar = sidebar.clone();
        let track_list = track_list.clone();
        let content_stack = content_stack.clone();
        let library_stack = library_views.stack.clone();
        let active_content_focus = active_content_focus.clone();
        let restore_album_focus = album_view.restore_focus_callback();
        back_action.connect_activate(move |_, _| {
            let Some(place) = nav_history.go_back() else {
                tracing::debug!("nav back: history is empty");
                return;
            };
            let current_source = track_list.current_source();
            super::album_grid_reveal::route_back_restoring_album_focus(
                &current_source,
                &place,
                || {
                    nav_history.begin_back();
                    crate::ui::sidebar_session::sync_current_source(
                        &sidebar.shared,
                        &track_list.current_source(),
                    );
                    nav_history.record_route(&place);
                    super::library_shell::route_to_place(
                        &place,
                        &sidebar,
                        &track_list,
                        &content_stack,
                        &library_stack,
                        &active_content_focus,
                        "nav back",
                    );
                    nav_history.end_back();
                },
                &restore_album_focus,
            );
        });
    }
    window.add_action(&back_action);
    app.set_accels_for_action("win.nav-back", &["<Alt>Left"]);

    let forward_action = gtk4::gio::SimpleAction::new("nav-forward", None);
    {
        let nav_history = nav_history.clone();
        let sidebar = sidebar.clone();
        let track_list = track_list.clone();
        let content_stack = content_stack.clone();
        let library_stack = library_views.stack.clone();
        let active_content_focus = active_content_focus.clone();
        forward_action.connect_activate(move |_, _| {
            let Some(place) = nav_history.go_forward() else {
                tracing::debug!("nav forward: nothing ahead");
                return;
            };
            nav_history.begin_back();
            crate::ui::sidebar_session::sync_current_source(
                &sidebar.shared,
                &track_list.current_source(),
            );
            nav_history.record_route(&place);
            super::library_shell::route_to_place(
                &place,
                &sidebar,
                &track_list,
                &content_stack,
                &library_stack,
                &active_content_focus,
                "nav forward",
            );
            nav_history.end_back();
        });
    }
    window.add_action(&forward_action);
    app.set_accels_for_action("win.nav-forward", &["<Alt>Right"]);

    // input-parity: ACC-8 keyboard=alt-left-right
    let mouse_nav = gtk4::GestureClick::builder()
        .button(0)
        .propagation_phase(gtk4::PropagationPhase::Capture)
        .build();
    {
        let window = window.downgrade();
        mouse_nav.connect_pressed(move |gesture, _n, _x, _y| {
            let action = match gesture.current_button() {
                8 => "nav-back",
                9 => "nav-forward",
                _ => return,
            };
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            if let Some(window) = window.upgrade() {
                gtk4::gio::prelude::ActionGroupExt::activate_action(&window, action, None);
            }
        });
    }
    window.add_controller(mouse_nav);
}
