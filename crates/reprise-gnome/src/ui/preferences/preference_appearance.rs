use std::rc::Rc;

use libadwaita as adw;
use libadwaita::prelude::*;

use super::preferences::PreferencesContext;
use super::strings;
use crate::ui::style::{self, theme::Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppearanceSection {
    Theme,
    WindowDecorations,
}

fn appearance_sections() -> [AppearanceSection; 2] {
    [
        AppearanceSection::Theme,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appearance_page_lists_theme_then_window_decorations() {
        assert_eq!(
            appearance_sections(),
            [
                AppearanceSection::Theme,
                AppearanceSection::WindowDecorations
            ]
        );
    }
}
