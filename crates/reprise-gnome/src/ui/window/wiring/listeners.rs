use super::*;

pub(super) fn wire_listeners(w: &RuntimeWiring<'_>) {
    let RuntimeWiring {
        track_list,
        sidebar,
        ..
    } = *w;
    let track_list_weak = Rc::downgrade(track_list);
    sidebar.set_on_tracks_added(move || match track_list_weak.upgrade() {
        Some(track_list) => track_list.reload(),
        None => tracing::warn!("track list reload skipped: track list is gone"),
    });
    let sidebar_weak = Rc::downgrade(sidebar);
    track_list.set_on_sidebar_playlist_drop(move |playlist_id, playlist_name, ids| {
        match sidebar_weak.upgrade() {
            Some(sidebar) => sidebar.handle_playlist_drop(playlist_id, playlist_name, ids),
            None => {
                tracing::warn!("sidebar is gone; cannot dispatch simulated playlist drop");
                false
            }
        }
    });
    let sidebar_weak = Rc::downgrade(sidebar);
    track_list.set_on_sidebar_queue_drop(move |ids| match sidebar_weak.upgrade() {
        Some(sidebar) => sidebar.handle_queue_drop(ids),
        None => {
            tracing::warn!("sidebar is gone; cannot dispatch simulated queue drop");
            false
        }
    });
}
