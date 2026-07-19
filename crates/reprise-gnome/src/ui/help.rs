//! Native offline help for the application's implemented keyboard shortcuts.

use libadwaita as adw;
use libadwaita::prelude::*;

use super::strings;

pub(super) const HELP_ACCELERATOR: &str = "F1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ShortcutSpec {
    title_message: &'static str,
    accelerator: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SectionSpec {
    title_message: &'static str,
    shortcuts: &'static [ShortcutSpec],
}

const PLAYBACK_SHORTCUTS: &[ShortcutSpec] = &[
    ShortcutSpec {
        title_message: strings::PLAY_OR_PAUSE,
        accelerator: "space",
    },
    ShortcutSpec {
        title_message: strings::PLAY_SELECTED_TRACK,
        accelerator: "Return",
    },
    ShortcutSpec {
        title_message: strings::INCREASE_VOLUME,
        accelerator: "<Control>Up",
    },
    ShortcutSpec {
        title_message: strings::DECREASE_VOLUME,
        accelerator: "<Control>Down",
    },
];

const NAVIGATION_SHORTCUTS: &[ShortcutSpec] = &[
    ShortcutSpec {
        title_message: strings::SEARCH_LIBRARY,
        accelerator: "<Control>f",
    },
    ShortcutSpec {
        title_message: strings::JUMP_TO_NOW_PLAYING,
        accelerator: "<Control>l",
    },
    ShortcutSpec {
        title_message: strings::NAVIGATE_BACK,
        accelerator: "<Alt>Left",
    },
    ShortcutSpec {
        title_message: strings::NAVIGATE_FORWARD,
        accelerator: "<Alt>Right",
    },
    ShortcutSpec {
        title_message: strings::TOGGLE_COMPACT_VIEW,
        accelerator: "<Control>m",
    },
    ShortcutSpec {
        title_message: strings::CLEAR_SEARCH_OR_RETURN_TO_CONTENT,
        accelerator: "Escape",
    },
    ShortcutSpec {
        title_message: strings::OPEN_CONTEXT_MENU,
        accelerator: "<Shift>F10",
    },
    ShortcutSpec {
        title_message: strings::CONTEXT_MENU_MOVE_UP,
        accelerator: "<Alt>Up",
    },
    ShortcutSpec {
        title_message: strings::CONTEXT_MENU_MOVE_DOWN,
        accelerator: "<Alt>Down",
    },
    ShortcutSpec {
        title_message: strings::OPEN_KEYBOARD_SHORTCUTS,
        accelerator: "<Control>question",
    },
    ShortcutSpec {
        title_message: strings::OPEN_HELP,
        accelerator: HELP_ACCELERATOR,
    },
    ShortcutSpec {
        title_message: strings::OPEN_MAIN_MENU,
        accelerator: "F10",
    },
    ShortcutSpec {
        title_message: strings::CLOSE_WINDOW,
        accelerator: "<Control>w",
    },
    ShortcutSpec {
        title_message: strings::QUIT_REPRISE,
        accelerator: "<Control>q",
    },
];

fn shortcut_sections() -> [SectionSpec; 2] {
    [
        SectionSpec {
            title_message: strings::PREFERENCES_PLAYBACK,
            shortcuts: PLAYBACK_SHORTCUTS,
        },
        SectionSpec {
            title_message: strings::NAVIGATION,
            shortcuts: NAVIGATION_SHORTCUTS,
        },
    ]
}

fn build_sections() -> Vec<adw::ShortcutsSection> {
    shortcut_sections()
        .into_iter()
        .map(|spec| {
            let section = adw::ShortcutsSection::new(Some(&strings::text(spec.title_message)));
            for shortcut in spec.shortcuts {
                section.add(adw::ShortcutsItem::new(
                    &strings::text(shortcut.title_message),
                    shortcut.accelerator,
                ));
            }
            section
        })
        .collect()
}

fn build_dialog() -> adw::ShortcutsDialog {
    let dialog = adw::ShortcutsDialog::builder()
        .title(strings::text(strings::HELP))
        .build();
    for section in build_sections() {
        dialog.add(section);
    }
    dialog
}

pub(super) fn present(parent: &adw::ApplicationWindow) {
    let dialog = build_dialog();
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(parent);
    focus_guard.restore_on_dialog_close(dialog.upcast_ref());
    focus_guard.close_on_control_w(dialog.upcast_ref());
    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    use super::*;

    #[test]
    fn acc_9_help_matches_registered_standard_shortcuts() {
        let sections = shortcut_sections();
        let accelerators = sections
            .iter()
            .flat_map(|section| section.shortcuts)
            .map(|shortcut| shortcut.accelerator)
            .collect::<Vec<_>>();

        assert_eq!(sections.len(), 2);
        assert_eq!(
            sections[1].shortcuts[5],
            ShortcutSpec {
                title_message: strings::CLEAR_SEARCH_OR_RETURN_TO_CONTENT,
                accelerator: "Escape",
            }
        );
        assert_eq!(
            accelerators,
            [
                "space",
                "Return",
                "<Control>Up",
                "<Control>Down",
                "<Control>f",
                "<Control>l",
                "<Alt>Left",
                "<Alt>Right",
                "<Control>m",
                "Escape",
                "<Shift>F10",
                "<Alt>Up",
                "<Alt>Down",
                "<Control>question",
                "F1",
                "F10",
                "<Control>w",
                "<Control>q",
            ]
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn dialog_exposes_translated_sections_and_shortcuts() {
        gtk4::init().expect("GTK display must be available");
        let dialog = build_dialog();
        let sections = build_sections();

        assert_eq!(dialog.title(), "Help");
        assert_eq!(sections[0].title().as_deref(), Some("Playback"));
        assert_eq!(sections[0].n_items(), 4);
        assert_eq!(sections[1].title().as_deref(), Some("Navigation"));
        assert_eq!(sections[1].n_items(), 14);

        let items = sections
            .iter()
            .flat_map(|section| (0..section.n_items()).map(move |index| (section, index)))
            .map(|(section, index)| {
                let item = section
                    .item(index)
                    .expect("shortcut item must exist")
                    .downcast::<adw::ShortcutsItem>()
                    .expect("section must contain shortcut items");
                (item.title().to_string(), item.accelerator().to_string())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            items,
            [
                ("Play or Pause".to_string(), "space".to_string()),
                ("Play Selected Track".to_string(), "Return".to_string()),
                ("Increase Volume".to_string(), "<Control>Up".to_string()),
                ("Decrease Volume".to_string(), "<Control>Down".to_string()),
                ("Search Library".to_string(), "<Control>f".to_string()),
                ("Jump to now playing".to_string(), "<Control>l".to_string(),),
                ("Back to previous view".to_string(), "<Alt>Left".to_string()),
                ("Forward to next view".to_string(), "<Alt>Right".to_string()),
                ("Toggle Compact View".to_string(), "<Control>m".to_string(),),
                (
                    "Clear Search or Return to Content".to_string(),
                    "Escape".to_string(),
                ),
                ("Open Context Menu".to_string(), "<Shift>F10".to_string()),
                ("Move up".to_string(), "<Alt>Up".to_string()),
                ("Move down".to_string(), "<Alt>Down".to_string()),
                (
                    "Open Keyboard Shortcuts".to_string(),
                    "<Control>question".to_string(),
                ),
                ("Open Help".to_string(), HELP_ACCELERATOR.to_string()),
                ("Open Main Menu".to_string(), "F10".to_string()),
                ("Close Window".to_string(), "<Control>w".to_string()),
                ("Quit Reprise".to_string(), "<Control>q".to_string()),
            ]
        );
    }
}
