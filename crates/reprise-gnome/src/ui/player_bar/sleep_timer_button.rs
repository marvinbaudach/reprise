//! Sleep-timer button, menu, tooltip, and checked-state presentation.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;

use super::PlayerBar;
use crate::ui::style::buttons;

const ICON_PREFERRED: &str = "weather-clear-night-symbolic";
const ICON_FALLBACK: &str = "alarm-symbolic";
const ACTION_GROUP: &str = "sleep-timer";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum SleepTimerChoice {
    Minutes(u32),
    EndOfTrack,
    Cancel,
}

type ChoiceCallback = Rc<dyn Fn(SleepTimerChoice)>;

pub(in crate::ui) struct SleepTimerButton {
    button: gtk4::ToggleButton,
    popover: gtk4::PopoverMenu,
    end_action: gtk4::gio::SimpleAction,
    cancel_menu: gtk4::gio::Menu,
    callbacks: Rc<RefCell<Vec<ChoiceCallback>>>,
    armed: Rc<Cell<bool>>,
}

impl SleepTimerButton {
    pub(in crate::ui) fn new() -> Self {
        let label = crate::i18n::gettext("Sleep Timer");
        let button = gtk4::ToggleButton::builder()
            .icon_name(available_icon_name())
            .tooltip_text(&label)
            .css_classes(["flat"])
            .build();
        button.update_property(&[gtk4::accessible::Property::Label(&label)]);
        buttons::arm(&button, buttons::TOGGLE_CLASS);

        let actions = gtk4::gio::SimpleActionGroup::new();
        let callbacks = Rc::new(RefCell::new(Vec::<ChoiceCallback>::new()));
        for (name, choice) in [
            ("minutes-15", SleepTimerChoice::Minutes(15)),
            ("minutes-30", SleepTimerChoice::Minutes(30)),
            ("minutes-45", SleepTimerChoice::Minutes(45)),
            ("minutes-60", SleepTimerChoice::Minutes(60)),
        ] {
            actions.add_action(&choice_action(name, choice, &callbacks));
        }
        let end_action = choice_action("end-of-track", SleepTimerChoice::EndOfTrack, &callbacks);
        actions.add_action(&end_action);
        actions.add_action(&choice_action(
            "cancel",
            SleepTimerChoice::Cancel,
            &callbacks,
        ));
        button.insert_action_group(ACTION_GROUP, Some(&actions));

        let choices = gtk4::gio::Menu::new();
        for minutes in [15, 30, 45, 60] {
            choices.append(
                Some(&crate::i18n::format_message(
                    &crate::i18n::gettext("{minutes} minutes"),
                    &[("minutes", &minutes.to_string())],
                )),
                Some(&format!("{ACTION_GROUP}.minutes-{minutes}")),
            );
        }
        choices.append(
            Some(&crate::i18n::gettext("End of Track")),
            Some(&format!("{ACTION_GROUP}.end-of-track")),
        );
        let cancel_menu = gtk4::gio::Menu::new();
        let model = gtk4::gio::Menu::new();
        model.append_section(None, &choices);
        model.append_section(None, &cancel_menu);
        let popover = gtk4::PopoverMenu::from_model(Some(&model));
        popover.set_parent(&button);
        popover.set_has_arrow(false);

        let armed = Rc::new(Cell::new(false));
        button.connect_clicked({
            let popover = popover.clone();
            let armed = armed.clone();
            move |button| {
                if popover.is_visible() {
                    popover.popdown();
                } else {
                    popover.popup();
                }
                button.set_active(armed.get() || popover.is_visible());
            }
        });
        popover.connect_closed({
            let button = button.clone();
            let armed = armed.clone();
            move |_| button.set_active(armed.get())
        });
        button.connect_destroy({
            let popover = popover.clone();
            move |_| popover.unparent()
        });

        Self {
            button,
            popover,
            end_action,
            cancel_menu,
            callbacks,
            armed,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::ToggleButton {
        &self.button
    }

    fn connect_choice(&self, callback: impl Fn(SleepTimerChoice) + 'static) {
        self.callbacks.borrow_mut().push(Rc::new(callback));
    }

    fn set_presentation(&self, armed: bool, tooltip: &str, end_of_track_enabled: bool) {
        self.armed.set(armed);
        self.button.set_active(armed || self.popover.is_visible());
        self.button.set_tooltip_text(Some(tooltip));
        self.end_action.set_enabled(end_of_track_enabled);
        self.cancel_menu.remove_all();
        if armed {
            self.cancel_menu.append(
                Some(&crate::i18n::gettext("Cancel Sleep Timer")),
                Some(&format!("{ACTION_GROUP}.cancel")),
            );
        }
    }
}

fn available_icon_name() -> &'static str {
    let Some(display) = gtk4::gdk::Display::default() else {
        return ICON_FALLBACK;
    };
    if gtk4::IconTheme::for_display(&display).has_icon(ICON_PREFERRED) {
        ICON_PREFERRED
    } else {
        ICON_FALLBACK
    }
}

fn choice_action(
    name: &str,
    choice: SleepTimerChoice,
    callbacks: &Rc<RefCell<Vec<ChoiceCallback>>>,
) -> gtk4::gio::SimpleAction {
    let action = gtk4::gio::SimpleAction::new(name, None);
    action.connect_activate({
        let callbacks = callbacks.clone();
        move |_, _| {
            let callbacks = callbacks.borrow().clone();
            for callback in callbacks {
                callback(choice);
            }
        }
    });
    action
}

impl PlayerBar {
    pub(in crate::ui) fn connect_sleep_timer_choice(
        &self,
        callback: impl Fn(SleepTimerChoice) + 'static,
    ) {
        self.sleep_timer.connect_choice(callback);
    }

    pub(in crate::ui) fn set_sleep_timer_presentation(
        &self,
        armed: bool,
        tooltip: &str,
        end_of_track_enabled: bool,
    ) {
        self.sleep_timer
            .set_presentation(armed, tooltip, end_of_track_enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn play_sleep_1_moon_button_tracks_armed_and_cancelled_state() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().expect("GTK must initialize under the display runner");
        let control = SleepTimerButton::new();

        control.set_presentation(true, "Pauses in 15 min", true);
        assert!(control.widget().is_active());
        assert_eq!(
            control.widget().tooltip_text().as_deref(),
            Some("Pauses in 15 min")
        );
        assert_eq!(control.cancel_menu.n_items(), 1);

        control.set_presentation(false, "Sleep Timer", true);
        assert!(!control.widget().is_active());
        assert_eq!(control.cancel_menu.n_items(), 0);
    }
}
