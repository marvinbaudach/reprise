use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::settings::{self, ColorScheme};

use super::preference_choice_cards::{self, ChoiceCardSpec};
use super::preference_visual_strings as visual_strings;
use super::preferences::{apply_color_scheme, PreferencesContext};
use super::strings;

fn color_scheme_from_index(index: u32) -> ColorScheme {
    match index {
        1 => ColorScheme::Light,
        2 => ColorScheme::Dark,
        _ => ColorScheme::System,
    }
}

fn color_scheme_index(value: ColorScheme) -> u32 {
    match value {
        ColorScheme::System => 0,
        ColorScheme::Light => 1,
        ColorScheme::Dark => 2,
    }
}

fn preview(classes: [&str; 2]) -> gtk4::Box {
    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    root.add_css_class("reprise-choice-preview");
    root.set_height_request(72);
    root.set_overflow(gtk4::Overflow::Hidden);
    for class in classes {
        let panel = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        panel.set_hexpand(true);
        panel.add_css_class(class);
        root.append(&panel);
    }
    root
}

pub(super) fn build(context: &Rc<PreferencesContext>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(strings::text(strings::PREFERENCES_APPEARANCE))
        .icon_name("applications-graphics-symbolic")
        .build();
    let group = adw::PreferencesGroup::builder()
        .title(strings::text(strings::COLOR_SCHEME))
        .build();
    let selected = {
        let conn = context.conn.borrow();
        color_scheme_index(settings::get_color_scheme(&conn))
    };
    let on_selected: Rc<dyn Fn(u32) -> bool> = {
        let weak = Rc::downgrade(context);
        Rc::new(move |index| {
            let Some(context) = weak.upgrade() else {
                return false;
            };
            let value = color_scheme_from_index(index);
            let saved = {
                let conn = context.conn.borrow();
                settings::set_color_scheme(&conn, value)
            };
            match saved {
                Ok(()) => {
                    apply_color_scheme(value);
                    true
                }
                Err(error) => {
                    tracing::warn!(%error, "could not save color scheme");
                    context.track_list.toast(&visual_strings::text(
                        visual_strings::COLOR_SCHEME_SAVE_FAILED,
                    ));
                    false
                }
            }
        })
    };
    let cards = preference_choice_cards::build(
        vec![
            ChoiceCardSpec::new(
                strings::text(strings::COLOR_SYSTEM),
                &preview(["reprise-preview-light", "reprise-preview-dark"]),
            ),
            ChoiceCardSpec::new(
                strings::text(strings::COLOR_LIGHT),
                &preview(["reprise-preview-light", "reprise-preview-light-alt"]),
            ),
            ChoiceCardSpec::new(
                strings::text(strings::COLOR_DARK),
                &preview(["reprise-preview-dark", "reprise-preview-dark-alt"]),
            ),
        ],
        selected,
        &on_selected,
    );
    group.add(&cards.root);
    page.add(&group);
    let decorations = adw::PreferencesGroup::builder()
        .title(super::window_decoration_strings::text(
            super::window_decoration_strings::WINDOW_DECORATIONS,
        ))
        .build();
    decorations.add(&super::preference_window_decorations::row(context));
    page.add(&decorations);
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_scheme_cards_round_trip_every_typed_value() {
        for (index, value) in [ColorScheme::System, ColorScheme::Light, ColorScheme::Dark]
            .into_iter()
            .enumerate()
        {
            assert_eq!(color_scheme_index(value), index as u32);
            assert_eq!(color_scheme_from_index(index as u32), value);
        }
    }
}
