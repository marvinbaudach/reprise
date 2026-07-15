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

const ACTION_RESTORE: &str = "restore";
const ACTION_PLAY_PAUSE: &str = "play-pause";
const ACTION_NEXT: &str = "next";
const ACTION_PREVIOUS: &str = "previous";
const ACTION_ALWAYS_ON_TOP: &str = "always-on-top";
const ACTION_PREFERENCES: &str = "preferences";
const ACTION_QUIT: &str = "quit";

// Kept for state management by physical controls — not shown in the menu.
const ACTION_LAYOUT: &str = "layout";
const ACTION_SHUFFLE: &str = "shuffle";
const ACTION_REPEAT: &str = "repeat";

/// Every action name registered in the compact action group.
pub(super) const MENU_ACTIONS: [&str; 10] = [
    ACTION_RESTORE,
    ACTION_PLAY_PAUSE,
    ACTION_NEXT,
    ACTION_PREVIOUS,
    ACTION_ALWAYS_ON_TOP,
    ACTION_PREFERENCES,
    ACTION_QUIT,
    ACTION_LAYOUT,
    ACTION_SHUFFLE,
    ACTION_REPEAT,
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
    always_on_top_action: gio::SimpleAction,
    playback_section: gio::Menu,
    on_restore: VoidCallback,
    on_play_pause: VoidCallback,
    on_next: VoidCallback,
    on_previous: VoidCallback,
    on_always_on_top: BoolCallback,
    on_layout: LayoutCallback,
    on_shuffle: BoolCallback,
    on_repeat: RepeatCallback,
    on_preferences: VoidCallback,
    on_quit: VoidCallback,
    is_playing: RefCell<bool>,
}

impl CompactMenu {
    pub(super) fn build(initial_layout: CompactLayout) -> Self {
        let on_restore = empty_callback();
        let on_play_pause = empty_callback();
        let on_next = empty_callback();
        let on_previous = empty_callback();
        let on_always_on_top: BoolCallback = Rc::new(RefCell::new(None));
        let on_layout: LayoutCallback = Rc::new(RefCell::new(None));
        let on_shuffle: BoolCallback = Rc::new(RefCell::new(None));
        let on_repeat: RepeatCallback = Rc::new(RefCell::new(None));
        let on_preferences = empty_callback();
        let on_quit = empty_callback();
        let action_group = gio::SimpleActionGroup::new();

        let restore_action = gio::SimpleAction::new(ACTION_RESTORE, None);
        connect_void_action(&restore_action, &on_restore);
        action_group.add_action(&restore_action);

        let play_pause_action = gio::SimpleAction::new(ACTION_PLAY_PAUSE, None);
        connect_void_action(&play_pause_action, &on_play_pause);
        action_group.add_action(&play_pause_action);

        let next_action = gio::SimpleAction::new(ACTION_NEXT, None);
        connect_void_action(&next_action, &on_next);
        action_group.add_action(&next_action);

        let previous_action = gio::SimpleAction::new(ACTION_PREVIOUS, None);
        connect_void_action(&previous_action, &on_previous);
        action_group.add_action(&previous_action);

        let always_on_top_action =
            gio::SimpleAction::new_stateful(ACTION_ALWAYS_ON_TOP, None, &false.to_variant());
        {
            let callback = on_always_on_top.clone();
            always_on_top_action.connect_change_state(move |action, value| {
                let Some(active) = value.and_then(glib::Variant::get::<bool>) else {
                    return;
                };
                action.set_state(&active.to_variant());
                let callback = callback.borrow().clone();
                if let Some(callback) = callback {
                    callback(active);
                }
            });
        }
        action_group.add_action(&always_on_top_action);

        let preferences_action = gio::SimpleAction::new(ACTION_PREFERENCES, None);
        connect_void_action(&preferences_action, &on_preferences);
        action_group.add_action(&preferences_action);

        let quit_action = gio::SimpleAction::new(ACTION_QUIT, None);
        connect_void_action(&quit_action, &on_quit);
        action_group.add_action(&quit_action);

        // State-only actions for physical controls (not shown in the menu).
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

        debug_assert!(MENU_ACTIONS
            .iter()
            .all(|action| action_group.has_action(action)));

        let playback_section = gio::Menu::new();
        let menu_model = menu_model(&playback_section, false);
        let popover = gtk4::PopoverMenu::from_model(Some(&menu_model));
        popover.set_has_arrow(false);

        Self {
            popover,
            action_group,
            layout_action,
            shuffle_action,
            repeat_action,
            always_on_top_action,
            playback_section,
            on_restore,
            on_play_pause,
            on_next,
            on_previous,
            on_always_on_top,
            on_layout,
            on_shuffle,
            on_repeat,
            on_preferences,
            on_quit,
            is_playing: RefCell::new(false),
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

    pub(super) fn set_playing(&self, playing: bool) {
        let mut current = self.is_playing.borrow_mut();
        if *current == playing {
            return;
        }
        *current = playing;
        rebuild_playback_section(&self.playback_section, playing);
    }

    pub(super) fn set_on_restore(&self, callback: Rc<dyn Fn()>) {
        *self.on_restore.borrow_mut() = Some(callback);
    }

    pub(super) fn set_on_play_pause(&self, callback: Rc<dyn Fn()>) {
        *self.on_play_pause.borrow_mut() = Some(callback);
    }

    pub(super) fn set_on_next(&self, callback: Rc<dyn Fn()>) {
        *self.on_next.borrow_mut() = Some(callback);
    }

    pub(super) fn set_on_previous(&self, callback: Rc<dyn Fn()>) {
        *self.on_previous.borrow_mut() = Some(callback);
    }

    pub(super) fn set_on_always_on_top(&self, callback: Rc<dyn Fn(bool)>) {
        *self.on_always_on_top.borrow_mut() = Some(callback);
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

    pub(super) fn set_on_quit(&self, callback: Rc<dyn Fn()>) {
        *self.on_quit.borrow_mut() = Some(callback);
    }
}

/// Builds the four-section menu matching the compact player design:
///
/// 1. Restore Full Window (Ctrl+M)
/// 2. Pause / Next / Previous (Space, Ctrl+→, Ctrl+←)
/// 3. Always on Top (toggle)
/// 4. Preferences (Ctrl+,) / Quit (Ctrl+Q)
fn menu_model(playback_section: &gio::Menu, is_playing: bool) -> gio::Menu {
    let restore = gio::Menu::new();
    restore.append_item(&item_with_accel(
        &strings::text(strings::RESTORE_FULL_WINDOW),
        "compact.restore",
        "<Control>m",
    ));

    rebuild_playback_section(playback_section, is_playing);

    let window = gio::Menu::new();
    window.append(
        Some(&strings::text(strings::ALWAYS_ON_TOP)),
        Some("compact.always-on-top"),
    );

    let footer = gio::Menu::new();
    footer.append_item(&item_with_accel(
        &strings::text(strings::PREFERENCES),
        "compact.preferences",
        "<Control>comma",
    ));
    footer.append_item(&item_with_accel(
        &strings::text(strings::QUIT),
        "compact.quit",
        "<Control>q",
    ));

    let menu = gio::Menu::new();
    menu.append_section(None, &restore);
    menu.append_section(None, playback_section);
    menu.append_section(None, &window);
    menu.append_section(None, &footer);
    menu
}

fn rebuild_playback_section(section: &gio::Menu, is_playing: bool) {
    section.remove_all();
    let label = if is_playing {
        strings::text(strings::PAUSE)
    } else {
        strings::text(strings::PLAY)
    };
    section.append_item(&item_with_accel(&label, "compact.play-pause", "space"));
    section.append_item(&item_with_accel(
        &strings::text(strings::NEXT),
        "compact.next",
        "<Control>Right",
    ));
    section.append_item(&item_with_accel(
        &strings::text(strings::PREVIOUS),
        "compact.previous",
        "<Control>Left",
    ));
}

fn item_with_accel(label: &str, action: &str, accel: &str) -> gio::MenuItem {
    let item = gio::MenuItem::new(Some(label), Some(action));
    item.set_attribute_value("accel", Some(&accel.to_variant()));
    item
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
    fn menu_model_has_four_sections() {
        let playback = gio::Menu::new();
        let model = menu_model(&playback, false);
        let mut section_count = 0;
        for i in 0..model.n_items() {
            if model.item_link(i, "section").is_some() {
                section_count += 1;
            }
        }
        assert_eq!(section_count, 4);
    }

    #[test]
    fn playback_label_reflects_playing_state() {
        let section = gio::Menu::new();
        rebuild_playback_section(&section, true);
        let label = section
            .item_attribute_value(0, "label", Some(glib::VariantTy::STRING))
            .and_then(|v| v.get::<String>());
        assert_eq!(label.as_deref(), Some("Pause"));

        rebuild_playback_section(&section, false);
        let label = section
            .item_attribute_value(0, "label", Some(glib::VariantTy::STRING))
            .and_then(|v| v.get::<String>());
        assert_eq!(label.as_deref(), Some("Play"));
    }

    #[test]
    fn menu_has_only_the_supported_native_actions() {
        let mut actions = BTreeSet::new();
        let playback = gio::Menu::new();
        let model = menu_model(&playback, false);
        assert!(!collect_model_contract(model.upcast_ref(), &mut actions));
        assert_eq!(
            actions,
            [
                "compact.always-on-top".to_string(),
                "compact.next".to_string(),
                "compact.play-pause".to_string(),
                "compact.preferences".to_string(),
                "compact.previous".to_string(),
                "compact.quit".to_string(),
                "compact.restore".to_string(),
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
