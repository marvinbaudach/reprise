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
pub(super) const ACTION_RESCAN_LIBRARY: &str = "rescan-library";
pub(super) const ACTION_SYNC_DEVICE: &str = "sync-device";
pub(super) const ACTION_STOP_PLAYBACK: &str = "stop-playback";
pub(super) const ACTION_PREFERENCES: &str = "preferences";
pub(super) const ACTION_KEYBOARD_SHORTCUTS: &str = "keyboard-shortcuts";
pub(super) const ACTION_HELP: &str = "help";
pub(super) const ACTION_ABOUT: &str = "about";
pub(super) const ACTION_OPEN_PRIMARY_MENU: &str = "open-primary-menu";
pub(super) const SMOKE_RHYTHMBOX_COLUMNS_ENV_VAR: &str = "REPRISE_SMOKE_RHYTHMBOX_COLUMNS";
const SMOKE_MINIMAL_VIEW_ENV_VAR: &str = "REPRISE_SMOKE_MINIMAL_VIEW";

pub(super) struct Callbacks {
    pub(super) on_minimal_view: Rc<dyn Fn()>,
    pub(super) on_rescan_library: Rc<dyn Fn()>,
    pub(super) on_cancel_scan: Rc<dyn Fn()>,
    pub(super) on_sync_device: Rc<dyn Fn()>,
    pub(super) on_stop_playback: Option<Rc<dyn Fn()>>,
    pub(super) on_preferences: Rc<dyn Fn()>,
}

/// View section: mode switches and personal views.
fn view_section_entries() -> Vec<(String, &'static str)> {
    vec![(
        strings::text(strings::COMPACT_MODE),
        "win.toggle-minimal-view",
    )]
}

fn playback_section_entries() -> [(String, &'static str); 1] {
    [(strings::text(strings::STOP_PLAYBACK), "win.stop-playback")]
}

/// Rebuilds the library section of the primary menu with the correct label
/// for the current scan state: "Rescan Library" when idle, "Cancel Scan"
/// when a scan is running. GTK reads the `gio::Menu` model on each popover
/// open, so rebuilding here is sufficient.
pub(super) fn update_library_section(menu: &gio::Menu, is_scanning: bool) {
    menu.remove_all();
    let label = if is_scanning {
        strings::text(strings::CANCEL_SCAN)
    } else {
        strings::text(strings::RESCAN_LIBRARY)
    };
    menu.append(Some(&label), Some("win.rescan-library"));
    menu.append(
        Some(&strings::text(strings::SYNC_DEVICE)),
        Some("win.sync-device"),
    );
}

/// Settings section: preferences, shortcuts, help, and about.
fn settings_section_entries() -> Vec<(String, &'static str)> {
    vec![
        (strings::text(strings::PREFERENCES), "win.preferences"),
        (
            strings::text(strings::KEYBOARD_SHORTCUTS),
            "win.keyboard-shortcuts",
        ),
        (strings::text(strings::HELP), "win.help"),
        (strings::text(strings::ABOUT_REPRISE), "win.about"),
    ]
}

#[allow(clippy::needless_pass_by_value)]
pub(super) fn install(
    header: &adw::HeaderBar,
    window: &adw::ApplicationWindow,
    track_list: &Rc<TrackList>,
    callbacks: Callbacks,
    scan_controls: &super::scan_flow::ScanControls,
) -> gio::Menu {
    let view = gio::Menu::new();
    for (label, action) in view_section_entries() {
        view.append(Some(&label), Some(action));
    }
    let playback = gio::Menu::new();
    for (label, action) in playback_section_entries() {
        playback.append(Some(&label), Some(action));
    }
    let library = gio::Menu::new();
    update_library_section(&library, scan_controls.is_scanning());
    let settings = gio::Menu::new();
    for (label, action) in settings_section_entries() {
        settings.append(Some(&label), Some(action));
    }
    let menu = gio::Menu::new();
    menu.append_section(None, &view);
    menu.append_section(None, &playback);
    menu.append_section(None, &library);
    menu.append_section(None, &settings);

    let menu_button = gtk4::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .tooltip_text(strings::text(strings::MAIN_MENU))
        .build();
    header.pack_end(&menu_button);

    let open_primary_menu = gio::SimpleAction::new(ACTION_OPEN_PRIMARY_MENU, None);
    {
        let menu_button = menu_button.downgrade();
        open_primary_menu.connect_activate(move |_, _| {
            if let Some(menu_button) = menu_button.upgrade() {
                menu_button.popup();
            }
        });
    }
    window.add_action(&open_primary_menu);

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
    {
        let cb = callbacks.on_minimal_view.clone();
        minimal.connect_activate(move |_, _| cb());
    }
    window.add_action(&minimal);
    arm_smoke_minimal_view(&minimal);

    let rescan = gio::SimpleAction::new(ACTION_RESCAN_LIBRARY, None);
    {
        let rescan_cb = callbacks.on_rescan_library.clone();
        let cancel_cb = callbacks.on_cancel_scan.clone();
        let scan_controls = scan_controls.clone();
        rescan.connect_activate(move |_, _| {
            if scan_controls.is_scanning() {
                cancel_cb();
            } else {
                rescan_cb();
            }
        });
    }
    window.add_action(&rescan);

    let sync_device = gio::SimpleAction::new(ACTION_SYNC_DEVICE, None);
    {
        let cb = callbacks.on_sync_device.clone();
        sync_device.connect_activate(move |_, _| cb());
    }
    window.add_action(&sync_device);

    let stop_playback = gio::SimpleAction::new(ACTION_STOP_PLAYBACK, None);
    stop_playback.set_enabled(callbacks.on_stop_playback.is_some());
    if let Some(cb) = callbacks.on_stop_playback {
        stop_playback.connect_activate(move |_, _| cb());
    }
    window.add_action(&stop_playback);

    let preferences = gio::SimpleAction::new(ACTION_PREFERENCES, None);
    {
        let cb = callbacks.on_preferences.clone();
        preferences.connect_activate(move |_, _| cb());
    }
    window.add_action(&preferences);
    if std::env::var(crate::ui::preferences::SMOKE_ENV).is_ok() {
        let preferences = preferences.clone();
        glib::idle_add_local_once(move || preferences.activate(None));
    }

    let keyboard_shortcuts = gio::SimpleAction::new(ACTION_KEYBOARD_SHORTCUTS, None);
    {
        let window = window.downgrade();
        keyboard_shortcuts.connect_activate(move |_, _| {
            if let Some(window) = window.upgrade() {
                crate::ui::help::present(&window);
            }
        });
    }
    window.add_action(&keyboard_shortcuts);

    let help = gio::SimpleAction::new(ACTION_HELP, None);
    {
        let window = window.downgrade();
        help.connect_activate(move |_, _| {
            if let Some(window) = window.upgrade() {
                crate::ui::help::present(&window);
            }
        });
    }
    window.add_action(&help);

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

    library
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
    fn view_section_starts_with_compact_mode() {
        let entries = view_section_entries();
        assert_eq!(entries[0].1, "win.toggle-minimal-view");
    }

    #[test]
    fn playback_section_exposes_stop_without_a_pointer() {
        assert_eq!(
            playback_section_entries(),
            [(strings::text(strings::STOP_PLAYBACK), "win.stop-playback")]
        );
    }

    #[test]
    fn library_section_has_rescan_and_sync_when_idle() {
        let menu = gio::Menu::new();
        update_library_section(&menu, false);
        let actions: Vec<_> = (0..menu.n_items())
            .filter_map(|i| {
                menu.item_attribute_value(i, "action", Some(glib::VariantTy::STRING))
                    .and_then(|v| v.get::<String>())
            })
            .collect();
        assert_eq!(actions, ["win.rescan-library", "win.sync-device"]);
    }

    #[test]
    fn library_section_shows_cancel_when_scanning() {
        let menu = gio::Menu::new();
        update_library_section(&menu, true);
        let label = menu
            .item_attribute_value(0, "label", Some(glib::VariantTy::STRING))
            .and_then(|v| v.get::<String>());
        assert_eq!(
            label.as_deref(),
            Some(strings::text(strings::CANCEL_SCAN).as_str())
        );
    }

    #[test]
    fn settings_section_has_help_before_about() {
        let actions: Vec<_> = settings_section_entries()
            .into_iter()
            .map(|(_, action)| action)
            .collect();
        let help = actions.iter().position(|a| *a == "win.help");
        let about = actions.iter().position(|a| *a == "win.about");
        assert_eq!(help.map(|i| i + 1), about);
    }

    #[test]
    fn primary_menu_exposes_a_window_action_for_f10() {
        assert_eq!(ACTION_OPEN_PRIMARY_MENU, "open-primary-menu");
    }

    #[test]
    fn no_rhythmbox_import_in_visible_menu() {
        let library = gio::Menu::new();
        update_library_section(&library, false);
        let library_actions: Vec<String> = (0..library.n_items())
            .filter_map(|i| {
                library
                    .item_attribute_value(i, "action", Some(glib::VariantTy::STRING))
                    .and_then(|v| v.get::<String>())
            })
            .collect();
        let mut all_actions: Vec<&str> = view_section_entries()
            .into_iter()
            .chain(settings_section_entries())
            .map(|(_, action)| action)
            .collect();
        all_actions.extend(library_actions.iter().map(std::string::String::as_str));
        assert!(!all_actions.contains(&"win.import-rhythmbox-columns"));
    }
}
