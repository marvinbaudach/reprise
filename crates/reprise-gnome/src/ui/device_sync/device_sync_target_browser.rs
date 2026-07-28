//! Design 7d's device folder browser (`MTP-31`): storage selection, the
//! folder tree, "New folder", the target preview, the playlist-target
//! conflict warning, and "Reset to default". Every decision here is a call
//! into `reprise_core::device_sync::browser`'s pure projections — this file
//! only gathers facts (storages, folder listings) through
//! [`DeviceSyncRuntime`]'s async wrappers and renders what they decide.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::device_sync::browser::{
    folder_conflicts_with_playlist_target, preview_target_folder, reset_target_folder,
    StorageOption, TargetPreview,
};
use reprise_core::device_sync::{StorageId, SyncTarget, SyncTargetKind};

use super::device_sync_runtime::DeviceSyncRuntime;
use super::device_sync_strings;

/// The recursive navigation callback shared by row activation, the "Up"
/// button, and the storage dropdown — see [`present`]'s comment on why it
/// is filled in through a `RefCell` rather than passed directly.
type NavigateFn = Rc<RefCell<Option<Rc<dyn Fn(String)>>>>;

struct BrowserState {
    original: SyncTarget,
    playlist_target: Option<SyncTarget>,
    storages: Vec<StorageOption>,
    storage: Option<StorageId>,
    path: String,
}

/// Opens the folder browser for `kind` on `device_id`, relative to
/// `parent`. A no-op if the device disconnected between the click and this
/// call — there is nothing left to browse.
pub(in crate::ui) fn present(
    parent: &impl IsA<gtk4::Widget>,
    runtime: &Rc<DeviceSyncRuntime>,
    device_id: &str,
    kind: SyncTargetKind,
) {
    let Some(original) = runtime.current_target(device_id, kind) else {
        return;
    };
    let playlist_target = (kind != SyncTargetKind::Playlists)
        .then(|| runtime.current_target(device_id, SyncTargetKind::Playlists))
        .flatten();

    let state = Rc::new(RefCell::new(BrowserState {
        original: original.clone(),
        playlist_target,
        storages: Vec::new(),
        storage: original.storage_id,
        path: original.path.clone(),
    }));

    let storage_model = gtk4::StringList::new(&[]);
    let storage_dropdown = gtk4::DropDown::new(Some(storage_model.clone()), gtk4::Expression::NONE);
    storage_dropdown.set_sensitive(false);
    storage_dropdown.update_property(&[gtk4::accessible::Property::Label("Storage")]);

    let breadcrumb = detail_label("Loading storages…");
    let up_button = gtk4::Button::from_icon_name("go-up-symbolic");
    up_button.set_tooltip_text(Some("Up one folder"));
    up_button.set_sensitive(false);
    let breadcrumb_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    breadcrumb_row.append(&up_button);
    breadcrumb.set_hexpand(true);
    breadcrumb_row.append(&breadcrumb);

    let folder_list = gtk4::ListBox::new();
    folder_list.add_css_class("boxed-list");
    folder_list.set_selection_mode(gtk4::SelectionMode::None);
    let folder_scroller = gtk4::ScrolledWindow::builder()
        .child(&folder_list)
        .min_content_height(220)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();

    let new_folder_entry = gtk4::Entry::new();
    new_folder_entry.set_placeholder_text(Some("New folder name"));
    new_folder_entry.set_hexpand(true);
    new_folder_entry.set_sensitive(false);
    let new_folder_button = gtk4::Button::with_label("Create");
    new_folder_button.set_sensitive(false);
    let new_folder_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    new_folder_row.append(&new_folder_entry);
    new_folder_row.append(&new_folder_button);

    let preview_label = detail_label("");
    let warning_label = gtk4::Label::new(None);
    warning_label.set_xalign(0.0);
    warning_label.set_wrap(true);
    let warning_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    warning_box.add_css_class("warning");
    warning_box.set_visible(false);
    warning_box.append(&warning_label);

    let error_label = gtk4::Label::new(None);
    error_label.set_xalign(0.0);
    error_label.set_wrap(true);
    let error_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    error_box.add_css_class("error");
    error_box.set_visible(false);
    error_box.append(&error_label);

    let reset_button = gtk4::Button::with_label("Reset to default");
    let cancel_button = gtk4::Button::with_label("Cancel");
    let save_button = gtk4::Button::with_label("Save");
    save_button.add_css_class("suggested-action");

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_top(16);
    content.set_margin_bottom(16);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.append(&storage_dropdown);
    content.append(&breadcrumb_row);
    content.append(&folder_scroller);
    content.append(&new_folder_row);
    content.append(&error_box);
    content.append(&warning_box);
    content.append(&preview_label);
    let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    footer.append(&reset_button);
    let footer_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    footer_spacer.set_hexpand(true);
    footer.append(&footer_spacer);
    footer.append(&cancel_button);
    footer.append(&save_button);
    content.append(&footer);

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(
        &format!(
            "Choose folder for {}",
            device_sync_strings::category_name(kind)
        ),
        "",
    )));
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));
    let dialog = adw::Dialog::builder()
        .child(&toolbar)
        .content_width(480)
        .content_height(560)
        .build();

    // `navigate` is filled in once below and re-borrowed by row activation
    // and the "Up" button, both of which need to call back into the same
    // async loader recursively.
    let navigate: NavigateFn = Rc::new(RefCell::new(None));
    let updating = Rc::new(Cell::new(false));

    let navigate_impl: Rc<dyn Fn(String)> = {
        let state = state.clone();
        let runtime = runtime.clone();
        let device_id = device_id.to_string();
        let folder_list = folder_list.clone();
        let breadcrumb = breadcrumb.clone();
        let up_button = up_button.clone();
        let preview_label = preview_label.clone();
        let warning_box = warning_box.clone();
        let warning_label = warning_label.clone();
        let error_box = error_box.clone();
        let error_label = error_label.clone();
        let new_folder_entry = new_folder_entry.clone();
        let new_folder_button = new_folder_button.clone();
        Rc::new(move |path: String| {
            let Some(storage) = state.borrow().storage else {
                return;
            };
            state.borrow_mut().path = path.clone();
            error_box.set_visible(false);
            breadcrumb.set_label(&path);
            up_button.set_sensitive(path != "/");
            new_folder_entry.set_sensitive(true);
            new_folder_button.set_sensitive(true);
            refresh_preview_and_warning(
                &state.borrow(),
                &preview_label,
                &warning_box,
                &warning_label,
            );
            while let Some(child) = folder_list.first_child() {
                folder_list.remove(&child);
            }

            let runtime = runtime.clone();
            let device_id = device_id.clone();
            let folder_list = folder_list.clone();
            let error_box = error_box.clone();
            let error_label = error_label.clone();
            gtk4::glib::MainContext::ref_thread_default().spawn_local(async move {
                match runtime.browse_folders(&device_id, storage, path).await {
                    Ok(folders) => {
                        for name in folders {
                            folder_list.append(&folder_row(&name));
                        }
                    }
                    Err(error) => {
                        error_label.set_label(&error);
                        error_box.set_visible(true);
                    }
                }
            });
        })
    };
    *navigate.borrow_mut() = Some(navigate_impl);

    folder_list.connect_row_activated({
        let navigate = navigate.clone();
        let state = state.clone();
        move |_, row| {
            let name = row.widget_name();
            let current = state.borrow().path.clone();
            let child_path = push_path(&current, &name);
            if let Some(navigate_fn) = navigate.borrow().as_ref() {
                navigate_fn(child_path);
            }
        }
    });

    up_button.connect_clicked({
        let navigate = navigate.clone();
        let state = state.clone();
        move |_| {
            let current = state.borrow().path.clone();
            if let Some(parent_path) = parent_path(&current) {
                if let Some(navigate_fn) = navigate.borrow().as_ref() {
                    navigate_fn(parent_path);
                }
            }
        }
    });

    storage_dropdown.connect_selected_notify({
        let navigate = navigate.clone();
        let state = state.clone();
        let updating = updating.clone();
        move |dropdown| {
            if updating.get() {
                return;
            }
            let index = dropdown.selected();
            if index == gtk4::INVALID_LIST_POSITION {
                return;
            }
            let Some(chosen) = state
                .borrow()
                .storages
                .get(index as usize)
                .map(|option| option.id)
            else {
                return;
            };
            state.borrow_mut().storage = Some(chosen);
            if let Some(navigate_fn) = navigate.borrow().as_ref() {
                navigate_fn("/".to_string());
            }
        }
    });

    let create_folder = {
        let runtime = runtime.clone();
        let device_id = device_id.to_string();
        let state = state.clone();
        let navigate = navigate.clone();
        let entry = new_folder_entry.clone();
        let error_box = error_box.clone();
        let error_label = error_label.clone();
        move || {
            let name = entry.text().trim().to_string();
            if name.is_empty() {
                return;
            }
            let (storage, path) = {
                let state = state.borrow();
                let Some(storage) = state.storage else { return };
                (storage, state.path.clone())
            };
            let runtime = runtime.clone();
            let device_id = device_id.clone();
            let navigate = navigate.clone();
            let entry = entry.clone();
            let error_box = error_box.clone();
            let error_label = error_label.clone();
            let path_for_reload = path.clone();
            gtk4::glib::MainContext::ref_thread_default().spawn_local(async move {
                match runtime
                    .create_target_folder(&device_id, storage, path, name)
                    .await
                {
                    Ok(()) => {
                        entry.set_text("");
                        error_box.set_visible(false);
                        if let Some(navigate_fn) = navigate.borrow().as_ref() {
                            navigate_fn(path_for_reload);
                        }
                    }
                    Err(error) => {
                        error_label.set_label(&error);
                        error_box.set_visible(true);
                    }
                }
            });
        }
    };
    new_folder_button.connect_clicked({
        let create_folder = create_folder.clone();
        move |_| create_folder()
    });
    new_folder_entry.connect_activate(move |_| create_folder());

    reset_button.connect_clicked({
        let state = state.clone();
        let updating = updating.clone();
        let storage_dropdown = storage_dropdown.clone();
        let breadcrumb = breadcrumb.clone();
        let up_button = up_button.clone();
        let new_folder_entry = new_folder_entry.clone();
        let new_folder_button = new_folder_button.clone();
        let preview_label = preview_label.clone();
        let warning_box = warning_box.clone();
        let warning_label = warning_label.clone();
        let error_box = error_box.clone();
        let folder_list = folder_list.clone();
        move |_| {
            let reset = reset_target_folder(&state.borrow().original);
            state.borrow_mut().storage = reset.storage_id;
            state.borrow_mut().path = reset.path.clone();
            updating.set(true);
            storage_dropdown.set_selected(gtk4::INVALID_LIST_POSITION);
            updating.set(false);
            breadcrumb.set_label("Pick a storage to browse");
            up_button.set_sensitive(false);
            new_folder_entry.set_sensitive(false);
            new_folder_button.set_sensitive(false);
            error_box.set_visible(false);
            while let Some(child) = folder_list.first_child() {
                folder_list.remove(&child);
            }
            refresh_preview_and_warning(
                &state.borrow(),
                &preview_label,
                &warning_box,
                &warning_label,
            );
        }
    });

    cancel_button.connect_clicked({
        let dialog = dialog.downgrade();
        move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        }
    });
    save_button.connect_clicked({
        let runtime = runtime.clone();
        let device_id = device_id.to_string();
        let state = state.clone();
        let dialog = dialog.downgrade();
        move |_| {
            let (storage, path) = {
                let state = state.borrow();
                (state.storage, state.path.clone())
            };
            if let Err(error) = runtime.set_target_folder(&device_id, kind, storage, path) {
                tracing::warn!(%error, "could not save Android sync target folder");
            }
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        }
    });

    refresh_preview_and_warning(
        &state.borrow(),
        &preview_label,
        &warning_box,
        &warning_label,
    );
    dialog.present(Some(parent));

    let load_storages = {
        let runtime = runtime.clone();
        let device_id = device_id.to_string();
        let state = state.clone();
        let navigate = navigate.clone();
        let storage_model = storage_model.clone();
        let storage_dropdown = storage_dropdown.clone();
        let updating = updating.clone();
        let breadcrumb = breadcrumb.clone();
        let error_box = error_box.clone();
        let error_label = error_label.clone();
        let starting_storage = original.storage_id;
        let starting_path = original.path.clone();
        async move {
            match runtime.browse_storages(&device_id).await {
                Ok(storages) => {
                    for option in &storages {
                        storage_model.append(&option.name);
                    }
                    let resolved_index = starting_storage
                        .and_then(|id| storages.iter().position(|option| option.id == id));
                    state.borrow_mut().storages = storages;
                    storage_dropdown.set_sensitive(true);
                    if let Some(index) = resolved_index {
                        #[allow(clippy::cast_possible_truncation)]
                        {
                            updating.set(true);
                            storage_dropdown.set_selected(index as u32);
                            updating.set(false);
                        }
                        if let Some(navigate_fn) = navigate.borrow().as_ref() {
                            navigate_fn(starting_path);
                        }
                    } else {
                        breadcrumb.set_label("Pick a storage to browse");
                    }
                }
                Err(error) => {
                    error_label.set_label(&error);
                    error_box.set_visible(true);
                    breadcrumb.set_label("Storage list unavailable");
                }
            }
        }
    };
    gtk4::glib::MainContext::ref_thread_default().spawn_local(load_storages);
}

fn detail_label(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.add_css_class("dim-label");
    label
}

fn folder_row(name: &str) -> gtk4::ListBoxRow {
    let icon = gtk4::Image::from_icon_name("folder-symbolic");
    let label = gtk4::Label::new(Some(name));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    let inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    inner.set_margin_top(6);
    inner.set_margin_bottom(6);
    inner.set_margin_start(8);
    inner.set_margin_end(8);
    inner.append(&icon);
    inner.append(&label);
    let row = gtk4::ListBoxRow::new();
    row.set_child(Some(&inner));
    row.set_widget_name(name);
    row
}

/// Pure display logic behind the preview label and the warning banner —
/// kept separate from the widgets so it is unit-tested without a display.
fn refresh_preview_and_warning(
    state: &BrowserState,
    preview_label: &gtk4::Label,
    warning_box: &gtk4::Box,
    warning_label: &gtk4::Label,
) {
    let candidate = SyncTarget {
        storage_id: state.storage,
        path: state.path.clone(),
        ..state.original.clone()
    };
    preview_label.set_label(&preview_text(&preview_target_folder(
        &candidate,
        &state.storages,
    )));
    let conflicts = state.playlist_target.as_ref().is_some_and(|playlist| {
        folder_conflicts_with_playlist_target(state.storage, &state.path, playlist)
    });
    warning_box.set_visible(conflicts);
    if conflicts {
        warning_label.set_label(
            "This folder is inside the Playlists folder, which removes anything Reprise \
             does not recognize on its own. Files copied here for this category could be \
             deleted by a playlist sync.",
        );
    }
}

/// `MTP-31`: the target preview's exact copy for each resolution state.
fn preview_text(preview: &TargetPreview) -> String {
    match preview {
        TargetPreview::Unresolved { path } => {
            format!("Files will land at {path} once a storage is chosen.")
        }
        TargetPreview::StorageMissing { path } => format!(
            "The previously chosen storage is no longer available on this device; files would have landed at {path}."
        ),
        TargetPreview::Resolved { storage_name, path } => {
            format!("Files will be stored at {storage_name} → {path}")
        }
    }
}

fn push_path(path: &str, name: &str) -> String {
    if path == "/" {
        format!("/{name}")
    } else {
        format!("{path}/{name}")
    }
}

fn parent_path(path: &str) -> Option<String> {
    if path == "/" {
        return None;
    }
    match path.rsplit_once('/') {
        Some(("", _)) => Some("/".to_string()),
        Some((parent, _)) => Some(parent.to_string()),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::device_sync::browser::StorageKind;

    #[test]
    fn mtp_31_path_navigation_pushes_and_pops_components() {
        assert_eq!(push_path("/", "Music"), "/Music");
        assert_eq!(push_path("/Music", "Reprise"), "/Music/Reprise");
        assert_eq!(parent_path("/Music/Reprise"), Some("/Music".to_string()));
        assert_eq!(parent_path("/Music"), Some("/".to_string()));
        assert_eq!(parent_path("/"), None);
    }

    #[test]
    fn mtp_31_preview_text_names_the_resolved_storage_and_path() {
        assert_eq!(
            preview_text(&TargetPreview::Resolved {
                storage_name: "Internal shared storage".to_string(),
                path: "/Music/Reprise-YouTube".to_string(),
            }),
            "Files will be stored at Internal shared storage → /Music/Reprise-YouTube"
        );
        assert!(preview_text(&TargetPreview::Unresolved {
            path: "/Music/Reprise-YouTube".to_string()
        })
        .contains("once a storage is chosen"));
        assert!(preview_text(&TargetPreview::StorageMissing {
            path: "/Music/Reprise-YouTube".to_string()
        })
        .contains("no longer available"));
    }

    #[test]
    fn mtp_31_conflict_warning_only_fires_against_an_actual_playlist_target() {
        let playlists = SyncTarget {
            kind: SyncTargetKind::Playlists,
            storage_id: Some(StorageId(1)),
            path: "/Music/Reprise".to_string(),
            enabled: true,
            cap_bytes: None,
        };
        let state = BrowserState {
            original: SyncTarget {
                kind: SyncTargetKind::YoutubeAudio,
                storage_id: Some(StorageId(1)),
                path: "/Music/Reprise-YouTube".to_string(),
                enabled: true,
                cap_bytes: None,
            },
            playlist_target: Some(playlists.clone()),
            storages: vec![StorageOption {
                id: StorageId(1),
                name: "Internal".to_string(),
                kind: StorageKind::Internal,
            }],
            storage: Some(StorageId(1)),
            path: "/Music/Reprise/Nested".to_string(),
        };
        let conflicts = state.playlist_target.as_ref().is_some_and(|playlist| {
            folder_conflicts_with_playlist_target(state.storage, &state.path, playlist)
        });
        assert!(conflicts);
    }
}
