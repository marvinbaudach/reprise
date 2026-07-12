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

const ACTION_DOWNLOAD_MISSING_COVERS: &str = "download-missing-covers";
const SMOKE_COVER_DOWNLOAD_ENV_VAR: &str = "REPRISE_SMOKE_COVER_DOWNLOAD";

pub(super) fn install(
    header: &adw::HeaderBar,
    window: &adw::ApplicationWindow,
    conn: &Rc<RefCell<Connection>>,
    runtime: &CoverDownloadRuntime,
) {
    let menu = gio::Menu::new();
    menu.append(
        Some(strings::DOWNLOAD_MISSING_COVERS),
        Some("win.download-missing-covers"),
    );
    let menu_button = gtk4::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .tooltip_text(strings::MAIN_MENU)
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
