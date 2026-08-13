use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::prelude::*;
use reprise_core::modules::{
    ModuleDescriptor, CONCERTS_MODULE, NEW_RELEASES_MODULE, PODCASTS_MODULE, RADIO_MODULE,
    YOUTUBE_MODULE,
};
use reprise_core::view_source::ViewSource;

use super::{show_toast, show_toast_with_action, Shared};
use crate::ui::{popover_lifecycle, strings};

const ACTION_DISABLE: &str = "disable";
const ACTION_GROUP: &str = "sidebarmodule";
const ACTION_SETTINGS: &str = "settings";
const OPTIONAL_MODULE_SOURCES: &[(ViewSource, &ModuleDescriptor)] = &[
    (ViewSource::Podcasts, &PODCASTS_MODULE),
    (ViewSource::Youtube, &YOUTUBE_MODULE),
    (ViewSource::Radio, &RADIO_MODULE),
    (ViewSource::Releases, &NEW_RELEASES_MODULE),
    (ViewSource::Concerts, &CONCERTS_MODULE),
];

pub(in crate::ui) struct ModuleMenuHighlight {
    generation: Cell<u64>,
    target: RefCell<gtk4::glib::WeakRef<gtk4::ListBoxRow>>,
}

impl ModuleMenuHighlight {
    pub(in crate::ui) fn new() -> Self {
        Self {
            generation: Cell::new(0),
            target: RefCell::new(gtk4::glib::WeakRef::new()),
        }
    }

    fn begin(&self, row: &gtk4::ListBoxRow) -> u64 {
        let previous = self.target.borrow().upgrade();
        if let Some(previous) = previous {
            previous.remove_css_class(crate::ui::preference_plugins::TARGET_CLASS);
        }
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.target.borrow_mut().set(Some(row));
        row.add_css_class(crate::ui::preference_plugins::TARGET_CLASS);
        generation
    }

    fn finish(&self, generation: u64) {
        if self.generation.get() != generation {
            return;
        }
        let target = self.target.borrow().upgrade();
        self.target.borrow_mut().set(None::<&gtk4::ListBoxRow>);
        if let Some(target) = target {
            target.remove_css_class(crate::ui::preference_plugins::TARGET_CLASS);
        }
    }
}

pub(in crate::ui) type OnSetModuleEnabled =
    Rc<dyn Fn(&'static ModuleDescriptor, bool) -> Result<(), String>>;
pub(in crate::ui) type OnPresentPlugins = Rc<dyn Fn(&[&'static str])>;

pub(in crate::ui) fn dispatch_present_plugins(
    callback: Option<OnPresentPlugins>,
    targets: &[&'static str],
) {
    if let Some(callback) = callback {
        callback(targets);
    }
}

fn dispatch_set_enabled(
    callback: Option<OnSetModuleEnabled>,
    module: &'static ModuleDescriptor,
    enabled: bool,
) -> Result<(), String> {
    let callback = callback.ok_or_else(|| "module state route is not wired".to_string())?;
    callback(module, enabled)
}

fn module_for_source(source: &ViewSource) -> Option<&'static ModuleDescriptor> {
    OPTIONAL_MODULE_SOURCES
        .iter()
        .find_map(|(candidate, module)| (candidate == source).then_some(*module))
}

fn source_for_module(module: &ModuleDescriptor) -> Option<ViewSource> {
    OPTIONAL_MODULE_SOURCES
        .iter()
        .find_map(|(source, candidate)| (candidate.id == module.id).then(|| source.clone()))
}

pub(in crate::ui) fn wire(
    shared: &Rc<Shared>,
    row: &gtk4::ListBoxRow,
    source: &ViewSource,
    title: &str,
) {
    let Some(module) = module_for_source(source) else {
        return;
    };
    row.update_property(&[gtk4::accessible::Property::KeyShortcuts("Menu Shift+F10")]);

    // input-parity: ACC-8 keyboard=menu-shift-f10
    let gesture = crate::ui::source_context_surface::secondary_click();
    {
        let shared = Rc::downgrade(shared);
        let title = title.to_string();
        gesture.connect_pressed(move |gesture, _, _, _| {
            let Some(shared) = shared.upgrade() else {
                return;
            };
            let Some(row) = gesture.widget().and_downcast::<gtk4::ListBoxRow>() else {
                return;
            };
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            show(&shared, &row, module, &title);
        });
    }
    row.add_controller(gesture);

    let keys = crate::ui::source_context_surface::context_keys();
    {
        let shared = Rc::downgrade(shared);
        let title = title.to_string();
        keys.connect_key_pressed(move |keys, key, _, modifiers| {
            if !crate::ui::source_context_surface::is_context_menu_shortcut(key, modifiers) {
                return gtk4::glib::Propagation::Proceed;
            }
            let Some(shared) = shared.upgrade() else {
                return gtk4::glib::Propagation::Proceed;
            };
            let Some(row) = keys.widget().and_downcast::<gtk4::ListBoxRow>() else {
                return gtk4::glib::Propagation::Proceed;
            };
            show(&shared, &row, module, &title);
            gtk4::glib::Propagation::Stop
        });
    }
    row.add_controller(keys);
}

fn show(
    shared: &Rc<Shared>,
    row: &gtk4::ListBoxRow,
    module: &'static ModuleDescriptor,
    title: &str,
) {
    let actions = gio::SimpleActionGroup::new();
    let disable = gio::SimpleAction::new(ACTION_DISABLE, None);
    {
        let shared = Rc::downgrade(shared);
        let title = title.to_string();
        disable.connect_activate(move |_, _| {
            if let Some(shared) = shared.upgrade() {
                disable_module(&shared, module, &title);
            }
        });
    }
    actions.add_action(&disable);
    let settings = gio::SimpleAction::new(ACTION_SETTINGS, None);
    {
        let shared = Rc::downgrade(shared);
        settings.connect_activate(move |_, _| {
            let Some(shared) = shared.upgrade() else {
                return;
            };
            let callback = shared.on_present_plugins.borrow().clone();
            dispatch_present_plugins(callback, &[module.id]);
        });
    }
    actions.add_action(&settings);
    row.insert_action_group(ACTION_GROUP, Some(&actions));

    let menu = gio::Menu::new();
    menu.append(
        Some(&strings::sidebar_turn_off(title)),
        Some(&format!("{ACTION_GROUP}.{ACTION_DISABLE}")),
    );
    menu.append(
        Some(&strings::sidebar_module_settings(title)),
        Some(&format!("{ACTION_GROUP}.{ACTION_SETTINGS}")),
    );
    let popover = gtk4::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(row);
    popover.set_has_arrow(true);
    popover.set_position(gtk4::PositionType::Right);
    popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(
        0,
        0,
        row.width(),
        row.height(),
    )));
    let highlight_generation = shared.module_menu_highlight.begin(row);
    {
        let shared = Rc::downgrade(shared);
        popover.connect_closed(move |_| {
            if let Some(shared) = shared.upgrade() {
                shared.module_menu_highlight.finish(highlight_generation);
            }
        });
    }
    popover_lifecycle::unparent_after_actions(popover.upcast_ref());
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(row);
    focus_guard.restore_on_popover_close(popover.upcast_ref());
    popover.popup();
}

fn disable_module(
    shared: &Rc<Shared>,
    module: &'static ModuleDescriptor,
    title: &str,
) -> Option<libadwaita::Toast> {
    let callback = shared.on_module_enabled.borrow().clone();
    let source = source_for_module(module);
    let current_source = shared.current_source.borrow().clone();
    let fell_back = source.as_ref() == Some(&current_source);
    if let Err(error) = dispatch_set_enabled(callback.clone(), module, false) {
        tracing::warn!(%error, module = module.id, "could not disable sidebar module");
        show_toast(shared, &strings::sidebar_turn_off_failed(title));
        return None;
    }

    let message = if fell_back {
        strings::sidebar_turned_off_showing_music(title)
    } else {
        strings::sidebar_turned_off(title)
    };
    let undone = Rc::new(Cell::new(false));
    let shared_weak = Rc::downgrade(shared);
    let undo_flag = undone.clone();
    show_toast_with_action(shared, &message, &strings::text(strings::UNDO), move || {
        if undo_flag.replace(true) {
            return;
        }
        let Some(shared) = shared_weak.upgrade() else {
            return;
        };
        if let Err(error) = dispatch_set_enabled(callback.clone(), module, true) {
            tracing::warn!(%error, module = module.id, "could not restore sidebar module");
            return;
        }
        if !fell_back {
            return;
        }
        let current_source = shared.current_source.borrow().clone();
        if current_source != ViewSource::Library {
            return;
        }
        let Some(source) = source.clone() else {
            return;
        };
        if let Some(row) = super::find_row(&shared, &source) {
            super::select_row_in_its_listbox(&row);
        }
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gtk4::prelude::*;
    use libadwaita as adw;
    use libadwaita::prelude::*;
    use reprise_core::modules::PODCASTS_MODULE;
    use reprise_core::view_source::ViewSource;

    use super::{
        disable_module, dispatch_present_plugins, dispatch_set_enabled, module_for_source,
        source_for_module, OnPresentPlugins, OnSetModuleEnabled,
    };
    use crate::ui::sidebar::{find_row, Sidebar};

    #[test]
    fn nav_16_only_optional_module_rows_offer_turn_off() {
        let optional = [
            (ViewSource::Podcasts, "podcasts"),
            (ViewSource::Youtube, "youtube"),
            (ViewSource::Radio, "radio"),
            (ViewSource::Releases, "new_releases"),
            (ViewSource::Concerts, "concerts"),
        ];
        for (source, expected_id) in optional {
            assert_eq!(
                module_for_source(&source).map(|module| module.id),
                Some(expected_id)
            );
        }

        for permanent in [
            ViewSource::Library,
            ViewSource::Queue,
            ViewSource::Playlist(7),
            ViewSource::Smart(9),
            ViewSource::RecentlyAdded,
            ViewSource::MyStats,
        ] {
            assert!(
                module_for_source(&permanent).is_none(),
                "{} must remain a permanent sidebar route",
                permanent.label()
            );
        }
    }

    #[test]
    fn nav_16_optional_module_sources_round_trip_both_directions() {
        for (source, module) in [
            (
                ViewSource::Podcasts,
                &reprise_core::modules::PODCASTS_MODULE,
            ),
            (ViewSource::Youtube, &reprise_core::modules::YOUTUBE_MODULE),
            (ViewSource::Radio, &reprise_core::modules::RADIO_MODULE),
            (
                ViewSource::Releases,
                &reprise_core::modules::NEW_RELEASES_MODULE,
            ),
            (
                ViewSource::Concerts,
                &reprise_core::modules::CONCERTS_MODULE,
            ),
        ] {
            assert_eq!(
                module_for_source(&source).map(|candidate| candidate.id),
                Some(module.id)
            );
            assert_eq!(source_for_module(module), Some(source));
        }
    }

    #[test]
    fn nav_16_turn_off_dispatches_the_clicked_module_once() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let seen_for_callback = seen.clone();
        let callback: OnSetModuleEnabled = Rc::new(move |module, enabled| {
            seen_for_callback.borrow_mut().push((module.id, enabled));
            Ok(())
        });

        dispatch_set_enabled(Some(callback.clone()), &PODCASTS_MODULE, false).unwrap();
        dispatch_set_enabled(Some(callback), &PODCASTS_MODULE, true).unwrap();

        assert_eq!(&*seen.borrow(), &[("podcasts", false), ("podcasts", true)]);
        assert!(dispatch_set_enabled(None, &PODCASTS_MODULE, false).is_err());
    }

    #[test]
    fn nav_16_module_settings_dispatches_the_clicked_module() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let seen_for_callback = seen.clone();
        let callback: OnPresentPlugins = Rc::new(move |targets| {
            seen_for_callback.borrow_mut().extend_from_slice(targets);
        });

        dispatch_present_plugins(Some(callback), &[PODCASTS_MODULE.id]);

        assert_eq!(&*seen.borrow(), &["podcasts"]);
    }

    #[test]
    fn nav_16_turned_off_row_tracks_every_disabled_optional_module() {
        let conn = crate::test_db::open().unwrap();
        for module in [
            &reprise_core::modules::PODCASTS_MODULE,
            &reprise_core::modules::YOUTUBE_MODULE,
            &reprise_core::modules::RADIO_MODULE,
            &reprise_core::modules::NEW_RELEASES_MODULE,
            &reprise_core::modules::CONCERTS_MODULE,
        ] {
            reprise_core::modules::set_enabled(&conn, module, true).unwrap();
        }
        assert!(crate::ui::sidebar::sidebar_rebuild::turned_off_modules(&conn).is_empty());

        reprise_core::modules::set_enabled(&conn, &reprise_core::modules::PODCASTS_MODULE, false)
            .unwrap();
        reprise_core::modules::set_enabled(&conn, &reprise_core::modules::CONCERTS_MODULE, false)
            .unwrap();

        assert_eq!(
            crate::ui::sidebar::sidebar_rebuild::turned_off_modules(&conn),
            vec!["podcasts", "concerts"]
        );
    }

    fn assert_row_anchored(popover: &gtk4::PopoverMenu, row: &gtk4::ListBoxRow) {
        assert!(popover.has_arrow());
        assert_eq!(popover.position(), gtk4::PositionType::Right);
        let (has_pointing_rect, pointing_rect) = popover.pointing_to();
        assert!(has_pointing_rect);
        assert_eq!(
            pointing_rect,
            gtk4::gdk::Rectangle::new(0, 0, row.width(), row.height())
        );
    }

    fn context_gesture(widget: &gtk4::Widget) -> gtk4::GestureClick {
        let controllers = widget.observe_controllers();
        (0..controllers.n_items())
            .find_map(|index| {
                controllers
                    .item(index)?
                    .downcast::<gtk4::GestureClick>()
                    .ok()
                    .filter(|gesture| gesture.button() == gtk4::gdk::BUTTON_SECONDARY)
            })
            .expect("optional sidebar row secondary-click gesture")
    }

    fn context_keys(widget: &gtk4::Widget) -> gtk4::EventControllerKey {
        let controllers = widget.observe_controllers();
        (0..controllers.n_items())
            .find_map(|index| {
                controllers
                    .item(index)?
                    .downcast::<gtk4::EventControllerKey>()
                    .ok()
                    .filter(|keys| keys.propagation_phase() == gtk4::PropagationPhase::Capture)
            })
            .expect("optional sidebar row context-menu keys")
    }

    fn attached_popover(widget: &gtk4::Widget) -> gtk4::PopoverMenu {
        std::iter::successors(widget.first_child(), gtk4::prelude::WidgetExt::next_sibling)
            .find_map(|child| child.downcast::<gtk4::PopoverMenu>().ok())
            .expect("optional sidebar row popover")
    }

    fn turned_off_action_row(listbox: &gtk4::ListBox) -> Option<gtk4::ListBoxRow> {
        std::iter::successors(
            listbox.first_child(),
            gtk4::prelude::WidgetExt::next_sibling,
        )
        .filter_map(|child| child.downcast::<gtk4::ListBoxRow>().ok())
        .find(|row| row.has_css_class("reprise-turned-off-modules"))
    }

    fn menu_has_action(model: &gtk4::gio::MenuModel, expected: &str) -> bool {
        (0..model.n_items()).any(|item| {
            model
                .item_attribute_value(item, "action", Some(gtk4::glib::VariantTy::STRING))
                .and_then(|value| value.get::<String>())
                .as_deref()
                == Some(expected)
        })
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_16_secondary_click_turns_off_the_row_and_falls_back_to_music() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        reprise_core::online_sources::set_enabled(&conn, true).unwrap();
        reprise_core::modules::set_enabled(&conn, &PODCASTS_MODULE, true).unwrap();
        let window = adw::ApplicationWindow::builder().build();
        let sidebar = Rc::new(Sidebar::new(conn.clone(), &window, || 0));
        {
            let conn = conn.clone();
            let sidebar_weak = Rc::downgrade(&sidebar);
            sidebar.set_on_module_enabled(move |module, enabled| {
                reprise_core::modules::set_enabled(&conn, module, enabled)
                    .map_err(|error| error.to_string())?;
                if let Some(sidebar) = sidebar_weak.upgrade() {
                    sidebar.refresh("test module disabled");
                }
                Ok(())
            });
        }
        sidebar.refresh_and_select(ViewSource::Podcasts, "test select Podcasts");
        window.set_content(Some(sidebar.widget()));
        window.present();
        crate::ui::source_context_surface::settle_layout();

        let row = find_row(&sidebar.shared, &ViewSource::Podcasts).unwrap();
        assert_eq!(
            context_gesture(row.upcast_ref()).propagation_phase(),
            gtk4::PropagationPhase::Capture
        );
        assert_eq!(
            context_keys(row.upcast_ref()).propagation_phase(),
            gtk4::PropagationPhase::Capture
        );
        context_gesture(row.upcast_ref()).emit_by_name::<()>("pressed", &[&1i32, &8.0f64, &8.0f64]);
        let popover = attached_popover(row.upcast_ref());
        assert!(popover.is_visible());
        assert_row_anchored(&popover, &row);
        assert!(row.has_css_class("reprise-plugin-target"));
        assert!(menu_has_action(
            &popover.menu_model().expect("sidebar module menu model"),
            "sidebarmodule.disable"
        ));
        assert!(menu_has_action(
            &popover.menu_model().expect("sidebar module menu model"),
            "sidebarmodule.settings"
        ));

        popover.popdown();
        crate::ui::source_context_surface::settle_layout();
        assert!(!row.has_css_class("reprise-plugin-target"));

        let handled = context_keys(row.upcast_ref()).emit_by_name::<bool>(
            "key-pressed",
            &[
                &gtk4::gdk::Key::F10,
                &0u32,
                &gtk4::gdk::ModifierType::SHIFT_MASK,
            ],
        );
        assert!(handled);
        let popover = attached_popover(row.upcast_ref());
        assert!(popover.is_visible());
        assert_row_anchored(&popover, &row);
        assert!(row.has_css_class("reprise-plugin-target"));

        row.activate_action("sidebarmodule.disable", None).unwrap();

        assert!(!reprise_core::modules::is_enabled(&conn, &PODCASTS_MODULE).unwrap());
        assert!(find_row(&sidebar.shared, &ViewSource::Podcasts).is_none());
        assert_eq!(*sidebar.shared.current_source.borrow(), ViewSource::Library);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_16_turn_off_posts_undo_and_restores_the_active_module() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        reprise_core::online_sources::set_enabled(&conn, true).unwrap();
        reprise_core::modules::set_enabled(&conn, &PODCASTS_MODULE, true).unwrap();
        let window = adw::ApplicationWindow::builder().build();
        let sidebar = Rc::new(Sidebar::new(conn.clone(), &window, || 0));
        let overlay = adw::ToastOverlay::new();
        sidebar.set_toast_overlay(&overlay);
        let calls = Rc::new(RefCell::new(Vec::new()));
        {
            let conn = conn.clone();
            let calls = calls.clone();
            let sidebar_weak = Rc::downgrade(&sidebar);
            sidebar.set_on_module_enabled(move |module, enabled| {
                calls.borrow_mut().push((module.id, enabled));
                reprise_core::modules::set_enabled(&conn, module, enabled)
                    .map_err(|error| error.to_string())?;
                if let Some(sidebar) = sidebar_weak.upgrade() {
                    sidebar.refresh("test module state changed");
                }
                Ok(())
            });
        }
        sidebar.refresh_and_select(ViewSource::Podcasts, "test select Podcasts");
        overlay.set_child(Some(sidebar.widget()));
        window.set_content(Some(&overlay));
        window.present();
        crate::ui::source_context_surface::settle_layout();

        let toast = disable_module(&sidebar.shared, &PODCASTS_MODULE, "Podcasts")
            .expect("successful turn-off toast");

        assert_eq!(
            toast.title().as_deref(),
            Some("Podcasts turned off · showing Music")
        );
        assert_eq!(toast.button_label().as_deref(), Some("Undo"));
        assert_eq!(toast.timeout(), 5);
        assert!(!reprise_core::modules::is_enabled(&conn, &PODCASTS_MODULE).unwrap());
        assert!(find_row(&sidebar.shared, &ViewSource::Podcasts).is_none());
        assert_eq!(*sidebar.shared.current_source.borrow(), ViewSource::Library);

        toast.emit_by_name::<()>("button-clicked", &[]);
        toast.emit_by_name::<()>("button-clicked", &[]);

        assert!(reprise_core::modules::is_enabled(&conn, &PODCASTS_MODULE).unwrap());
        assert!(find_row(&sidebar.shared, &ViewSource::Podcasts).is_some());
        assert_eq!(
            *sidebar.shared.current_source.borrow(),
            ViewSource::Podcasts
        );
        assert_eq!(&*calls.borrow(), &[("podcasts", false), ("podcasts", true)]);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_16_turned_off_row_is_not_a_restorable_session_source() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        reprise_core::online_sources::set_enabled(&conn, true).unwrap();
        for module in [
            &reprise_core::modules::PODCASTS_MODULE,
            &reprise_core::modules::YOUTUBE_MODULE,
            &reprise_core::modules::RADIO_MODULE,
            &reprise_core::modules::NEW_RELEASES_MODULE,
            &reprise_core::modules::CONCERTS_MODULE,
        ] {
            reprise_core::modules::set_enabled(&conn, module, true).unwrap();
        }
        let window = adw::ApplicationWindow::builder().build();
        let sidebar = Sidebar::new(conn.clone(), &window, || 0);
        assert!(turned_off_action_row(&sidebar.shared.listbox).is_none());

        reprise_core::modules::set_enabled(&conn, &PODCASTS_MODULE, false).unwrap();
        reprise_core::modules::set_enabled(&conn, &reprise_core::modules::CONCERTS_MODULE, false)
            .unwrap();
        let presented = Rc::new(RefCell::new(Vec::new()));
        {
            let presented = presented.clone();
            sidebar.set_on_present_plugins(move |targets| {
                presented.borrow_mut().extend_from_slice(targets);
            });
        }
        sidebar.refresh("test optional modules disabled");

        let action_row =
            turned_off_action_row(&sidebar.shared.listbox).expect("turned-off modules action row");
        assert_eq!(
            action_row
                .child()
                .and_then(|button| button.downcast::<gtk4::Button>().ok())
                .and_then(|button| button.child())
                .and_then(|content| content.downcast::<gtk4::Box>().ok())
                .and_then(|content| content.first_child())
                .and_then(|power| power.next_sibling())
                .and_then(|title| title.downcast::<gtk4::Label>().ok())
                .map(|title| title.text().to_string())
                .as_deref(),
            Some("2 turned off")
        );
        assert_eq!(
            action_row
                .child()
                .and_then(|button| button.downcast::<gtk4::Button>().ok())
                .and_then(|button| button.child())
                .and_then(|content| content.downcast::<gtk4::Box>().ok())
                .and_then(|content| content.last_child())
                .and_then(|next| next.downcast::<gtk4::Image>().ok())
                .and_then(|next| next.icon_name())
                .as_deref(),
            Some("go-next-symbolic")
        );
        assert!(!sidebar
            .shared
            .rows
            .borrow()
            .iter()
            .any(|(row, _, _)| row == &action_row));
        assert!(find_row(&sidebar.shared, &ViewSource::Podcasts).is_none());
        let (restored, _) = sidebar.restore_source(ViewSource::Podcasts);
        assert_eq!(restored, ViewSource::Library);

        action_row.activate();
        assert_eq!(&*presented.borrow(), &["podcasts", "concerts"]);

        reprise_core::modules::set_enabled(&conn, &PODCASTS_MODULE, true).unwrap();
        reprise_core::modules::set_enabled(&conn, &reprise_core::modules::CONCERTS_MODULE, true)
            .unwrap();
        sidebar.refresh("test optional modules restored");
        assert!(turned_off_action_row(&sidebar.shared.listbox).is_none());
    }
}
