//! Scratch values shared by ordered runtime-wiring concerns.

use super::RuntimeWiring;

/// What the ordered wiring concerns share beyond [`RuntimeWiring`]. Values that
/// one concern builds for a later one are returned and passed explicitly, so a
/// concern cannot run before the one it needs.
pub(super) struct WiringScratch {
    pub(super) active_content_focus: super::library_shell::ActiveContentFocus,
}

impl WiringScratch {
    pub(super) fn new(w: &RuntimeWiring<'_>) -> Self {
        Self {
            active_content_focus: w.active_content_focus.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn runtime_wiring_groups_remain_in_load_bearing_order() {
        let source = include_str!("../window_runtime_wiring.rs");
        let wire_body = source
            .split_once("pub(in crate::ui) fn wire")
            .expect("runtime wire function must exist")
            .1;
        let mut cursor = 0;

        for call in [
            "deferred_sources::wire_deferred_sources",
            "library_doctor::wire_library_doctor",
            "compact_mode::wire_compact_mode",
            "menu::wire_menu",
            "playing_source::wire_playing_source",
            "sleep_timer::wire_sleep_timer",
            "nav_back::wire_nav_back",
            "section_search::wire_section_search",
            "quick_open_wiring::wire_quick_open",
            "clear_all::wire_clear_all",
            "listeners::wire_listeners",
            "view_session::wire_view_session",
            "close::wire_close",
            "session_restore::wire_session_restore",
            "deep_link::wire_deep_link",
        ] {
            let offset = wire_body[cursor..]
                .find(call)
                .unwrap_or_else(|| panic!("{call} must follow the preceding wiring group"));
            cursor += offset + call.len();
        }
    }
}
