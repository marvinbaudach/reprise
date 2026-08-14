use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::concerts::ConcertRow;

use super::concerts_columns::{self, OnOpenTarget};
use super::concerts_model::{ConcertObject, ConcertsModel};

pub(super) fn activate_row(row: &ConcertRow, on_open: &OnOpenTarget) -> bool {
    let Some(target) = concerts_columns::ticket_target(row) else {
        return false;
    };
    on_open(target.to_owned());
    true
}

pub(super) fn wire(view: &gtk4::ColumnView, model: &Rc<ConcertsModel>, on_open: OnOpenTarget) {
    {
        let model = model.clone();
        let on_open = on_open.clone();
        view.connect_activate(move |_, position| {
            let Some(object) = model.store().item(position).and_downcast::<ConcertObject>() else {
                return;
            };
            activate_row(&object.row(), &on_open);
        });
    }
    let model = model.clone();
    let keys = gtk4::EventControllerKey::new();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key != gtk4::gdk::Key::space {
            return gtk4::glib::Propagation::Proceed;
        }
        let position = model.selection().selected();
        let Some(object) = model.store().item(position).and_downcast::<ConcertObject>() else {
            return gtk4::glib::Propagation::Proceed;
        };
        if !activate_row(&object.row(), &on_open) {
            return gtk4::glib::Propagation::Proceed;
        }
        gtk4::glib::Propagation::Stop
    });
    view.add_controller(keys);
}
