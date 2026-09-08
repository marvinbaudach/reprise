use super::*;

pub(super) fn wire_compact_mode(w: &RuntimeWiring<'_>) {
    let RuntimeWiring {
        window,
        minimal_view,
        player,
        conn,
        preferences,
        toast_overlay,
        ..
    } = *w;
    let compact_preferences = preferences.clone();
    super::compact_mode_controls::install(
        window,
        minimal_view,
        player.as_ref().map(|player| &player.compact_player),
        conn,
        Rc::new(move || compact_preferences.present()),
    );
    super::startup_report::mark("compact_mode_controls::install");
    super::compact_mode_suggestion::install(window, toast_overlay, minimal_view, player.is_some());
}
