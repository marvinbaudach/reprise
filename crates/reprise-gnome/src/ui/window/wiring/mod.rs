//! Scratch values shared by ordered runtime-wiring concerns.

use std::cell::OnceCell;
use std::rc::Rc;

use super::{library_doctor_ui, section_search_ui, RuntimeWiring};

pub(super) struct WiringScratch {
    pub(super) active_content_focus: super::library_shell::ActiveContentFocus,
    pub(super) library_doctor: OnceCell<Rc<library_doctor_ui::LibraryDoctorLauncher>>,
    pub(super) section_search: OnceCell<Rc<section_search_ui::SectionSearch>>,
}

impl WiringScratch {
    pub(super) fn new(w: &RuntimeWiring<'_>) -> Self {
        Self {
            active_content_focus: w.active_content_focus.clone(),
            library_doctor: OnceCell::new(),
            section_search: OnceCell::new(),
        }
    }

    pub(super) fn library_doctor(&self) -> &Rc<library_doctor_ui::LibraryDoctorLauncher> {
        self.library_doctor
            .get()
            .expect("library doctor wiring must run first")
    }

    pub(super) fn section_search(&self) -> &Rc<section_search_ui::SectionSearch> {
        self.section_search
            .get()
            .expect("section search wiring must run first")
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
            "nav_back::wire_nav_back",
            "section_search::wire_section_search",
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
