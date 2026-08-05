//! Child controls for the local Sound Similarity plugin row.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::sound_preferences::{SoundSimilarityPreferences, SoundWeighting};

use super::{strings, PreferencesContext};

pub(super) struct SoundPreferenceRows {
    rows: Vec<gtk4::Widget>,
}

impl SoundPreferenceRows {
    pub(super) fn add_to(&self, parent: &adw::ExpanderRow) {
        for row in &self.rows {
            parent.add_row(row);
        }
    }

    pub(super) fn set_sensitive(&self, sensitive: bool) {
        for row in &self.rows {
            row.set_sensitive(sensitive);
        }
    }
}

fn save(
    context: &PreferencesContext,
    state: &Cell<SoundSimilarityPreferences>,
    update: impl FnOnce(&mut SoundSimilarityPreferences),
) {
    let mut preferences = state.get();
    update(&mut preferences);
    if let Err(error) = preferences.save(&context.conn) {
        tracing::warn!(%error, "could not save Sound Similarity preferences");
        return;
    }
    state.set(preferences);
    context.info_panel.refresh_sound_options();
}

pub(super) fn build(context: &Rc<PreferencesContext>, sensitive: bool) -> SoundPreferenceRows {
    let initial = SoundSimilarityPreferences::load(&context.conn).unwrap_or_default();
    let state = Rc::new(Cell::new(initial));

    let exclude_album = adw::SwitchRow::builder()
        .title(strings::text(strings::SOUND_EXCLUDE_SAME_ALBUM))
        .active(initial.exclude_same_album)
        .sensitive(sensitive)
        .build();
    {
        let context = Rc::downgrade(context);
        let state = state.clone();
        exclude_album.connect_active_notify(move |row| {
            if let Some(context) = context.upgrade() {
                save(&context, &state, |prefs| {
                    prefs.exclude_same_album = row.is_active();
                });
            }
        });
    }

    let exclude_artist = adw::SwitchRow::builder()
        .title(strings::text(strings::SOUND_EXCLUDE_SAME_ARTIST))
        .active(initial.exclude_same_artist)
        .sensitive(sensitive)
        .build();
    {
        let context = Rc::downgrade(context);
        let state = state.clone();
        exclude_artist.connect_active_notify(move |row| {
            if let Some(context) = context.upgrade() {
                save(&context, &state, |prefs| {
                    prefs.exclude_same_artist = row.is_active();
                });
            }
        });
    }

    let include_tempo = adw::SwitchRow::builder()
        .title(strings::text(strings::SOUND_INCLUDE_TEMPO))
        .subtitle(strings::text(strings::SOUND_TEMPO_WARNING))
        .active(initial.include_tempo)
        .sensitive(sensitive)
        .build();
    {
        let context = Rc::downgrade(context);
        let state = state.clone();
        include_tempo.connect_active_notify(move |row| {
            if let Some(context) = context.upgrade() {
                save(&context, &state, |prefs| {
                    prefs.include_tempo = row.is_active();
                });
            }
        });
    }

    let weighting = adw::ComboRow::builder()
        .title(strings::text(strings::SOUND_WEIGHTING))
        .model(&gtk4::StringList::new(&[
            &strings::text(strings::SOUND_WEIGHT_DEFAULT),
            &strings::text(strings::SOUND_WEIGHT_TIMBRE),
            &strings::text(strings::SOUND_WEIGHT_DYNAMICS),
        ]))
        .selected(match initial.weighting {
            SoundWeighting::Default => 0,
            SoundWeighting::Timbre => 1,
            SoundWeighting::Dynamics => 2,
        })
        .sensitive(sensitive)
        .build();
    {
        let context = Rc::downgrade(context);
        let state = state.clone();
        weighting.connect_selected_notify(move |row| {
            if let Some(context) = context.upgrade() {
                let weighting = match row.selected() {
                    1 => SoundWeighting::Timbre,
                    2 => SoundWeighting::Dynamics,
                    _ => SoundWeighting::Default,
                };
                save(&context, &state, |prefs| prefs.weighting = weighting);
            }
        });
    }

    let matches = adw::SpinRow::builder()
        .title(strings::text(strings::SOUND_NUMBER_OF_MATCHES))
        .adjustment(&gtk4::Adjustment::new(
            initial.match_count as f64,
            1.0,
            50.0,
            1.0,
            5.0,
            0.0,
        ))
        .digits(0)
        .sensitive(sensitive)
        .build();
    {
        let context = Rc::downgrade(context);
        let state = state.clone();
        matches.connect_value_notify(move |row| {
            if let Some(context) = context.upgrade() {
                save(&context, &state, |prefs| {
                    prefs.match_count = row.value() as usize;
                });
            }
        });
    }

    SoundPreferenceRows {
        rows: vec![
            exclude_album.upcast(),
            exclude_artist.upcast(),
            include_tempo.upcast(),
            weighting.upcast(),
            matches.upcast(),
        ],
    }
}
