//! Header primary menu and its window-scoped actions. Kept out of
//! `window.rs` because that composition root is already close to the
//! project's 800-line limit.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use libadwaita as adw;

use crate::ui::strings;

#[derive(Default)]
pub(in crate::ui) struct ActiveTable(
    RefCell<Option<Rc<dyn crate::ui::table_columns::EditorModel>>>,
);

impl ActiveTable {
    pub(in crate::ui) fn set(&self, model: Option<Rc<dyn crate::ui::table_columns::EditorModel>>) {
        self.0.replace(model);
    }

    pub(in crate::ui) fn get(&self) -> Option<Rc<dyn crate::ui::table_columns::EditorModel>> {
        self.0.borrow().clone()
    }
}

pub(super) const ACTION_EDIT_COLUMN_LAYOUT: &str = "edit-column-layout";
pub(super) const ACTION_TOGGLE_MINIMAL_VIEW: &str = "toggle-minimal-view";
pub(super) const ACTION_LIBRARY_DOCTOR: &str = "library-doctor";
/// The sidebar's `ISSUES` entry, which exists only because a completed scan
/// has findings nobody has looked at yet. The menu holds the verb ("run a
/// scan") and lands on the Doctor's own page; this action holds the noun and
/// goes straight to the findings.
pub(super) const ACTION_LIBRARY_DOCTOR_FINDINGS: &str = "library-doctor-findings";
pub(super) const ACTION_IMPORT_PLAYLIST: &str = "import-playlist";
pub(super) const ACTION_STOP_PLAYBACK: &str = "stop-playback";
pub(super) const ACTION_PREFERENCES: &str = "preferences";
pub(super) const ACTION_KEYBOARD_SHORTCUTS: &str = "keyboard-shortcuts";
pub(super) const ACTION_HELP: &str = "help";
pub(super) const ACTION_ABOUT: &str = "about";
pub(super) const ACTION_OPEN_PRIMARY_MENU: &str = "open-primary-menu";
const SMOKE_MINIMAL_VIEW_ENV_VAR: &str = "REPRISE_SMOKE_MINIMAL_VIEW";

pub(super) struct Callbacks {
    pub(super) on_minimal_view: Rc<dyn Fn()>,
    pub(super) on_library_doctor: Rc<dyn Fn()>,
    pub(super) on_library_doctor_findings: Rc<dyn Fn()>,
    pub(super) on_import_playlist: Rc<dyn Fn()>,
    pub(super) on_stop_playback: Option<Rc<dyn Fn()>>,
    pub(super) on_preferences: Rc<dyn Fn()>,
}

/// View section: mode switches and personal views.
fn view_section_entries() -> Vec<(String, &'static str)> {
    vec![
        (
            strings::text(strings::COMPACT_MODE),
            "win.toggle-minimal-view",
        ),
        (
            strings::text(strings::EDIT_COLUMN_LAYOUT),
            "win.edit-column-layout",
        ),
    ]
}

fn build_library_section() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(
        Some(&strings::text(strings::LIBRARY_DOCTOR)),
        Some("win.library-doctor"),
    );
    menu.append(
        Some(&strings::text(strings::IMPORT_PLAYLIST)),
        Some("win.import-playlist"),
    );
    menu
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

fn build_primary_menu() -> gio::Menu {
    let view = gio::Menu::new();
    for (label, action) in view_section_entries() {
        view.append(Some(&label), Some(action));
    }
    let settings = gio::Menu::new();
    for (label, action) in settings_section_entries() {
        settings.append(Some(&label), Some(action));
    }
    let library = build_library_section();
    let menu = gio::Menu::new();
    menu.append_section(None, &view);
    menu.append_section(None, &library);
    menu.append_section(None, &settings);
    menu
}

#[allow(clippy::needless_pass_by_value)]
pub(super) fn install(
    header: &adw::HeaderBar,
    window: &adw::ApplicationWindow,
    active_table: &Rc<ActiveTable>,
    callbacks: Callbacks,
) {
    let menu = build_primary_menu();

    let menu_button = gtk4::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .tooltip_text(strings::shortcut_tooltip(
            strings::MAIN_MENU,
            strings::SHORTCUT_MAIN_MENU,
        ))
        .build();
    header.pack_end(&menu_button);

    let open_primary_menu = gio::SimpleAction::new(ACTION_OPEN_PRIMARY_MENU, None);
    {
        let menu_button = menu_button.downgrade();
        open_primary_menu.connect_activate(move |_, _| {
            let Some(menu_button) = menu_button.upgrade() else {
                return;
            };
            // Upgrading proves the button is alive, not that it is still in a
            // window. Compact mode detaches the whole Library tree via
            // `content_host.set_content()` while this struct keeps it alive, so
            // the weak ref upgrades on a widget with no toplevel. `popup()`
            // then realizes a popover whose parent surface is NULL, and GTK
            // dereferences it without checking — a segfault, not a warning.
            // The F10 accelerator still reaches this action in compact mode,
            // which is exactly how it was hit.
            if gtk4::prelude::WidgetExt::root(&menu_button).is_none() {
                tracing::debug!("primary menu: button is not in a window; ignoring");
                return;
            }
            menu_button.popup();
        });
    }
    window.add_action(&open_primary_menu);

    let edit = gio::SimpleAction::new(ACTION_EDIT_COLUMN_LAYOUT, None);
    edit.set_enabled(active_table.get().is_some());
    {
        let window = window.downgrade();
        let active_table = active_table.clone();
        edit.connect_activate(move |_, _| {
            let Some(window) = window.upgrade() else {
                return;
            };
            let Some(model) = active_table.get() else {
                return;
            };
            crate::ui::table_columns::editor::present_dialog(&window, &model);
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

    let library_doctor = gio::SimpleAction::new(ACTION_LIBRARY_DOCTOR, None);
    {
        let cb = callbacks.on_library_doctor.clone();
        library_doctor.connect_activate(move |_, _| cb());
    }
    window.add_action(&library_doctor);

    let library_doctor_findings = gio::SimpleAction::new(ACTION_LIBRARY_DOCTOR_FINDINGS, None);
    {
        let cb = callbacks.on_library_doctor_findings.clone();
        library_doctor_findings.connect_activate(move |_, _| cb());
    }
    window.add_action(&library_doctor_findings);

    let import_playlist = gio::SimpleAction::new(ACTION_IMPORT_PLAYLIST, None);
    {
        let cb = callbacks.on_import_playlist.clone();
        import_playlist.connect_activate(move |_, _| cb());
    }
    window.add_action(&import_playlist);

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

#[cfg(test)]
mod tests {
    use super::*;
    // Narrow, not a glob: this module already imports `gio::prelude::*`, whose
    // `Mount::root` and `set_content` shadow the GTK widget methods of the same
    // name. A second glob would deepen that.
    use gtk4::prelude::BoxExt;

    struct FakeModel(&'static str);

    impl crate::ui::table_columns::EditorModel for FakeModel {
        fn title(&self) -> String {
            self.0.to_owned()
        }

        fn columns(&self) -> Vec<crate::ui::table_columns::ColumnDescriptor> {
            Vec::new()
        }

        fn is_visible(&self, _id: &str) -> bool {
            false
        }

        fn set_visible(&self, _id: &str, _visible: bool) {}

        fn move_column(&self, _id: &str, _target: &str, _after: bool) {}

        fn reset(&self) {}
    }

    fn fake_model(title: &'static str) -> Rc<dyn crate::ui::table_columns::EditorModel> {
        Rc::new(FakeModel(title))
    }

    /// STYLE-10: the keyboard route addresses the table the user is looking
    /// at and has no stale target on a non-table surface.
    #[test]
    fn style_10_the_menu_action_follows_the_active_table() {
        let active = ActiveTable::default();
        assert!(active.get().is_none(), "no table, no target");
        active.set(Some(fake_model("Releases")));
        assert_eq!(active.get().expect("a table").title(), "Releases");
        active.set(None);
        assert!(active.get().is_none());
    }

    #[test]
    fn view_section_starts_with_compact_mode() {
        let entries = view_section_entries();
        assert_eq!(entries[0].1, "win.toggle-minimal-view");
    }

    #[test]
    fn primary_menu_omits_stop_playback_when_transport_is_persistent() {
        let menu = build_primary_menu();
        let actions = (0..menu.n_items())
            .filter_map(|index| menu.item_link(index, gio::MENU_LINK_SECTION))
            .flat_map(|section| {
                (0..section.n_items()).filter_map(move |index| {
                    section
                        .item_attribute_value(index, "action", Some(glib::VariantTy::STRING))
                        .and_then(|value| value.get::<String>())
                })
            })
            .collect::<Vec<_>>();
        assert!(
            !actions.iter().any(|action| action == "win.stop-playback"),
            "the persistent Player Bar owns playback controls"
        );
    }

    #[test]
    fn doc_8a_the_menu_carries_exactly_one_library_doctor_item_and_no_sync_device() {
        let menu = build_library_section();
        let actions: Vec<_> = (0..menu.n_items())
            .filter_map(|i| {
                menu.item_attribute_value(i, "action", Some(glib::VariantTy::STRING))
                    .and_then(|v| v.get::<String>())
            })
            .collect();
        assert_eq!(actions, ["win.library-doctor", "win.import-playlist"]);
        assert_eq!(
            actions
                .iter()
                .filter(|action| action.as_str() == "win.library-doctor")
                .count(),
            1
        );
        assert!(!actions.iter().any(|action| action == "win.sync-device"));
    }

    #[test]
    fn nav_14_import_playlist_lives_in_the_overflow_menu() {
        let menu = build_library_section();
        let actions = (0..menu.n_items())
            .filter_map(|index| {
                menu.item_attribute_value(index, "action", Some(glib::VariantTy::STRING))
                    .and_then(|value| value.get::<String>())
            })
            .collect::<Vec<_>>();

        assert!(actions.iter().any(|action| action == "win.import-playlist"));
        assert!(!actions.iter().any(|action| action == "win.sync-device"));
    }

    #[test]
    fn nav_15_library_section_omits_manual_analysis() {
        let menu = build_library_section();
        let actions = (0..menu.n_items())
            .filter_map(|index| {
                menu.item_attribute_value(index, "action", Some(glib::VariantTy::STRING))
                    .and_then(|value| value.get::<String>())
            })
            .collect::<Vec<_>>();
        assert!(!actions.iter().any(|action| action == "win.analyze-library"));
    }

    #[test]
    fn nav_15_library_section_has_no_stop_analysis_state() {
        let menu = build_library_section();
        let labels = (0..menu.n_items())
            .filter_map(|index| {
                menu.item_attribute_value(index, "label", Some(glib::VariantTy::STRING))
                    .and_then(|value| value.get::<String>())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            labels,
            [
                strings::text(strings::LIBRARY_DOCTOR),
                strings::text(strings::IMPORT_PLAYLIST)
            ]
        );
    }

    #[test]
    fn nav_15_library_section_is_static() {
        let menu = build_library_section();
        let actions = (0..menu.n_items())
            .filter_map(|index| {
                menu.item_attribute_value(index, "action", Some(glib::VariantTy::STRING))
                    .and_then(|value| value.get::<String>())
            })
            .collect::<Vec<_>>();
        assert_eq!(actions, ["win.library-doctor", "win.import-playlist"]);
    }

    #[test]
    fn nav_15_library_section_omits_header_rescan() {
        let menu = build_library_section();
        let actions = (0..menu.n_items())
            .filter_map(|index| {
                menu.item_attribute_value(index, "action", Some(glib::VariantTy::STRING))
                    .and_then(|value| value.get::<String>())
            })
            .collect::<Vec<_>>();
        assert!(!actions.iter().any(|action| action == "win.rescan-library"));
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
    #[ignore = "requires a display; run via xvfb-run"]
    fn compact_mode_unroots_the_menu_button_so_popup_needs_a_root_guard() {
        // Reproduces the shape that segfaulted: compact mode swaps the window
        // content, which detaches the whole Library tree — header bar and menu
        // button included — while the owning struct keeps it alive. A weak ref
        // still upgrades, so liveness is not the property to check. Calling
        // `popup()` here realizes a popover whose parent surface is NULL and
        // GTK dereferences it unchecked, which is a crash rather than a
        // warning; the F10 window action reaches it in compact mode.
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let window = adw::ApplicationWindow::builder().build();
        let content_host = adw::ToolbarView::new();
        adw::prelude::AdwApplicationWindowExt::set_content(&window, Some(&content_host));

        let library_root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let header = adw::HeaderBar::new();
        let menu_button = gtk4::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&gio::Menu::new())
            .build();
        header.pack_end(&menu_button);
        library_root.append(&header);
        adw::ToolbarView::set_content(&content_host, Some(&library_root));

        assert!(
            gtk4::prelude::WidgetExt::root(&menu_button).is_some(),
            "sanity: the button is in a window while the Library tree is mounted"
        );

        // enter_compact()'s `content_host.set_content(compact_root)`.
        let compact_root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        adw::ToolbarView::set_content(&content_host, Some(&compact_root));

        // The struct keeps `full_root` alive across the swap, so a weak ref
        // still upgrades — which is why liveness was the wrong guard.
        assert!(
            gtk4::prelude::WidgetExt::parent(&library_root).is_none(),
            "the Library tree is detached but still owned by the caller"
        );
        assert!(
            gtk4::prelude::WidgetExt::root(&menu_button).is_none(),
            "compact mode must leave the menu button unrooted — if this ever \
             stops holding, the popup guard is testing the wrong property"
        );

        // Remounting restores it, so the guard must not latch.
        adw::ToolbarView::set_content(&content_host, Some(&library_root));
        assert!(gtk4::prelude::WidgetExt::root(&menu_button).is_some());
    }

    #[test]
    fn no_rhythmbox_import_in_visible_menu() {
        let library = build_library_section();
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
