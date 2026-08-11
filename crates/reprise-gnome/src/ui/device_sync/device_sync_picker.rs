//! The device page's single playlists picker (`MTP-51`).

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::device_sync::EVERYTHING_SOURCE;

use super::device_sync_runtime::{
    DeviceSyncRuntime, PickerPlaylistRow, PickerSave, PickerSnapshot,
};
use super::device_sync_strings;

#[derive(Clone)]
struct PickerDraft {
    original: PickerSnapshot,
    current: PickerSnapshot,
    filter: String,
}

type RefreshFn = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

pub(super) fn present(parent: &gtk4::Widget, runtime: &Rc<DeviceSyncRuntime>, device_id: &str) {
    let Ok(snapshot) = runtime.picker_snapshot(device_id) else {
        return;
    };
    let draft = Rc::new(RefCell::new(PickerDraft {
        original: snapshot.clone(),
        current: snapshot,
        filter: String::new(),
    }));

    let dialog = adw::Dialog::builder()
        .title(device_sync_strings::text(
            device_sync_strings::CHOOSE_PLAYLISTS,
        ))
        .content_width(560)
        .content_height(680)
        .build();
    let header = adw::HeaderBar::new();
    let cancel = gtk4::Button::with_label(&device_sync_strings::text(device_sync_strings::CANCEL));
    let save = gtk4::Button::with_label(&device_sync_strings::text(device_sync_strings::SAVE));
    save.add_css_class("suggested-action");
    header.pack_start(&cancel);
    header.pack_end(&save);

    let filter = gtk4::SearchEntry::new();
    filter.set_placeholder_text(Some(&device_sync_strings::text(
        device_sync_strings::FILTER_SYNC_CONTENT,
    )));
    let select_all =
        gtk4::Button::with_label(&device_sync_strings::text(device_sync_strings::SELECT_ALL));
    let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    filter.set_hexpand(true);
    controls.append(&filter);
    controls.append(&select_all);

    let smart_label = gtk4::Label::new(Some(&device_sync_strings::text(
        device_sync_strings::KEEP_SMART_PLAYLISTS_UPDATED,
    )));
    smart_label.set_hexpand(true);
    smart_label.set_xalign(0.0);
    smart_label.set_wrap(true);
    let smart_toggle = gtk4::Switch::new();
    smart_toggle.set_active(draft.borrow().current.keep_smart_updated);
    let smart_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    smart_row.append(&smart_label);
    smart_row.append(&smart_toggle);

    let list = gtk4::ListBox::new();
    list.set_selection_mode(gtk4::SelectionMode::None);
    list.set_show_separators(true);
    let scroller = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .child(&list)
        .build();
    let footer = gtk4::Label::new(None);
    footer.add_css_class("dim-label");
    footer.set_xalign(0.0);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.append(&controls);
    content.append(&smart_row);
    content.append(&scroller);
    content.append(&footer);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));
    dialog.set_child(Some(&toolbar));

    let refresh: RefreshFn = Rc::new(RefCell::new(None));
    let refresh_impl: Rc<dyn Fn()> = {
        let draft = draft.clone();
        let list = list.clone();
        let footer = footer.clone();
        let refresh = refresh.clone();
        Rc::new(move || {
            rebuild_list(&list, &draft, &refresh);
            update_footer(&footer, &draft.borrow().current);
        })
    };
    *refresh.borrow_mut() = Some(refresh_impl.clone());

    {
        let draft = draft.clone();
        let refresh = refresh.clone();
        filter.connect_search_changed(move |entry| {
            draft.borrow_mut().filter = entry.text().to_string();
            call_refresh(&refresh);
        });
    }
    {
        let draft = draft.clone();
        let refresh = refresh.clone();
        select_all.connect_clicked(move |_| {
            select_all_rows(&mut draft.borrow_mut().current);
            call_refresh(&refresh);
        });
    }
    {
        let draft = draft.clone();
        smart_toggle.connect_state_set(move |_, active| {
            draft.borrow_mut().current.keep_smart_updated = active;
            gtk4::glib::Propagation::Proceed
        });
    }
    {
        let dialog = dialog.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        });
    }
    {
        let runtime = runtime.clone();
        let device_id = device_id.to_string();
        let draft = draft.clone();
        let dialog = dialog.downgrade();
        save.connect_clicked(move |_| {
            if runtime
                .save_picker(&device_id, picker_changes(&draft.borrow()))
                .is_ok()
            {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.close();
                }
            }
        });
    }

    refresh_impl();
    dialog.present(Some(parent));
}

fn rebuild_list(list: &gtk4::ListBox, draft: &Rc<RefCell<PickerDraft>>, refresh: &RefreshFn) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let (rows, filter) = {
        let draft = draft.borrow();
        (draft.current.rows.clone(), draft.filter.to_lowercase())
    };
    for (index, row) in rows
        .iter()
        .enumerate()
        .filter(|(_, row)| filter.is_empty() || row.name.to_lowercase().contains(&filter))
    {
        list.append(&playlist_row(index, row, draft, refresh));
    }
}

fn playlist_row(
    index: usize,
    row: &PickerPlaylistRow,
    draft: &Rc<RefCell<PickerDraft>>,
    refresh: &RefreshFn,
) -> gtk4::Box {
    let name = if row.source == EVERYTHING_SOURCE {
        device_sync_strings::text(device_sync_strings::EVERYTHING)
    } else {
        row.name.clone()
    };
    let check = gtk4::CheckButton::with_label(&name);
    check.set_active(row.selected);
    let subtitle = if row.smart {
        format!(
            "{} · {} · {}",
            device_sync_strings::text(device_sync_strings::SMART_PLAYLIST),
            device_sync_strings::picker_content(row.track_count),
            device_sync_strings::file_size(row.size_bytes)
        )
    } else {
        format!(
            "{} · {}",
            device_sync_strings::picker_content(row.track_count),
            device_sync_strings::file_size(row.size_bytes)
        )
    };
    let subtitle = picker_label(&subtitle);
    let copy = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    copy.set_hexpand(true);
    copy.append(&check);
    copy.append(&subtitle);
    let draft = draft.clone();
    let refresh = refresh.clone();
    check.connect_toggled(move |check| {
        set_playlist_row_selected(&mut draft.borrow_mut().current, index, check.is_active());
        call_refresh(&refresh);
    });
    copy
}

fn set_playlist_row_selected(snapshot: &mut PickerSnapshot, index: usize, selected: bool) {
    if index >= snapshot.rows.len() {
        return;
    }
    if selected {
        if snapshot.rows[index].source == EVERYTHING_SOURCE {
            for row in &mut snapshot.rows {
                row.selected = false;
            }
        } else if let Some(everything) = snapshot
            .rows
            .iter_mut()
            .find(|row| row.source == EVERYTHING_SOURCE)
        {
            everything.selected = false;
        }
    }
    snapshot.rows[index].selected = selected;
}

fn select_all_rows(snapshot: &mut PickerSnapshot) {
    for row in &mut snapshot.rows {
        row.selected = row.source != EVERYTHING_SOURCE;
    }
}

fn update_footer(label: &gtk4::Label, snapshot: &PickerSnapshot) {
    let selected = snapshot.rows.iter().filter(|row| row.selected);
    let count = selected.clone().count().to_string();
    let tracks = selected.clone().map(|row| row.track_count).sum::<usize>();
    let size = selected.map(|row| row.size_bytes).sum::<u64>();
    label.set_text(&device_sync_strings::formatted(
        device_sync_strings::PICKER_FOOTER,
        &[
            ("selected", &count),
            ("content", &device_sync_strings::picker_content(tracks)),
            ("size", &device_sync_strings::file_size(size)),
        ],
    ));
}

fn picker_changes(draft: &PickerDraft) -> PickerSave {
    let playlist_changes = draft
        .original
        .rows
        .iter()
        .filter_map(|original| {
            draft
                .current
                .rows
                .iter()
                .find(|current| current.source == original.source)
                .filter(|current| current.selected != original.selected)
                .map(|current| (current.source.clone(), current.selected))
        })
        .collect();
    PickerSave {
        playlist_changes,
        keep_smart_updated: (draft.original.keep_smart_updated != draft.current.keep_smart_updated)
            .then_some(draft.current.keep_smart_updated),
    }
}

fn picker_label(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class("dim-label");
    label.set_xalign(0.0);
    label.set_wrap(true);
    label
}

fn call_refresh(refresh: &RefreshFn) {
    let callback = refresh.borrow().clone();
    if let Some(callback) = callback {
        callback();
    }
}

#[cfg(test)]
#[path = "device_sync_picker_unit_tests.rs"]
mod tests;
