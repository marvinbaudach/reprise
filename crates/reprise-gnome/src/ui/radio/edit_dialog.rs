use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::radio::{self, StationRow};
use rusqlite::Connection;

use crate::ui::strings;

pub(super) fn present(
    parent: &impl IsA<gtk4::Widget>,
    conn: Rc<RefCell<Connection>>,
    station: &StationRow,
    on_saved: impl Fn() + 'static,
) {
    let name = adw::EntryRow::builder()
        .title(strings::text(strings::RADIO_STATION))
        .text(&station.name)
        .build();
    let genre = adw::EntryRow::builder()
        .title(strings::text(strings::RADIO_GENRE))
        .text(station.genre.as_deref().unwrap_or_default())
        .build();
    let stream = adw::EntryRow::builder()
        .title(strings::text(strings::RADIO_DIALOG_HINT))
        .text(&station.stream_url)
        .build();
    let list = gtk4::ListBox::new();
    list.add_css_class("boxed-list");
    list.append(&name);
    list.append(&genre);
    list.append(&stream);

    let cancel = gtk4::Button::with_label(&strings::text(strings::RADIO_CANCEL));
    let save = gtk4::Button::with_label(&strings::text(strings::TAG_SAVE));
    save.add_css_class("suggested-action");
    let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    buttons.set_halign(gtk4::Align::End);
    buttons.append(&cancel);
    buttons.append(&save);
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_top(16);
    content.set_margin_bottom(16);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.append(&list);
    content.append(&buttons);

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(
        &strings::text(strings::RADIO_EDIT),
        "",
    )));
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));
    let station_id = station.id;
    let dialog = adw::Dialog::builder()
        .child(&toolbar)
        .content_width(480)
        .build();
    {
        let dialog = dialog.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        });
    }
    {
        let dialog = dialog.downgrade();
        save.connect_clicked(move |_| {
            let name_value = name.text();
            let genre_value = genre.text();
            let stream_value = stream.text();
            if name_value.trim().is_empty()
                || !(stream_value.starts_with("http://") || stream_value.starts_with("https://"))
            {
                return;
            }
            match radio::station::update_details(
                &conn.borrow(),
                station_id,
                name_value.trim(),
                (!genre_value.trim().is_empty()).then(|| genre_value.trim()),
                stream_value.trim(),
            ) {
                Ok(_) => {
                    on_saved();
                    if let Some(dialog) = dialog.upgrade() {
                        dialog.close();
                    }
                }
                Err(error) => tracing::warn!(%error, "could not update radio station"),
            }
        });
    }
    dialog.present(Some(parent));
}
