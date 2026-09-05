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
