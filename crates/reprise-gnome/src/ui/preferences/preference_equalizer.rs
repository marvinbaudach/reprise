use std::cell::Cell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::equalizer::EqualizerPreset;

use super::preference_playback::build_equalizer_surface;
use super::strings;
use super::surface::preset_label;

pub(super) struct EqualizerControls {
    pub(super) group: adw::PreferencesGroup,
    pub(super) enabled: adw::SwitchRow,
    pub(super) root: adw::ExpanderRow,
    #[cfg(test)]
    pub(super) preset_row: adw::ActionRow,
    #[cfg(test)]
    pub(super) preset_button: gtk4::MenuButton,
    #[cfg(test)]
    pub(super) scales: Vec<gtk4::Scale>,
}

pub(super) fn build_equalizer_controls(
    stored_bands: [f64; 10],
    enabled: bool,
    on_enabled: Rc<dyn Fn(bool)>,
    on_preset: &Rc<dyn Fn([f64; 10]) -> bool>,
    on_band: Rc<dyn Fn(usize, f64)>,
) -> EqualizerControls {
    let group = adw::PreferencesGroup::builder()
        .title(strings::text(strings::EQUALIZER))
        .build();
    let enabled_row = adw::SwitchRow::builder()
        .title(strings::text(strings::ENABLE_EQUALIZER))
        .active(enabled)
        .build();
    group.add(&enabled_row);

    let preset_menu = gio::Menu::new();
    for (index, preset) in EqualizerPreset::ALL.into_iter().enumerate() {
        preset_menu.append(
            Some(&strings::text(preset_label(preset))),
            Some(&format!("equalizer.select-{index}")),
        );
    }
    let selected_label = EqualizerPreset::ALL
        .iter()
        .position(|preset| preset.ten_band_levels() == stored_bands)
        .map_or(strings::PRESET_CUSTOM, |index| {
            preset_label(EqualizerPreset::ALL[index])
        });
    let preset_button = gtk4::MenuButton::builder()
        .label(strings::text(selected_label))
        .menu_model(&preset_menu)
        .build();
    let preset_row = adw::ActionRow::builder()
        .title(strings::text(strings::EQUALIZER_PRESET))
        .build();
    preset_row.add_suffix(&preset_button);
    preset_row.set_activatable_widget(Some(&preset_button));
    group.add(&preset_row);

    let updating = Rc::new(Cell::new(false));
    let preset_for_band = preset_button.clone();
    let updating_for_band = updating.clone();
    let on_band_changed: Rc<dyn Fn(usize, f64)> = Rc::new(move |index, value| {
        if updating_for_band.get() {
            return;
        }
        preset_for_band.set_label(&strings::text(strings::PRESET_CUSTOM));
        on_band(index, value);
    });
    let surface = build_equalizer_surface(stored_bands, enabled, &on_band_changed);
    let scales = surface.scales.clone();
    let bands = adw::ExpanderRow::builder()
        .title(strings::text(strings::EQUALIZER_MANUAL))
        .expanded(false)
        .build();
    bands.set_sensitive(enabled);
    bands.add_row(&surface.root);
    group.add(&bands);

    let bands_for_enabled = bands.clone();
    enabled_row.connect_active_notify(move |row| {
        bands_for_enabled.set_sensitive(row.is_active());
        on_enabled(row.is_active());
    });

    let preset_actions = gio::SimpleActionGroup::new();
    for (index, preset) in EqualizerPreset::ALL.into_iter().enumerate() {
        let action = gio::SimpleAction::new(&format!("select-{index}"), None);
        let updating = updating.clone();
        let scales = scales.clone();
        let on_preset = on_preset.clone();
        let preset_button = preset_button.clone();
        action.connect_activate(move |_, _| {
            let bands = preset.ten_band_levels();
            if !on_preset(bands) {
                return;
            }
            updating.set(true);
            for (scale, value) in scales.iter().zip(bands) {
                scale.set_value(value);
            }
            preset_button.set_label(&strings::text(preset_label(preset)));
            updating.set(false);
        });
        preset_actions.add_action(&action);
    }
    preset_row.insert_action_group("equalizer", Some(&preset_actions));

    EqualizerControls {
        group,
        enabled: enabled_row,
        root: bands,
        #[cfg(test)]
        preset_row,
        #[cfg(test)]
        preset_button,
        #[cfg(test)]
        scales,
    }
}
