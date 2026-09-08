use super::*;

pub(super) fn wire_view_session(w: &RuntimeWiring<'_>, scratch: &WiringScratch) {
    let RuntimeWiring {
        search_entry,
        track_list,
        sidebar,
        window_title,
        app,
        window,
        search,
        player,
        ..
    } = *w;
    let search_restore_guard = super::view_session_ui::new_search_restore_guard();
    {
        // SEARCH-8a: the track list answers to the header entry only while a
        // track section is the visible one. A query typed in Podcasts must
        // not silently re-filter Music behind the user's back.
        let section_search = scratch.section_search().clone();
        super::view_session_ui::wire_search(
            search_entry,
            track_list.clone(),
            search_restore_guard.clone(),
            Rc::new(move || {
                section_search.is_active(reprise_view::search_scope::SearchScope::Tracks)
                    || section_search.is_active(reprise_view::search_scope::SearchScope::Missing)
            }),
        );
    }
    super::view_session_ui::arm_smoke(
        search_entry,
        track_list,
        sidebar,
        window_title,
        &search_restore_guard,
    );
    let focus_active_content: Rc<dyn Fn() -> bool> = {
        let active_content_focus = scratch.active_content_focus.clone();
        Rc::new(move || active_content_focus.focus())
    };
    super::shortcuts::wire(
        app,
        window,
        search,
        super::shortcuts::ShortcutHooks::for_section_search(
            focus_active_content,
            scratch.section_search().clone(),
        ),
        player.clone(),
    );
}
