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

use super::{show_toast, Shared};
use crate::ui::{popover_lifecycle, strings};

const ACTION_DISABLE: &str = "disable";
const ACTION_GROUP: &str = "sidebarmodule";

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

pub(in crate::ui) type OnDisableModule =
    Rc<dyn Fn(&'static ModuleDescriptor) -> Result<(), String>>;

fn dispatch_disable(
    callback: Option<OnDisableModule>,
    module: &'static ModuleDescriptor,
) -> Result<(), String> {
    let callback = callback.ok_or_else(|| "module disable route is not wired".to_string())?;
    callback(module)
}

fn module_for_source(source: &ViewSource) -> Option<&'static ModuleDescriptor> {
    match source {
        ViewSource::Podcasts => Some(&PODCASTS_MODULE),
        ViewSource::Youtube => Some(&YOUTUBE_MODULE),
        ViewSource::Radio => Some(&RADIO_MODULE),
        ViewSource::Releases => Some(&NEW_RELEASES_MODULE),
        ViewSource::Concerts => Some(&CONCERTS_MODULE),
        _ => None,
    }
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
    row.insert_action_group(ACTION_GROUP, Some(&actions));

    let menu = gio::Menu::new();
    menu.append(
        Some(&strings::sidebar_turn_off(title)),
        Some(&format!("{ACTION_GROUP}.{ACTION_DISABLE}")),
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

fn disable_module(shared: &Rc<Shared>, module: &'static ModuleDescriptor, title: &str) {
    let callback = shared.on_disable_module.borrow().clone();
    if let Err(error) = dispatch_disable(callback, module) {
        tracing::warn!(%error, module = module.id, "could not disable sidebar module");
        show_toast(shared, &strings::sidebar_turn_off_failed(title));
    }
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

    use super::{dispatch_disable, module_for_source, OnDisableModule};
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
    fn nav_16_turn_off_dispatches_the_clicked_module_once() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let seen_for_callback = seen.clone();
        let callback: OnDisableModule = Rc::new(move |module| {
            seen_for_callback.borrow_mut().push(module.id);
            Ok(())
        });

        dispatch_disable(Some(callback), &PODCASTS_MODULE).unwrap();

        assert_eq!(&*seen.borrow(), &["podcasts"]);
        assert!(dispatch_disable(None, &PODCASTS_MODULE).is_err());
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
            sidebar.set_on_disable_module(move |module| {
                reprise_core::modules::set_enabled(&conn, module, false)
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
}
