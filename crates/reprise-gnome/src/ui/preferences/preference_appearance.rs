use std::rc::Rc;

use libadwaita as adw;
use libadwaita::prelude::*;

use super::preferences::PreferencesContext;
use super::strings;
use crate::ui::style::{self, theme::Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppearanceSection {
    Theme,
    ColorScheme,
    WindowDecorations,
}

fn appearance_sections() -> [AppearanceSection; 3] {
    [
        AppearanceSection::Theme,
        AppearanceSection::ColorScheme,
        AppearanceSection::WindowDecorations,
    ]
}

pub(super) fn build(context: &Rc<PreferencesContext>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(strings::text(strings::PREFERENCES_APPEARANCE))
        .icon_name("applications-graphics-symbolic")
        .build();
    for section in appearance_sections() {
        match section {
            AppearanceSection::Theme => {
                let group = adw::PreferencesGroup::builder().title("Theme").build();
                group.add(&theme_row(context));
                page.add(&group);
            }
            AppearanceSection::ColorScheme => {
                let group = adw::PreferencesGroup::builder()
                    .title(strings::text(strings::COLOR_SCHEME))
                    .build();
                group.add(&color_scheme_row(context));
                page.add(&group);
            }
            AppearanceSection::WindowDecorations => {
                let decorations = adw::PreferencesGroup::builder()
                    .title(super::window_decoration_strings::text(
                        super::window_decoration_strings::WINDOW_DECORATIONS,
                    ))
                    .build();
                decorations.add(&super::preference_window_decorations::row(context));
                page.add(&decorations);
            }
        }
    }
    page
}

/// A `ComboRow` that live-switches and persists the named dark theme.
fn theme_row(context: &Rc<PreferencesContext>) -> adw::ComboRow {
    let themes = Theme::all();
    let names: Vec<&str> = themes.iter().map(|theme| theme.display_name()).collect();
    let model = gtk4::StringList::new(&names);
    let row = adw::ComboRow::builder()
        .title("Theme")
        .subtitle("Named dark palette used across the app")
        .model(&model)
        .build();

    let stored = reprise_core::library::settings::get_setting(
        &context.conn.borrow(),
        style::theme::THEME_SETTING_KEY,
    )
    .ok()
    .flatten();
    let current = stored
        .as_deref()
        .and_then(Theme::from_id)
        .unwrap_or(Theme::DEFAULT);
    if let Some(index) = themes.iter().position(|theme| *theme == current) {
        row.set_selected(index as u32);
    }

    row.connect_selected_notify({
        let context = context.clone();
        move |row| {
            let Some(theme) = Theme::all().get(row.selected() as usize).copied() else {
                return;
            };
            style::set_theme(theme);
            if let Err(error) = reprise_core::library::settings::set_setting(
                &context.conn.borrow(),
                style::theme::THEME_SETTING_KEY,
                theme.id(),
            ) {
                tracing::warn!(%error, "could not persist the selected theme");
            }
        }
    });

    row
}

/// A `ComboRow` that switches between System / Dark / Light color schemes.
fn color_scheme_row(context: &Rc<PreferencesContext>) -> adw::ComboRow {
    let schemes = [
        strings::text(strings::SCHEME_SYSTEM),
        strings::text(strings::SCHEME_DARK),
        strings::text(strings::SCHEME_LIGHT),
    ];
    let model = gtk4::StringList::new(&schemes.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    let row = adw::ComboRow::builder()
        .title(strings::text(strings::COLOR_SCHEME))
        .subtitle(strings::text(strings::COLOR_SCHEME_SUBTITLE))
        .model(&model)
        .build();

    let stored = reprise_core::library::settings::get_color_scheme(&context.conn.borrow());
    let index = match stored {
        "dark" => 1u32,
        "light" => 2,
        _ => 0,
    };
    row.set_selected(index);

    row.connect_selected_notify({
        let context = context.clone();
        move |row| {
            let scheme = match row.selected() {
                1 => "dark",
                2 => "light",
                _ => "system",
            };
            style::set_color_scheme(scheme);
            if let Err(error) = reprise_core::library::settings::set_color_scheme(
                &context.conn.borrow(),
                scheme,
            ) {
                tracing::warn!(%error, "could not persist color scheme");
            }
        }
    });

    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appearance_page_lists_theme_color_scheme_then_window_decorations() {
        assert_eq!(
            appearance_sections(),
            [
                AppearanceSection::Theme,
                AppearanceSection::ColorScheme,
                AppearanceSection::WindowDecorations,
            ]
        );
    }
}
