//! Header primary menu and its window-scoped actions. Kept out of
//! `window.rs` because that composition root is already close to the
//! project's 800-line limit.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use libadwaita as adw;
use rusqlite::Connection;

use crate::ui::cover_download_worker::CoverDownloadRuntime;
use crate::ui::strings;
use crate::ui::track_list::TrackList;

pub(super) const ACTION_DOWNLOAD_MISSING_COVERS: &str = "download-missing-covers";
pub(super) const ACTION_IMPORT_RHYTHMBOX_COLUMNS: &str = "import-rhythmbox-columns";
const SMOKE_COVER_DOWNLOAD_ENV_VAR: &str = "REPRISE_SMOKE_COVER_DOWNLOAD";
const SMOKE_RHYTHMBOX_COLUMNS_ENV_VAR: &str = "REPRISE_SMOKE_RHYTHMBOX_COLUMNS";

pub(super) fn install(
    header: &adw::HeaderBar,
    window: &adw::ApplicationWindow,
    conn: &Rc<RefCell<Connection>>,
    runtime: &CoverDownloadRuntime,
    track_list: &Rc<TrackList>,
) {
    let menu = gio::Menu::new();
    menu.append(
        Some(&strings::text(strings::DOWNLOAD_MISSING_COVERS)),
        Some("win.download-missing-covers"),
    );
    menu.append(
        Some(&strings::text(strings::IMPORT_RHYTHMBOX_COLUMNS)),
        Some("win.import-rhythmbox-columns"),
    );
    let menu_button = gtk4::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .tooltip_text(strings::text(strings::MAIN_MENU))
        .build();
    header.pack_end(&menu_button);

    let initial_enabled = runtime.enabled.get();
    let toggle = gio::SimpleAction::new_stateful(
        ACTION_DOWNLOAD_MISSING_COVERS,
        None,
        &initial_enabled.to_variant(),
    );
    {
        let conn = conn.clone();
        let flag = runtime.enabled.clone();
        toggle.connect_change_state(move |action, state| {
            let Some(enabled) = state.and_then(glib::Variant::get::<bool>) else {
                return;
            };
            let persisted = {
                let conn = conn.borrow();
                reprise_core::modules::set_enabled(
                    &conn,
                    &reprise_core::modules::COVER_DOWNLOAD_MODULE,
                    enabled,
                )
            };
            if let Err(error) = persisted {
                tracing::warn!(%error, "could not persist cover_download toggle");
            }
            flag.set(enabled);
            action.set_state(&enabled.to_variant());
        });
    }
    window.add_action(&toggle);
    arm_smoke_toggle(&toggle);

    let import = gio::SimpleAction::new(ACTION_IMPORT_RHYTHMBOX_COLUMNS, None);
    {
        let track_list_weak = Rc::downgrade(track_list);
        import.connect_activate(move |_, _| {
            let Some(track_list) = track_list_weak.upgrade() else {
                return;
            };
            handle_rhythmbox_import(&track_list, rhythmbox_smoke_tokens());
        });
    }
    window.add_action(&import);
    arm_smoke_rhythmbox_import(&import);
}

fn arm_smoke_toggle(toggle: &gio::SimpleAction) {
    let Ok(value) = std::env::var(SMOKE_COVER_DOWNLOAD_ENV_VAR) else {
        return;
    };
    if value != "on" {
        tracing::warn!(%value, "{SMOKE_COVER_DOWNLOAD_ENV_VAR} ignored; expected 'on'");
        return;
    }
    let toggle = toggle.clone();
    glib::idle_add_local_once(move || {
        toggle.change_state(&true.to_variant());
        tracing::info!("smoke: cover_download toggled on");
    });
}

fn handle_rhythmbox_import(track_list: &TrackList, override_tokens: Option<Vec<String>>) {
    let tokens = match override_tokens {
        Some(tokens) => tokens,
        None => match crate::ui::column_layout::read_rhythmbox_visible_columns() {
            Ok(tokens) => tokens,
            Err(error) => {
                tracing::warn!(%error, "could not read Rhythmbox visible columns");
                track_list.toast(&strings::rhythmbox_columns_import_failed(
                    &error.to_string(),
                ));
                return;
            }
        },
    };
    let layout = crate::ui::column_layout::import_rhythmbox_tokens(&tokens);
    if let Err(error) = track_list.apply_column_layout(&layout) {
        tracing::warn!(%error, "could not persist imported Rhythmbox column layout");
        track_list.toast(&strings::text(
            strings::RHYTHMBOX_COLUMNS_IMPORT_SAVE_FAILED,
        ));
        return;
    }
    tracing::info!(
        layout = %crate::ui::column_layout::serialize_layout(&layout),
        "Rhythmbox column layout imported"
    );
    track_list.toast(&strings::text(strings::RHYTHMBOX_COLUMNS_IMPORTED));
}

fn rhythmbox_smoke_tokens() -> Option<Vec<String>> {
    std::env::var(SMOKE_RHYTHMBOX_COLUMNS_ENV_VAR)
        .ok()
        .map(|value| value.split(',').map(str::to_string).collect())
}

fn arm_smoke_rhythmbox_import(action: &gio::SimpleAction) {
    if std::env::var(SMOKE_RHYTHMBOX_COLUMNS_ENV_VAR).is_err()
        || std::env::var(crate::ui::first_run::SMOKE_ENV).is_ok()
    {
        return;
    }
    let action = action.clone();
    glib::idle_add_local_once(move || {
        action.activate(None);
    });
}
