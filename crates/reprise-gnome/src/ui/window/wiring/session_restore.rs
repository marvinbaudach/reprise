use std::path::PathBuf;

use reprise_core::browser::BrowserPlace;
use reprise_core::view_source::ViewSource;

use super::*;

pub(super) fn wire_session_restore(w: &RuntimeWiring<'_>, scratch: &WiringScratch) {
    let RuntimeWiring {
        player,
        session_state,
        nav_history,
        sidebar,
        track_list,
        content_nav,
        content_stack,
        window_title,
        window,
        conn,
        geometry_guard,
        preferences,
        db_path,
        scan_controls,
        toast_overlay,
        watcher_state,
        scan_button,
        first_run_decision,
        ..
    } = *w;
    super::session_restore_ui::restore_runtime(player.as_ref(), session_state);
    super::startup_report::mark("session_restore::restore_runtime");
    // START-3: restore the last visible place, but not the Back/Forward stack.
    // The Music root remains a separate remembered place so an absolute
    // sidebar click still restores its own refinements.
    let startup_place = super::session_restore_ui::startup_place(session_state);
    let library_root = session_state
        .library_root
        .clone()
        .unwrap_or_else(|| BrowserPlace::from(ViewSource::Library));
    nav_history.restore(startup_place.clone(), library_root);
    nav_history.begin_back();
    super::library_shell::route_to_place(
        &crate::ui::nav_history::NavPlace::browser(startup_place.clone()),
        sidebar,
        track_list,
        super::library_shell::ContentPages::new(content_nav, content_stack),
        window_title,
        &scratch.active_content_focus,
        "session restore",
    );
    super::startup_report::mark("route_to_place");
    nav_history.end_back();
    track_list.finish_startup_load(&startup_place);
    // START-3: the routing above owns the model; this owns the viewport.
    // Order matters — the view must exist before its rows can be centered.
    track_list.center_loaded_track();
    super::startup_report::mark("center_loaded_track");
    super::session_restore_ui::wire_close(
        window,
        conn,
        track_list,
        player.as_ref(),
        session_state,
        geometry_guard,
        nav_history,
    );
    super::session_restore_ui::arm_seed_close(window);
    let present_rhythmbox_import = {
        let preferences = Rc::downgrade(preferences);
        Rc::new(move || {
            if let Some(preferences) = preferences.upgrade() {
                preferences.present_rhythmbox_import_dialog();
            }
        }) as Rc<dyn Fn()>
    };
    let start_scan_of = {
        let db_path = db_path.to_path_buf();
        let scan_controls = scan_controls.clone();
        let toast_overlay = toast_overlay.clone();
        let track_list = track_list.clone();
        let sidebar = sidebar.clone();
        let watcher_state = watcher_state.clone();
        Rc::new(move |folder| {
            super::scan_worker::spawn_scan(
                folder,
                db_path.clone(),
                scan_controls.clone(),
                toast_overlay.clone(),
                track_list.clone(),
                sidebar.clone(),
                watcher_state.clone(),
            );
        }) as Rc<dyn Fn(PathBuf)>
    };
    super::first_run::run(
        window,
        scan_button,
        scan_controls,
        conn,
        first_run_decision,
        &start_scan_of,
        &present_rhythmbox_import,
    );
    super::startup_report::mark("first_run::run");
}
