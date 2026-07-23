use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::strings;
use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;

const ACTION_RESTORE: &str = "restore";
const ACTION_PLAY_PAUSE: &str = "play-pause";
const ACTION_NEXT: &str = "next";
const ACTION_PREVIOUS: &str = "previous";
const ACTION_ALWAYS_ON_TOP: &str = "always-on-top";
const ACTION_PREFERENCES: &str = "preferences";
const ACTION_QUIT: &str = "quit";

/// Every action name registered in the compact action group.
pub(in crate::ui) const MENU_ACTIONS: [&str; 7] = [
    ACTION_RESTORE,
    ACTION_PLAY_PAUSE,
    ACTION_NEXT,
    ACTION_PREVIOUS,
    ACTION_ALWAYS_ON_TOP,
    ACTION_PREFERENCES,
    ACTION_QUIT,
];

type VoidCallback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;
type BoolCallback = Rc<RefCell<Option<Rc<dyn Fn(bool)>>>>;

pub(in crate::ui) struct CompactMenu {
    pub(in crate::ui) popover: gtk4::PopoverMenu,
    pub(in crate::ui) action_group: gio::SimpleActionGroup,
    pub(in crate::ui) always_on_top_action: gio::SimpleAction,
    playback_section: gio::Menu,
    on_restore: VoidCallback,
    on_play_pause: VoidCallback,
    on_next: VoidCallback,
    on_previous: VoidCallback,
    on_always_on_top: BoolCallback,
    on_preferences: VoidCallback,
    on_quit: VoidCallback,
    is_playing: RefCell<bool>,
    always_on_top_available: Cell<bool>,
}

impl CompactMenu {
    pub(in crate::ui) fn build() -> Self {
        let on_restore = empty_callback();
        let on_play_pause = empty_callback();
        let on_next = empty_callback();
        let on_previous = empty_callback();
        let on_always_on_top: BoolCallback = Rc::new(RefCell::new(None));
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

        debug_assert!(MENU_ACTIONS
            .iter()
            .all(|action| action_group.has_action(action)));

        let playback_section = gio::Menu::new();
        let menu_model = menu_model(&playback_section, false, true);
        let popover = gtk4::PopoverMenu::from_model(Some(&menu_model));
        popover.set_has_arrow(false);

        Self {
            popover,
            action_group,
            always_on_top_action,
            playback_section,
            on_restore,
            on_play_pause,
            on_next,
            on_previous,
            on_always_on_top,
            on_preferences,
            on_quit,
            is_playing: RefCell::new(false),
            always_on_top_available: Cell::new(true),
        }
    }

    pub(in crate::ui) fn set_playing(&self, playing: bool) {
        let mut current = self.is_playing.borrow_mut();
        if *current == playing {
            return;
        }
        *current = playing;
        rebuild_playback_section(&self.playback_section, playing);
    }

    /// Shows or hides the "Always on Top" section (MINI-3). Rebuilds the
    /// popover model, reusing the same playback section so the play/pause
    /// label stays in sync.
    pub(in crate::ui) fn set_always_on_top_available(&self, available: bool) {
        if self.always_on_top_available.get() == available {
            return;
        }
        self.always_on_top_available.set(available);
        let model = menu_model(&self.playback_section, *self.is_playing.borrow(), available);
        self.popover.set_menu_model(Some(&model));
    }

    pub(in crate::ui) fn set_on_restore(&self, callback: Rc<dyn Fn()>) {
        *self.on_restore.borrow_mut() = Some(callback);
    }

    pub(in crate::ui) fn set_on_play_pause(&self, callback: Rc<dyn Fn()>) {
        *self.on_play_pause.borrow_mut() = Some(callback);
    }

    pub(in crate::ui) fn set_on_next(&self, callback: Rc<dyn Fn()>) {
        *self.on_next.borrow_mut() = Some(callback);
    }

    pub(in crate::ui) fn set_on_previous(&self, callback: Rc<dyn Fn()>) {
        *self.on_previous.borrow_mut() = Some(callback);
    }

    pub(in crate::ui) fn set_on_always_on_top(&self, callback: Rc<dyn Fn(bool)>) {
        *self.on_always_on_top.borrow_mut() = Some(callback);
    }

    pub(in crate::ui) fn set_on_preferences(&self, callback: Rc<dyn Fn()>) {
        *self.on_preferences.borrow_mut() = Some(callback);
    }

    pub(in crate::ui) fn set_on_quit(&self, callback: Rc<dyn Fn()>) {
        *self.on_quit.borrow_mut() = Some(callback);
    }
}

/// Builds the four-section menu matching the compact player design:
///
/// 1. Restore Full Window (Ctrl+M)
/// 2. Pause / Next / Previous (Space, Ctrl+→, Ctrl+←)
/// 3. Always on Top (toggle)
/// 4. Preferences (Ctrl+,) / Quit (Ctrl+Q)
fn menu_model(
    playback_section: &gio::Menu,
    is_playing: bool,
    always_on_top_available: bool,
) -> gio::Menu {
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
    if always_on_top_available {
        menu.append_section(None, &window);
    }
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

#[cfg(test)]
pub(in crate::ui) const fn accepts_context_menu(interactive_descendant: bool) -> bool {
    !interactive_descendant
}

pub(in crate::ui) fn popup_at(
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn mini_3_context_menu_skips_interactive_regions() {
        assert!(accepts_context_menu(false));
        assert!(!accepts_context_menu(true));
    }

    #[test]
    fn mini_3_context_menu_has_four_sections() {
        let playback = gio::Menu::new();
        let model = menu_model(&playback, false, true);
        let mut section_count = 0;
        for i in 0..model.n_items() {
            if model.item_link(i, "section").is_some() {
                section_count += 1;
            }
        }
        assert_eq!(section_count, 4);
    }

    #[test]
    fn mini_3_context_menu_playback_label_reflects_state() {
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
    fn mini_3_context_menu_actions_match_contract() {
        let mut actions = BTreeSet::new();
        let playback = gio::Menu::new();
        let model = menu_model(&playback, false, true);
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

    #[test]
    fn mini_3_context_menu_hides_always_on_top_on_wayland() {
        let playback = gio::Menu::new();
        let with = menu_model(&playback, false, true);
        let without = menu_model(&playback, false, false);

        let count_sections = |model: &gio::Menu| {
            (0..model.n_items())
                .filter(|i| model.item_link(*i, "section").is_some())
                .count()
        };
        assert_eq!(count_sections(&with), 4);
        assert_eq!(count_sections(&without), 3);

        let mut actions = BTreeSet::new();
        collect_model_contract(without.upcast_ref(), &mut actions);
        assert!(!actions.contains("compact.always-on-top"));
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
