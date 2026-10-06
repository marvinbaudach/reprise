use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::settings::{self, ReplayGainMode};

use crate::ui::preferences::preference_equalizer::build_equalizer_controls;
use crate::ui::strings;

use super::surface::{replay_gain_index, PreferencesContext};

fn replay_gain_from_index(index: u32) -> ReplayGainMode {
    match index {
        1 => ReplayGainMode::Track,
        2 => ReplayGainMode::Album,
        _ => ReplayGainMode::Off,
    }
}

/// Formats the crossfade slider's live value readout: `0` reads as "Off",
/// otherwise a whole-second overlap ("4.0 s").
fn crossfade_value_label(seconds: u8) -> String {
    if seconds == 0 {
        strings::text(strings::CROSSFADE_OFF)
    } else {
        format!("{:.1} s", f64::from(seconds))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct GaplessControlState {
    pub(super) sensitive: bool,
    pub(super) subtitle: &'static str,
}

pub(super) fn gapless_control_state(crossfade_seconds: u8) -> GaplessControlState {
    if crossfade_seconds == 0 {
        GaplessControlState {
            sensitive: true,
            subtitle: strings::GAPLESS_SUBTITLE,
        }
    } else {
        GaplessControlState {
            sensitive: false,
            subtitle: strings::GAPLESS_CROSSFADE_ACTIVE_SUBTITLE,
        }
    }
}

fn apply_gapless_control_state(row: &adw::SwitchRow, crossfade_seconds: u8) {
    let state = gapless_control_state(crossfade_seconds);
    row.set_sensitive(state.sensitive);
    row.set_subtitle(&strings::text(state.subtitle));
}
impl PreferencesContext {
    pub(in crate::ui::preferences) fn playback_page(self: &Rc<Self>) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::builder()
            .title(strings::text(strings::PREFERENCES_PLAYBACK))
            .icon_name("audio-speakers-symbolic")
            .build();
        let equalizer_enabled = {
            let conn = &self.conn;
            settings::get_equalizer_enabled(conn)
        };
        let weak = Rc::downgrade(self);
        let on_enabled: Rc<dyn Fn(bool)> = Rc::new(move |active| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            if context.syncing_effect_controls.get() {
                return;
            }
            context.set_equalizer_enabled(active);
        });
        let stored_bands = settings::get_equalizer_bands(&self.conn);
        let weak = Rc::downgrade(self);
        let on_preset: Rc<dyn Fn([f64; 10]) -> bool> = Rc::new(move |bands| {
            let Some(context) = weak.upgrade() else {
                return false;
            };
            if let Err(error) = settings::set_equalizer_bands(&context.conn, bands) {
                tracing::warn!(%error, "could not save equalizer preset");
                return false;
            }
            context.apply_audio_effects();
            true
        });
        let weak = Rc::downgrade(self);
        let on_band: Rc<dyn Fn(usize, f64)> = Rc::new(move |index, value| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            let mut bands = settings::get_equalizer_bands(&context.conn);
            bands[index] = value;
            if let Err(error) = settings::set_equalizer_bands(&context.conn, bands) {
                tracing::warn!(%error, "could not save equalizer bands");
                return;
            }
            context.apply_audio_effects();
        });
        let controls = build_equalizer_controls(
            stored_bands,
            equalizer_enabled,
            on_enabled,
            &on_preset,
            on_band,
        );
        self.equalizer_controls
            .borrow_mut()
            .push(controls.enabled.clone());
        self.equalizer_surfaces
            .borrow_mut()
            .push(controls.root.clone().upcast());
        let equalizer = controls.group;
        // (equalizer/replaygain are added to the page after Audio Transitions
        // below, so Transitions leads the Playback page — matching the mockup.)
        let replaygain = adw::PreferencesGroup::builder()
            .title(strings::text(strings::REPLAYGAIN))
            .build();
        let modes = gtk4::StringList::new(&[
            &strings::text(strings::REPLAYGAIN_OFF),
            &strings::text(strings::REPLAYGAIN_TRACK),
            &strings::text(strings::REPLAYGAIN_ALBUM),
        ]);
        let selected_mode = {
            let conn = &self.conn;
            replay_gain_index(settings::get_replay_gain_mode(conn))
        };
        let mode = crate::ui::rows::combo_row()
            .title(strings::text(strings::REPLAYGAIN_MODE))
            .model(&modes)
            .selected(selected_mode)
            .build();
        let weak = Rc::downgrade(self);
        mode.connect_selected_notify(move |row| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            if context.syncing_effect_controls.get() {
                return;
            }
            context.set_replay_gain_mode(replay_gain_from_index(row.selected()));
        });
        self.replaygain_mode.borrow_mut().replace(mode.clone());
        replaygain.add(&mode);

        // Audio Transitions: a crossfade slider + a gapless toggle in one
        // group (the "NEW" badge sits in the group header suffix). The two
        // controls are independent; the effective mode is derived from them
        // (see `settings::get_track_transition`).
        let transitions = adw::PreferencesGroup::builder()
            .title(strings::text(strings::AUDIO_TRANSITIONS))
            .build();

        // Both controls sit in one boxed-list card (crossfade on top, gapless
        // below), matching the mockup. We build the list ourselves because the
        // crossfade row is a custom widget — a full-width slider does not fit a
        // standard AdwActionRow, and a non-row widget added straight to the
        // group would fall outside its card.
        let list = gtk4::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk4::SelectionMode::None);

        // Crossfade card row: title + live value + subtitle + a 0..10 s slider
        // ("Off" at 0).
        let stored_crossfade = {
            let conn = &self.conn;
            settings::get_crossfade_seconds(conn)
        };
        let crossfade_content = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        crossfade_content.add_css_class("reprise-crossfade");
        let crossfade_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let crossfade_title = gtk4::Label::new(Some(&strings::text(strings::CROSSFADE)));
        crossfade_title.add_css_class("title");
        crossfade_title.set_xalign(0.0);
        crossfade_title.set_hexpand(true);
        let crossfade_value = gtk4::Label::new(Some(&crossfade_value_label(stored_crossfade)));
        crossfade_value.add_css_class("reprise-crossfade-value");
        crossfade_value.set_halign(gtk4::Align::End);
        crossfade_header.append(&crossfade_title);
        crossfade_header.append(&crossfade_value);
        crossfade_content.append(&crossfade_header);
        let crossfade_subtitle =
            gtk4::Label::new(Some(&strings::text(strings::CROSSFADE_SUBTITLE)));
        crossfade_subtitle.add_css_class("dim-label");
        crossfade_subtitle.set_xalign(0.0);
        crossfade_content.append(&crossfade_subtitle);
        let crossfade_scale = gtk4::Scale::with_range(
            gtk4::Orientation::Horizontal,
            f64::from(settings::CROSSFADE_SECONDS_MIN),
            f64::from(settings::CROSSFADE_SECONDS_MAX),
            1.0,
        );
        crossfade_scale.set_value(f64::from(stored_crossfade));
        crossfade_scale.set_draw_value(false);
        crossfade_scale.set_hexpand(true);
        crossfade_scale.add_css_class("reprise-crossfade-scale");
        crossfade_scale.add_mark(
            0.0,
            gtk4::PositionType::Bottom,
            Some(&strings::text(strings::CROSSFADE_OFF)),
        );
        crossfade_scale.add_mark(5.0, gtk4::PositionType::Bottom, Some("5 s"));
        crossfade_scale.add_mark(10.0, gtk4::PositionType::Bottom, Some("10 s"));
        crossfade_content.append(&crossfade_scale);
        let crossfade_lbrow = gtk4::ListBoxRow::new();
        crossfade_lbrow.set_activatable(false);
        crossfade_lbrow.set_child(Some(&crossfade_content));
        list.append(&crossfade_lbrow);

        // Gapless: a standard switch row, the second row of the same card.
        let gapless_enabled = {
            let conn = &self.conn;
            settings::get_gapless_enabled(conn)
        };
        let gapless = crate::ui::rows::switch_row()
            .title(strings::text(strings::GAPLESS_PLAYBACK))
            .active(gapless_enabled)
            .build();
        apply_gapless_control_state(&gapless, stored_crossfade);
        list.append(&gapless);
        transitions.add(&list);

        let weak = Rc::downgrade(self);
        let value_label = crossfade_value.clone();
        let gapless_for_crossfade = gapless.clone();
        crossfade_scale.connect_value_changed(move |scale| {
            let seconds = scale.value().round() as u8;
            value_label.set_label(&crossfade_value_label(seconds));
            apply_gapless_control_state(&gapless_for_crossfade, seconds);
            if let Some(context) = weak.upgrade() {
                context.set_crossfade_seconds(seconds);
            }
        });
        let weak = Rc::downgrade(self);
        gapless.connect_active_notify(move |row| {
            if let Some(context) = weak.upgrade() {
                context.set_gapless_enabled(row.is_active());
            }
        });
        // Order: Audio Transitions first (matching the mockup), then the
        // Equalizer and ReplayGain groups built above.
        page.add(&transitions);
        page.add(&equalizer);
        page.add(&replaygain);
        page
    }
    /// Persists the gapless toggle and pushes the derived transition to the
    /// backend (plus a re-feed) so the change takes effect immediately.
    fn set_gapless_enabled(&self, enabled: bool) {
        {
            let conn = &self.conn;
            if let Err(error) = settings::set_gapless_enabled(conn, enabled) {
                tracing::warn!(%error, "could not save gapless setting");
                return;
            }
        }
        if let Some(player) = &self.player {
            player.apply_transition();
        }
    }

    fn set_crossfade_seconds(&self, seconds: u8) {
        {
            let conn = &self.conn;
            if let Err(error) = settings::set_crossfade_seconds(conn, seconds) {
                tracing::warn!(%error, "could not save crossfade duration");
                return;
            }
        }
        if let Some(player) = &self.player {
            player.apply_transition();
        }
    }
}
