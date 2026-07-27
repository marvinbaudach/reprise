use std::rc::Rc;

use gtk4::prelude::*;

use super::surface::Shared;

pub(super) fn first_issue_row(shared: &Shared) -> Option<gtk4::ListBoxRow> {
    shared
        .rows
        .borrow()
        .iter()
        .find(|(row, _, _)| row.parent().as_ref() == Some(shared.issues_listbox.upcast_ref()))
        .map(|(row, _, _)| row.clone())
}

pub(super) fn last_main_row(shared: &Shared) -> Option<gtk4::ListBoxRow> {
    shared
        .rows
        .borrow()
        .iter()
        .rev()
        .find(|(row, _, _)| row.parent().as_ref() == Some(shared.listbox.upcast_ref()))
        .map(|(row, _, _)| row.clone())
}

pub(super) fn wire_collection_boundary_navigation(shared: &Rc<Shared>) {
    let down = gtk4::EventControllerKey::new();
    down.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let shared_down = shared.clone();
    down.connect_key_pressed(move |_, key, _, modifiers| {
        if key != gtk4::gdk::Key::Down || !modifiers.is_empty() {
            return gtk4::glib::Propagation::Proceed;
        }
        let Some(current) = last_main_row(&shared_down) else {
            return gtk4::glib::Propagation::Proceed;
        };
        if !current.has_focus() {
            return gtk4::glib::Propagation::Proceed;
        }
        let Some(target) = first_issue_row(&shared_down) else {
            return gtk4::glib::Propagation::Proceed;
        };
        if target.grab_focus() {
            gtk4::glib::Propagation::Stop
        } else {
            gtk4::glib::Propagation::Proceed
        }
    });
    shared.listbox.add_controller(down);

    let up = gtk4::EventControllerKey::new();
    up.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let shared_up = shared.clone();
    up.connect_key_pressed(move |_, key, _, modifiers| {
        if key != gtk4::gdk::Key::Up || !modifiers.is_empty() {
            return gtk4::glib::Propagation::Proceed;
        }
        let Some(current) = first_issue_row(&shared_up) else {
            return gtk4::glib::Propagation::Proceed;
        };
        if !current.has_focus() {
            return gtk4::glib::Propagation::Proceed;
        }
        let Some(target) = last_main_row(&shared_up) else {
            return gtk4::glib::Propagation::Proceed;
        };
        if target.grab_focus() {
            gtk4::glib::Propagation::Stop
        } else {
            gtk4::glib::Propagation::Proceed
        }
    });
    shared.issues_listbox.add_controller(up);
}
