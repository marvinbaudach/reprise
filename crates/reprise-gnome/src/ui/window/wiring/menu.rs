use gtk4::prelude::*;

use super::*;

pub(super) fn wire_menu(w: &RuntimeWiring<'_>, scratch: &WiringScratch) {
    let RuntimeWiring {
        minimal_view,
        preferences,
        player,
        conn,
        db_path,
        window,
        content_stack,
        content_nav,
        track_list,
        concerts_view,
        releases_view,
        radio_view,
        header,
        sidebar,
        app,
        scan_controls,
        ..
    } = *w;
    let minimal_toggle = minimal_view.clone();
    let menu_preferences = preferences.clone();
    let findings_library_doctor = scratch.library_doctor().clone();
    let menu_library_doctor = scratch.library_doctor().clone();
    let stop_player = player.as_ref().map(|player| {
        let player = Rc::downgrade(player);
        Rc::new(move || {
            if let Some(player) = player.upgrade() {
                player.reset_to_stopped();
            }
        }) as Rc<dyn Fn()>
    });
    // Built here rather than in `window.rs` for the same reason the fingerprint
    // backend above is: this is where the window layer may name a platform
    // concrete, and the composition root is held below 600 lines.
    let spectrogram_batch = super::spectrogram_backend::build(conn.clone(), db_path.to_path_buf());
    super::startup_report::mark("spectrogram_backend::build");
    let active_table = super::table_columns::active_table(
        window,
        content_stack,
        content_nav,
        track_list,
        concerts_view,
        releases_view,
        radio_view,
    );
    super::primary_menu::install(
        header,
        window,
        &active_table,
        super::primary_menu::Callbacks {
            on_minimal_view: Rc::new(move || minimal_toggle.toggle()),
            on_library_doctor: Rc::new(move || menu_library_doctor.open()),
            on_library_doctor_findings: Rc::new(move || findings_library_doctor.open_findings()),
            on_import_playlist: {
                let sidebar = sidebar.clone();
                Rc::new(move || sidebar.activate_import_playlist())
            },
            on_stop_playback: stop_player,
            on_preferences: Rc::new(move || menu_preferences.present()),
            on_about: {
                let window = window.clone();
                let conn = conn.clone();
                let db_path = db_path.to_path_buf();
                Rc::new(move || crate::ui::about::present(&window, &conn, &db_path))
            },
        },
    );
    super::startup_report::mark("primary_menu::install");
    app.set_accels_for_action("win.open-primary-menu", &["F10"]);
    super::spectrogram_batch_progress::install(scan_controls, &spectrogram_batch);
    {
        // The menu action used to own the batch for the window lifetime.
        // Completion now owns it instead, so the idle start and later scans
        // still reach the same resumable batch after that action is gone.
        let batch = spectrogram_batch.clone();
        scan_controls.add_on_complete(move || batch.start());
    }
    // The colour of the seek bar arrives the way its shape already does: by
    // itself. The run is resumable, so a library that is already analyzed ends
    // it immediately and shows nothing; the scan card cancels a visible run.
    // This automatic start shares the one post-frame quiet gate. A completed
    // user-requested scan still starts immediately through the callback above.
    {
        let batch = spectrogram_batch.clone();
        super::startup_quiet::run_after_quiet(move || batch.start());
    }
}
