use std::rc::Rc;

use libadwaita as adw;
use libadwaita::prelude::*;

use reprise_core::library::settings::SeekColouring;

use super::strings;
use super::PreferencesContext;
use crate::ui::style::{
    self,
    accent::{AccentSource, ACCENT_SOURCE_SETTING_KEY},
    theme::Theme,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppearanceSection {
    Theme,
    AccentColor,
    ColorScheme,
    SeekBar,
    WindowDecorations,
}

fn appearance_sections() -> [AppearanceSection; 5] {
    [
        AppearanceSection::Theme,
        AppearanceSection::AccentColor,
        AppearanceSection::ColorScheme,
        // Between the colours it depends on and the window frame it does not:
        // the seek bar's colouring is a colour decision, not a chrome one.
        AppearanceSection::SeekBar,
        AppearanceSection::WindowDecorations,
    ]
}

/// The two colourings, in the order they are offered.
const SEEK_COLORINGS: [SeekColouring; 2] = [SeekColouring::Frequency, SeekColouring::Solid];

pub(in crate::ui) fn build(context: &Rc<PreferencesContext>) -> adw::PreferencesPage {
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
            AppearanceSection::AccentColor => {
                let group = adw::PreferencesGroup::builder()
                    .title(strings::text(strings::ACCENT_COLOR))
                    .build();
                group.add(&accent_row(context));
                page.add(&group);
            }
            AppearanceSection::ColorScheme => {
                let group = adw::PreferencesGroup::builder()
                    .title(strings::text(strings::COLOR_SCHEME))
                    .build();
                group.add(&color_scheme_row(context));
                page.add(&group);
            }
            AppearanceSection::SeekBar => {
                let group = adw::PreferencesGroup::builder()
                    .title(strings::text(strings::SEEK_BAR))
                    .build();
                group.add(&seek_coloring_row(context));
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
        &context.conn,
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
                &context.conn,
                style::theme::THEME_SETTING_KEY,
                theme.id(),
            ) {
                tracing::warn!(%error, "could not persist the selected theme");
            }
        }
    });

    row
}

/// A `ComboRow` that live-switches and persists the accent source.
fn accent_row(context: &Rc<PreferencesContext>) -> adw::ComboRow {
    let sources = [AccentSource::App, AccentSource::System];
    let names = [
        strings::text(strings::ACCENT_SOURCE_APP),
        strings::text(strings::SCHEME_SYSTEM),
    ];
    let model = gtk4::StringList::new(
        &names
            .iter()
            .map(std::string::String::as_str)
            .collect::<Vec<_>>(),
    );
    let row = adw::ComboRow::builder()
        .title(strings::text(strings::ACCENT_COLOR))
        .subtitle(strings::text(strings::ACCENT_COLOR_SUBTITLE))
        .model(&model)
        .build();

    let stored =
        reprise_core::library::settings::get_setting(&context.conn, ACCENT_SOURCE_SETTING_KEY)
            .ok()
            .flatten();
    let current = stored
        .as_deref()
        .map_or(AccentSource::DEFAULT, AccentSource::from_id);
    let index = sources
        .iter()
        .position(|source| *source == current)
        .unwrap_or_default();
    row.set_selected(index as u32);

    row.connect_selected_notify({
        let context = context.clone();
        move |row| {
            let Some(source) = sources.get(row.selected() as usize).copied() else {
                return;
            };
            style::set_accent_source(source);
            if let Err(error) = reprise_core::library::settings::set_setting(
                &context.conn,
                ACCENT_SOURCE_SETTING_KEY,
                source.id(),
            ) {
                tracing::warn!(%error, "could not persist the selected accent source");
            }
        }
    });

    row
}

/// A `ComboRow` that switches the seek bar between its two colourings and
/// applies the choice to the running bar at once — the bar is on screen behind
/// the dialog, so a change nobody can see until the next restart would be a
/// setting that answers late.
fn seek_coloring_row(context: &Rc<PreferencesContext>) -> adw::ComboRow {
    let names = [
        strings::text(strings::SEEK_COLORING_FREQUENCY),
        strings::text(strings::SEEK_COLORING_SOLID),
    ];
    let model = gtk4::StringList::new(
        &names
            .iter()
            .map(std::string::String::as_str)
            .collect::<Vec<_>>(),
    );
    let row = adw::ComboRow::builder()
        .title(strings::text(strings::SEEK_COLORING))
        .subtitle(strings::text(strings::SEEK_COLORING_SUBTITLE))
        .model(&model)
        .build();

    let current = reprise_core::library::settings::get_seek_colouring(&context.conn);
    let index = SEEK_COLORINGS
        .iter()
        .position(|colouring| *colouring == current)
        .unwrap_or_default();
    row.set_selected(index as u32);

    row.connect_selected_notify({
        let context = context.clone();
        move |row| {
            let Some(colouring) = SEEK_COLORINGS.get(row.selected() as usize).copied() else {
                return;
            };
            if let Some(player) = context.player.as_ref() {
                player.bar.set_seek_colouring(colouring);
            }
            if let Err(error) =
                reprise_core::library::settings::set_seek_colouring(&context.conn, colouring)
            {
                tracing::warn!(%error, "could not persist the seek bar colouring");
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
    let model = gtk4::StringList::new(
        &schemes
            .iter()
            .map(std::string::String::as_str)
            .collect::<Vec<_>>(),
    );
    let row = adw::ComboRow::builder()
        .title(strings::text(strings::COLOR_SCHEME))
        .subtitle(strings::text(strings::COLOR_SCHEME_SUBTITLE))
        .model(&model)
        .build();

    let stored = reprise_core::library::settings::get_color_scheme(&context.conn);
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
            if let Err(error) =
                reprise_core::library::settings::set_color_scheme(&context.conn, scheme)
            {
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
    fn style_8_appearance_places_accent_between_theme_and_color_scheme() {
        assert_eq!(
            appearance_sections(),
            [
                AppearanceSection::Theme,
                AppearanceSection::AccentColor,
                AppearanceSection::ColorScheme,
                AppearanceSection::SeekBar,
                AppearanceSection::WindowDecorations,
            ]
        );
    }

    #[test]
    fn the_seek_bar_offers_two_colourings_and_neither_is_an_off_switch() {
        // The quiet variant is a second colouring and a legitimate taste. It is
        // named for what it does, and the default is the one that carries
        // information.
        assert_eq!(
            SEEK_COLORINGS,
            [SeekColouring::Frequency, SeekColouring::Solid]
        );
        assert_eq!(SEEK_COLORINGS[0], SeekColouring::DEFAULT);
        let solid = strings::SEEK_COLORING_SOLID.to_lowercase();
        assert!(!solid.contains("off") && !solid.contains("none"));
    }

    #[test]
    fn accent_source_persistence_round_trips_and_unknown_defaults_to_app() {
        for source in [AccentSource::App, AccentSource::System] {
            assert_eq!(AccentSource::from_id(source.id()), source);
        }
        assert_eq!(
            AccentSource::from_id("future-source"),
            AccentSource::DEFAULT
        );
    }
}
