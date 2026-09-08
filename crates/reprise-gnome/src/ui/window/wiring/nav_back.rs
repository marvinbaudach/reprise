use gtk4::prelude::*;
use reprise_core::view_source::ViewSource;

use super::*;

pub(super) fn wire_nav_back(w: &RuntimeWiring<'_>, scratch: &WiringScratch) {
    let RuntimeWiring {
        player,
        nav_history,
        sidebar,
        track_list,
        content_nav,
        content_stack,
        window_title,
        window,
        app,
        ..
    } = *w;
    if player.is_some() {
        // NAV-2 Back: pop the most recent place and route there without
        // re-recording (begin/end_back around the synchronous re-route).
        let back_action = gtk4::gio::SimpleAction::new("nav-back", None);
        {
            let nav_history = nav_history.clone();
            let sidebar = sidebar.clone();
            let track_list = track_list.clone();
            let content_nav = content_nav.clone();
            let content_stack = content_stack.clone();
            let window_title = window_title.clone();
            let active_content_focus = scratch.active_content_focus.clone();
            back_action.connect_activate(move |_, _| {
                let Some(place) = nav_history.go_back_from(track_list.browser_place()) else {
                    tracing::debug!("nav back: history is empty");
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
                    super::library_shell::ContentPages::new(&content_nav, &content_stack),
                    &window_title,
                    &active_content_focus,
                    "nav back",
                );
                nav_history.end_back();
            });
        }
        window.add_action(&back_action);
        app.set_accels_for_action("win.nav-back", &["<Alt>Left"]);

        // NAV-2 Forward: the browser counterpart — returns to the place the
        // last Back left, until a new navigation invalidates it.
        let forward_action = gtk4::gio::SimpleAction::new("nav-forward", None);
        {
            let nav_history = nav_history.clone();
            let sidebar = sidebar.clone();
            let track_list = track_list.clone();
            let content_nav = content_nav.clone();
            let content_stack = content_stack.clone();
            let window_title = window_title.clone();
            let active_content_focus = scratch.active_content_focus.clone();
            forward_action.connect_activate(move |_, _| {
                let Some(place) = nav_history.go_forward_from(track_list.browser_place()) else {
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
                    super::library_shell::ContentPages::new(&content_nav, &content_stack),
                    &window_title,
                    &active_content_focus,
                    "nav forward",
                );
                nav_history.end_back();
            });
        }
        window.add_action(&forward_action);
        app.set_accels_for_action("win.nav-forward", &["<Alt>Right"]);

        // Browser-style mouse navigation buttons: 8 (back) / 9 (forward)
        // fire the same actions as Alt+Left / Alt+Right. One gesture
        // listening to all buttons, claiming ONLY 8/9 so every other button
        // passes through untouched; capture phase on the toplevel so it
        // works over every view.
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

        // Dev/verification hook (permanent, like `REPRISE_SMOKE_ACTIVATE`):
        // `REPRISE_SMOKE_JUMP=1` fires the NAV-9b jump action ~2s after
        // startup (past the other smoke hooks' idle work) and the NAV-2
        // back action ~2s later — the exact same `gio` actions Ctrl+L and
        // Alt+Left run. Headless E2E asserts the resulting routing +
        // selection log lines.
        if std::env::var("REPRISE_SMOKE_JUMP").is_ok() {
            // Mirrors the acceptance repro: open Queue through the sidebar,
            // then jump, then back — each step two seconds apart, past
            // startup idle work.
            let sidebar_for_smoke = sidebar.clone();
            gtk4::glib::timeout_add_seconds_local_once(2, move || {
                tracing::info!("smoke: selecting queue via sidebar");
                sidebar_for_smoke.refresh_and_select(ViewSource::Queue, "smoke jump precondition");
            });
            let window_for_jump = window.clone();
            gtk4::glib::timeout_add_seconds_local_once(4, move || {
                tracing::info!("smoke: firing jump-to-now-playing");
                gtk4::gio::prelude::ActionGroupExt::activate_action(
                    &window_for_jump,
                    "jump-to-now-playing",
                    None,
                );
            });
            let window_for_back = window.clone();
            gtk4::glib::timeout_add_seconds_local_once(6, move || {
                tracing::info!("smoke: firing nav-back");
                gtk4::gio::prelude::ActionGroupExt::activate_action(
                    &window_for_back,
                    "nav-back",
                    None,
                );
            });
        }
    }
    super::startup_report::mark("navigation actions");
}
