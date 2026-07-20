//! NAV-9b coordinator for Ctrl+L and the player artist: reveal and select the
//! loaded track in its play origin.

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
    pub active_content_focus: library_shell::ActiveContentFocus,
}

pub(in crate::ui) fn runtime_coordinator(context: &JumpContext) -> JumpCallback {
    let player_for_origin = context.player.clone();
    let player_for_notify = context.player.clone();
    let sidebar_for_prepare = context.sidebar.clone();
    let track_list_for_sync = context.track_list.clone();
    let sidebar_for_route = context.sidebar.clone();
    let track_list_for_route = context.track_list.clone();
    let nav_history_for_route = context.nav_history.clone();
    let content_stack_for_route = context.content_stack.clone();
    let active_content_focus_for_route = context.active_content_focus.clone();

    coordinator(JumpSteps {
        current_origin: Rc::new(move || {
            let player = player_for_origin.upgrade()?;
            Some(
                player
                    .current_play_origin()
                    .map_or(ViewSource::Library, |origin| origin.source),
            )
        }),
        prepare_origin: Rc::new(move |_origin| {
            crate::ui::sidebar_session::sync_current_source(
                &sidebar_for_prepare.shared,
                &track_list_for_sync.current_source(),
            );
        }),
        route_origin: Rc::new(move |origin| {
            let place = NavPlace::source(origin.clone());
            nav_history_for_route.record_route_from(&place, track_list_for_route.browser_place());
            library_shell::route_to_place(
                &place,
                &sidebar_for_route,
                &track_list_for_route,
                &content_stack_for_route,
                &active_content_focus_for_route,
                "jump to current track origin",
            );
        }),
        notify_current_track: Rc::new(move || {
            if let Some(player) = player_for_notify.upgrade() {
                player.notify_revealed_current_track();
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
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_9b_ctrl_l_and_player_artist_reveal_current_track_origin() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        let events = Rc::new(RefCell::new(Vec::new()));
        let history = Rc::new(NavHistory::default());
        let queue = NavPlace::source(ViewSource::Queue);
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
                    history.record_route(&NavPlace::source(source.clone()));
                    events.borrow_mut().push(("route", source.clone()));
                })
            },
            notify_current_track: {
                let events = events.clone();
                Rc::new(move || {
                    events
                        .borrow_mut()
                        .push(("reveal-with-selection", ViewSource::Playlist(7)));
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
                ("reveal-with-selection", ViewSource::Playlist(7)),
            ]
        );
        assert_eq!(history.go_back(), Some(queue));
    }

    #[test]
    fn nav_9b_player_artist_uses_the_ctrl_l_jump_and_selects_the_track() {
        let runtime = include_str!("window_runtime_wiring.rs")
            .split_whitespace()
            .collect::<String>();
        assert_eq!(
            runtime
                .matches("letjump_from_artist=jump_to_current_track.clone();player.connect_artist_clicked(move||jump_from_artist());")
                .count(),
            1,
            "the player artist must activate the same current-track jump as Ctrl+L"
        );
        assert!(
            runtime.contains("jump_action.connect_activate(move|_,_|jump_to_current_track());"),
            "Ctrl+L must retain the shared current-track jump"
        );

        let selection = include_str!("../track_list/current_track_selection.rs")
            .split_whitespace()
            .collect::<String>();
        assert!(
            selection.contains("CurrentTrackChange::ExplicitReveal=>{self.shared.selection.select_item(position,true);"),
            "an explicit jump must replace selection with the playing row"
        );

        let old_wiring = include_str!("window_action_wiring.rs");
        assert!(
            !old_wiring.contains("player.connect_artist_clicked"),
            "the obsolete artist-master deep link must be removed"
        );
    }
}
