//! Header primary menu and its window-scoped actions. Kept out of
//! `window.rs` because that composition root is already close to the
//! project's 800-line limit.

use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use libadwaita as adw;

use crate::ui::strings;
use crate::ui::track_list::TrackList;

pub(super) const ACTION_IMPORT_RHYTHMBOX_COLUMNS: &str = "import-rhythmbox-columns";
pub(super) const ACTION_EDIT_COLUMN_LAYOUT: &str = "edit-column-layout";
pub(super) const ACTION_TOGGLE_MINIMAL_VIEW: &str = "toggle-minimal-view";
pub(super) const ACTION_PREFERENCES: &str = "preferences";
pub(super) const ACTION_ABOUT: &str = "about";
pub(super) const SMOKE_RHYTHMBOX_COLUMNS_ENV_VAR: &str = "REPRISE_SMOKE_RHYTHMBOX_COLUMNS";
const SMOKE_MINIMAL_VIEW_ENV_VAR: &str = "REPRISE_SMOKE_MINIMAL_VIEW";

pub(super) struct Callbacks {
    pub(super) on_minimal_view: Rc<dyn Fn()>,
    pub(super) on_preferences: Rc<dyn Fn()>,
}

fn primary_menu_entries() -> Vec<(String, &'static str)> {
    vec![
        (strings::text(strings::PREFERENCES), "win.preferences"),
        (
            strings::text(strings::COMPACT_VIEW),
            "win.toggle-minimal-view",
        ),
        (
            strings::text(strings::EDIT_COLUMN_LAYOUT),
            "win.edit-column-layout",
        ),
        (strings::text(strings::ABOUT), "win.about"),
    ]
}

pub(super) fn install(
    header: &adw::HeaderBar,
    window: &adw::ApplicationWindow,
    track_list: &Rc<TrackList>,
    callbacks: Callbacks,
) {
    let menu = gio::Menu::new();
    for (label, action) in primary_menu_entries() {
        menu.append(Some(&label), Some(action));
    }
    let menu_button = gtk4::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .tooltip_text(strings::text(strings::MAIN_MENU))
        .build();
    header.pack_end(&menu_button);

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

    let edit = gio::SimpleAction::new(ACTION_EDIT_COLUMN_LAYOUT, None);
    {
        let window = window.downgrade();
        let track_list = Rc::downgrade(track_list);
        edit.connect_activate(move |_, _| {
            let (Some(window), Some(track_list)) = (window.upgrade(), track_list.upgrade()) else {
                return;
            };
            crate::ui::column_layout_editor::present(&window, &track_list);
        });
    }
    window.add_action(&edit);
    arm_smoke_column_layout_editor(&edit);

    let minimal = gio::SimpleAction::new(ACTION_TOGGLE_MINIMAL_VIEW, None);
    minimal.connect_activate(move |_, _| (callbacks.on_minimal_view)());
    window.add_action(&minimal);
    arm_smoke_minimal_view(&minimal);

    let preferences = gio::SimpleAction::new(ACTION_PREFERENCES, None);
    preferences.connect_activate(move |_, _| (callbacks.on_preferences)());
    window.add_action(&preferences);
    if std::env::var(crate::ui::preferences::SMOKE_ENV).is_ok() {
        let preferences = preferences.clone();
        glib::idle_add_local_once(move || preferences.activate(None));
    }

    let about = gio::SimpleAction::new(ACTION_ABOUT, None);
    {
        let window = window.downgrade();
        about.connect_activate(move |_, _| {
            if let Some(window) = window.upgrade() {
                crate::ui::about::present(&window);
            }
        });
    }
    window.add_action(&about);
}

fn arm_smoke_minimal_view(action: &gio::SimpleAction) {
    let Ok(mode) = std::env::var(SMOKE_MINIMAL_VIEW_ENV_VAR) else {
        return;
    };
    let enter = action.clone();
    glib::idle_add_local_once(move || enter.activate(None));
    if mode == "stay" {
        return;
    }
    let restore = action.clone();
    glib::timeout_add_seconds_local_once(1, move || restore.activate(None));
}

fn arm_smoke_column_layout_editor(action: &gio::SimpleAction) {
    if std::env::var(crate::ui::column_layout_editor::SMOKE_ENV).is_err() {
        return;
    }
    let action = action.clone();
    glib::idle_add_local_once(move || action.activate(None));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_menu_does_not_offer_rhythmbox_import() {
        let actions = primary_menu_entries()
            .into_iter()
            .map(|(_, action)| action)
            .collect::<Vec<_>>();

        assert!(!actions.contains(&"win.import-rhythmbox-columns"));
        assert!(actions.contains(&"win.edit-column-layout"));
        assert!(actions.contains(&"win.toggle-minimal-view"));
        assert!(actions.contains(&"win.about"));
    }
}
