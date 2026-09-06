use gtk4::prelude::*;

use super::*;

pub(super) fn wire_clear_all(w: &RuntimeWiring<'_>, scratch: &WiringScratch) {
    let RuntimeWiring {
        window,
        track_list,
        metadata_navigator,
        lyrics_batch,
        cover_batch,
        session_state,
        conn,
        app,
        sidebar_toggle,
        split_view,
        sidebar_page,
        content_nav,
        sidebar,
        nav_history,
        stats_view,
        concerts_view,
        releases_view,
        podcasts_view,
        youtube_view,
        radio_view,
        content_stack,
        window_title,
        library_doctor_navigation,
        db_path,
        podcasts_runtime,
        ..
    } = *w;
    let clear_all = gtk4::gio::SimpleAction::new("clear-all-filters", None);
    {
        let section_search = scratch.section_search().clone();
        clear_all.connect_activate(move |_, _| {
            // FIL-2a: the current view only — its query and its facets.
            section_search.clear_all();
        });
    }
    window.add_action(&clear_all);
    {
        let section_search = scratch.section_search().clone();
        track_list.set_on_search_cleared(move || {
            section_search.clear_active_query();
        });
    }
    {
        let window = window.clone();
        track_list.set_on_clear_all(move || {
            gtk4::prelude::ActionGroupExt::activate_action(&window, "clear-all-filters", None);
        });
    }
    {
        let navigator = metadata_navigator.clone();
        track_list.set_on_scope_cleared(move || {
            navigator.leave_scope();
        });
    }

    {
        let lyrics_batch = lyrics_batch.clone();
        let cover_batch = cover_batch.clone();
        let previous_session = session_state.clone();
        let current_library_root = reprise_core::library::settings::get_library_root(conn)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "could not read library root for lyrics due-check");
                None
            });
        super::startup_quiet::run_after_quiet(move || {
            lyrics_batch.start_after_cover(
                &cover_batch,
                &previous_session,
                current_library_root.as_deref(),
            );
        });
    }
    app.set_accels_for_action("win.toggle-minimal-view", &["<Control>m"]);
    app.set_accels_for_action("win.preferences", &["<Control>comma"]);
    app.set_accels_for_action("win.keyboard-shortcuts", &["<Control>question"]);
    app.set_accels_for_action("win.help", &[super::help::HELP_ACCELERATOR]);

    super::window_navigation::wire_sidebar_toggle(sidebar_toggle, split_view, sidebar_page, conn);
    let show_content_if_collapsed =
        super::window_navigation::show_content_callback(split_view, content_nav);
    super::library_shell::wire_source_routing(
        sidebar,
        nav_history,
        track_list,
        stats_view,
        concerts_view,
        releases_view,
        podcasts_view,
        youtube_view,
        radio_view,
        conn,
        content_nav,
        content_stack,
        window_title,
        show_content_if_collapsed,
        &scratch.active_content_focus,
        scratch.section_search(),
    );
    {
        let track_list = track_list.clone();
        scratch
            .section_search()
            .observe(content_stack, window_title, move || {
                track_list.current_source()
            });
    }
    scratch
        .section_search()
        .observe_doctor_review(library_doctor_navigation);
    podcasts_view.on_materialized({
        let conn = conn.clone();
        let db_path = db_path.to_path_buf();
        let podcasts_runtime = podcasts_runtime.clone();
        move |podcasts_view| {
            super::podcast_refresh_scheduler::arm(
                &conn,
                &db_path,
                &podcasts_runtime,
                podcasts_view,
            );
        }
    });
}
