//! NAV-9a coordinator for Ctrl+L: reveal the loaded track's play origin.

use std::rc::{Rc, Weak};

use reprise_core::view_source::ViewSource;

use super::library_shell;
use crate::ui::nav_history::{NavHistory, NavPlace};
use crate::ui::player_controller::PlayerController;
use crate::ui::sidebar::Sidebar;
use crate::ui::track_list::TrackList;

pub(in crate::ui) type JumpCallback = Rc<dyn Fn()>;

pub(in crate::ui) struct JumpSteps {
    pub current_origin: Rc<dyn Fn() -> Option<ViewSource>>,
    pub prepare_origin: Rc<dyn Fn(&ViewSource)>,
    pub route_origin: Rc<dyn Fn(&ViewSource)>,
    pub notify_current_track: Rc<dyn Fn()>,
}

pub(in crate::ui) struct JumpContext {
    pub player: Weak<PlayerController>,
    pub sidebar: Rc<Sidebar>,
    pub track_list: Rc<TrackList>,
    pub nav_history: Rc<NavHistory>,
    pub content_stack: gtk4::Stack,
    pub library_stack: libadwaita::ViewStack,
    pub album_grid: gtk4::GridView,
}

pub(in crate::ui) fn runtime_coordinator(context: &JumpContext) -> JumpCallback {
    let player_for_origin = context.player.clone();
    let player_for_notify = context.player.clone();
    let track_list_for_prepare = context.track_list.clone();
    let sidebar_for_prepare = context.sidebar.clone();
    let track_list_for_sync = context.track_list.clone();
    let sidebar_for_route = context.sidebar.clone();
    let track_list_for_route = context.track_list.clone();
    let nav_history_for_route = context.nav_history.clone();
    let content_stack_for_route = context.content_stack.clone();
    let library_stack_for_route = context.library_stack.clone();
    let album_grid_for_route = context.album_grid.clone();

    coordinator(JumpSteps {
        current_origin: Rc::new(move || {
            let player = player_for_origin.upgrade()?;
            Some(
                player
                    .current_play_origin()
                    .map_or(ViewSource::Library, |origin| origin.source),
            )
        }),
        prepare_origin: Rc::new(move |origin| {
            track_list_for_prepare.forget_view_state(origin);
            crate::ui::sidebar_session::sync_current_source(
                &sidebar_for_prepare.shared,
                &track_list_for_sync.current_source(),
            );
        }),
        route_origin: Rc::new(move |origin| {
            let place = NavPlace::source(
                origin.clone(),
                Some(library_shell::LIBRARY_VIEW_TRACKS.to_owned()),
            );
            nav_history_for_route.record_route(&place);
            library_shell::route_to_place(
                &place,
                &sidebar_for_route,
                &track_list_for_route,
                &content_stack_for_route,
                &library_stack_for_route,
                &album_grid_for_route,
                "jump to current track origin",
            );
        }),
        notify_current_track: Rc::new(move || {
            if let Some(player) = player_for_notify.upgrade() {
                player.notify_restored_current_track();
            }
        }),
    })
}

pub(in crate::ui) fn coordinator(steps: JumpSteps) -> JumpCallback {
    Rc::new(move || {
        let Some(origin) = (steps.current_origin)() else {
            return;
        };
        (steps.prepare_origin)(&origin);
        (steps.route_origin)(&origin);
        let notify_current_track = steps.notify_current_track.clone();
        gtk4::glib::idle_add_local_once(move || {
            notify_current_track();
        });
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[test]
    fn nav_9a_ctrl_l_reveals_current_track_origin() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let history = Rc::new(NavHistory::default());
        let queue = NavPlace::source(
            ViewSource::Queue,
            Some(library_shell::LIBRARY_VIEW_TRACKS.into()),
        );
        history.record_route(&queue);
        let jump = coordinator(JumpSteps {
            current_origin: Rc::new(|| Some(ViewSource::Playlist(7))),
            prepare_origin: {
                let events = events.clone();
                Rc::new(move |source| events.borrow_mut().push(("prepare", source.clone())))
            },
            route_origin: {
                let events = events.clone();
                let history = history.clone();
                Rc::new(move |source| {
                    history.record_route(&NavPlace::source(
                        source.clone(),
                        Some(library_shell::LIBRARY_VIEW_TRACKS.into()),
                    ));
                    events.borrow_mut().push(("route", source.clone()));
                })
            },
            notify_current_track: {
                let events = events.clone();
                Rc::new(move || {
                    events
                        .borrow_mut()
                        .push(("reveal-without-selection", ViewSource::Playlist(7)));
                })
            },
        });

        jump();
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert_eq!(
            &*events.borrow(),
            &[
                ("prepare", ViewSource::Playlist(7)),
                ("route", ViewSource::Playlist(7)),
                ("reveal-without-selection", ViewSource::Playlist(7)),
            ]
        );
        assert_eq!(history.go_back(), Some(queue));
    }
}
