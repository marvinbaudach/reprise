use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library::settings::CompactLayout;
use reprise_core::queue::Repeat;

use super::compact_player_layouts::{layout_from_token, layout_token};
use super::strings;

pub(super) const LAYOUT_TARGETS: [&str; 3] = ["cover", "pill", "card"];
pub(super) const LAYOUT_NAMES: [(CompactLayout, &str); 3] = [
    (CompactLayout::Cover, strings::COMPACT_LAYOUT_COVER),
    (CompactLayout::Pill, strings::COMPACT_LAYOUT_PILL),
    (CompactLayout::Card, strings::COMPACT_LAYOUT_CARD),
];

const ACTION_LAYOUT: &str = "layout";
const ACTION_RESTORE: &str = "restore";
const ACTION_SHUFFLE: &str = "shuffle";
const ACTION_REPEAT: &str = "repeat";
const ACTION_PREFERENCES: &str = "preferences";
pub(super) const MENU_ACTIONS: [&str; 5] = [
    ACTION_RESTORE,
    ACTION_LAYOUT,
    ACTION_SHUFFLE,
    ACTION_REPEAT,
    ACTION_PREFERENCES,
];

type VoidCallback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;
type LayoutCallback = Rc<RefCell<Option<Rc<dyn Fn(CompactLayout)>>>>;
type BoolCallback = Rc<RefCell<Option<Rc<dyn Fn(bool)>>>>;
type RepeatCallback = Rc<RefCell<Option<Rc<dyn Fn(Repeat)>>>>;

pub(super) struct CompactMenu {
    pub(super) popover: gtk4::PopoverMenu,
    pub(super) action_group: gio::SimpleActionGroup,
    layout_action: gio::SimpleAction,
    shuffle_action: gio::SimpleAction,
    repeat_action: gio::SimpleAction,
    on_restore: VoidCallback,
    on_layout: LayoutCallback,
    on_shuffle: BoolCallback,
    on_repeat: RepeatCallback,
    on_preferences: VoidCallback,
}

impl CompactMenu {
    pub(super) fn build(initial_layout: CompactLayout) -> Self {
        let on_restore = empty_callback();
        let on_layout: LayoutCallback = Rc::new(RefCell::new(None));
        let on_shuffle: BoolCallback = Rc::new(RefCell::new(None));
        let on_repeat: RepeatCallback = Rc::new(RefCell::new(None));
        let on_preferences = empty_callback();
        let action_group = gio::SimpleActionGroup::new();

        let restore_action = gio::SimpleAction::new(ACTION_RESTORE, None);
        connect_void_action(&restore_action, &on_restore);
        action_group.add_action(&restore_action);

        let layout_action = gio::SimpleAction::new_stateful(
            ACTION_LAYOUT,
            Some(glib::VariantTy::STRING),
            &layout_token(initial_layout).to_variant(),
        );
        {
            let callback = on_layout.clone();
            layout_action.connect_change_state(move |_, value| {
                let Some(layout) = value
                    .and_then(glib::Variant::get::<String>)
                    .as_deref()
                    .and_then(layout_from_token)
                else {
                    return;
                };
                let callback = callback.borrow().clone();
                if let Some(callback) = callback {
                    callback(layout);
                }
            });
        }
        action_group.add_action(&layout_action);

        let shuffle_action =
            gio::SimpleAction::new_stateful(ACTION_SHUFFLE, None, &false.to_variant());
        {
            let callback = on_shuffle.clone();
            shuffle_action.connect_change_state(move |_, value| {
                let Some(active) = value.and_then(glib::Variant::get::<bool>) else {
                    return;
                };
                let callback = callback.borrow().clone();
                if let Some(callback) = callback {
                    callback(active);
                }
            });
        }
        action_group.add_action(&shuffle_action);

        let repeat_action = gio::SimpleAction::new_stateful(
            ACTION_REPEAT,
            Some(glib::VariantTy::STRING),
            &repeat_token(Repeat::Off).to_variant(),
        );
        {
            let callback = on_repeat.clone();
            repeat_action.connect_change_state(move |_, value| {
                let Some(repeat) = value
                    .and_then(glib::Variant::get::<String>)
                    .as_deref()
                    .and_then(repeat_from_token)
                else {
                    return;
                };
                let callback = callback.borrow().clone();
                if let Some(callback) = callback {
                    callback(repeat);
                }
            });
        }
        action_group.add_action(&repeat_action);

        let preferences_action = gio::SimpleAction::new(ACTION_PREFERENCES, None);
        connect_void_action(&preferences_action, &on_preferences);
        action_group.add_action(&preferences_action);
        debug_assert!(MENU_ACTIONS
            .iter()
            .all(|action| action_group.has_action(action)));

        let menu_model = menu_model();
        let popover = gtk4::PopoverMenu::from_model(Some(&menu_model));
        popover.set_has_arrow(false);

        Self {
            popover,
            action_group,
            layout_action,
            shuffle_action,
            repeat_action,
            on_restore,
            on_layout,
            on_shuffle,
            on_repeat,
            on_preferences,
        }
    }

    pub(super) fn layout_action(&self) -> gio::SimpleAction {
        self.layout_action.clone()
    }

    pub(super) fn set_shuffle(&self, active: bool) {
        self.shuffle_action.set_state(&active.to_variant());
    }

    pub(super) fn set_repeat(&self, repeat: Repeat) {
        self.repeat_action
            .set_state(&repeat_token(repeat).to_variant());
    }

    pub(super) fn set_on_restore(&self, callback: Rc<dyn Fn()>) {
        *self.on_restore.borrow_mut() = Some(callback);
    }

    pub(super) fn set_on_layout(&self, callback: Rc<dyn Fn(CompactLayout)>) {
        *self.on_layout.borrow_mut() = Some(callback);
    }

    pub(super) fn set_on_shuffle(&self, callback: Rc<dyn Fn(bool)>) {
        *self.on_shuffle.borrow_mut() = Some(callback);
    }

    pub(super) fn set_on_repeat(&self, callback: Rc<dyn Fn(Repeat)>) {
        *self.on_repeat.borrow_mut() = Some(callback);
    }

    pub(super) fn set_on_preferences(&self, callback: Rc<dyn Fn()>) {
        *self.on_preferences.borrow_mut() = Some(callback);
    }
}

fn menu_model() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(
        Some(&strings::text(strings::RETURN_TO_LIBRARY)),
        Some("compact.restore"),
    );

    let layouts = gio::Menu::new();
    for ((layout, name), target) in LAYOUT_NAMES.into_iter().zip(LAYOUT_TARGETS) {
        debug_assert_eq!(layout_token(layout), target);
        layouts.append(
            Some(&strings::text(name)),
            Some(&format!("compact.layout::{target}")),
        );
    }
    menu.append_submenu(Some(&strings::text(strings::COMPACT_LAYOUT)), &layouts);
    menu.append(
        Some(&strings::text(strings::SHUFFLE)),
        Some("compact.shuffle"),
    );

    let repeats = gio::Menu::new();
    for (repeat, name) in [
        (Repeat::Off, strings::REPEAT_OFF),
        (Repeat::All, strings::REPEAT_ALL),
        (Repeat::One, strings::REPEAT_ONE),
    ] {
        repeats.append(
            Some(&strings::text(name)),
            Some(&format!("compact.repeat::{}", repeat_token(repeat))),
        );
    }
    menu.append_submenu(Some(&strings::text(strings::REPEAT)), &repeats);

    menu.append(
        Some(&strings::text(strings::PREFERENCES)),
        Some("compact.preferences"),
    );
    menu
}

fn connect_void_action(action: &gio::SimpleAction, callback: &VoidCallback) {
    let callback = callback.clone();
    action.connect_activate(move |_, _| {
        let callback = callback.borrow().clone();
        if let Some(callback) = callback {
            callback();
        }
    });
}

fn empty_callback() -> VoidCallback {
    Rc::new(RefCell::new(None))
}

pub(super) const fn active_target(layout: CompactLayout) -> &'static str {
    layout_token(layout)
}

const fn repeat_token(repeat: Repeat) -> &'static str {
    match repeat {
        Repeat::Off => "off",
        Repeat::All => "all",
        Repeat::One => "one",
    }
}

fn repeat_from_token(token: &str) -> Option<Repeat> {
    match token {
        "off" => Some(Repeat::Off),
        "all" => Some(Repeat::All),
        "one" => Some(Repeat::One),
        _ => None,
    }
}

pub(super) const fn accepts_context_menu(interactive_descendant: bool) -> bool {
    !interactive_descendant
}

pub(super) fn popup_at(
    popover: &gtk4::PopoverMenu,
    anchor: &gtk4::Widget,
    point: Option<(i32, i32)>,
) {
    popover.popdown();
    if popover.parent().is_some() {
        popover.unparent();
    }
    popover.set_parent(anchor);
    if let Some((x, y)) = point {
        popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x, y, 1, 1)));
    } else {
        popover.set_pointing_to(None);
    }
    popover.popup();
}

pub(super) fn is_context_menu_shortcut(
    key: gtk4::gdk::Key,
    modifiers: gtk4::gdk::ModifierType,
) -> bool {
    key == gtk4::gdk::Key::Menu
        || (key == gtk4::gdk::Key::F10 && modifiers.contains(gtk4::gdk::ModifierType::SHIFT_MASK))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn layout_radio_targets_are_complete_and_stable() {
        assert_eq!(LAYOUT_TARGETS, ["cover", "pill", "card"]);
    }

    #[test]
    fn active_radio_follows_the_selected_layout() {
        for (layout, target) in [
            (CompactLayout::Cover, "cover"),
            (CompactLayout::Pill, "pill"),
            (CompactLayout::Card, "card"),
        ] {
            assert_eq!(active_target(layout), target);
        }
    }

    #[test]
    fn context_menu_is_limited_to_non_interactive_regions() {
        assert!(accepts_context_menu(false));
        assert!(!accepts_context_menu(true));
    }

    #[test]
    fn menu_has_only_the_supported_native_actions() {
        assert_eq!(
            MENU_ACTIONS,
            ["restore", "layout", "shuffle", "repeat", "preferences"]
        );
        let mut actions = BTreeSet::new();
        let model = menu_model();
        assert!(!collect_model_contract(model.upcast_ref(), &mut actions));
        assert_eq!(
            actions,
            [
                "compact.layout".to_string(),
                "compact.preferences".to_string(),
                "compact.repeat".to_string(),
                "compact.restore".to_string(),
                "compact.shuffle".to_string(),
            ]
            .into_iter()
            .collect()
        );
    }

    fn collect_model_contract(model: &gio::MenuModel, actions: &mut BTreeSet<String>) -> bool {
        let mut has_custom = false;
        for item in 0..model.n_items() {
            if let Some(action) = model
                .item_attribute_value(item, "action", Some(glib::VariantTy::STRING))
                .and_then(|value| value.get::<String>())
            {
                actions.insert(action);
            }
            has_custom |= model.item_attribute_value(item, "custom", None).is_some();
            for link in ["section", "submenu"] {
                if let Some(child) = model.item_link(item, link) {
                    has_custom |= collect_model_contract(&child, actions);
                }
            }
        }
        has_custom
    }
}
