//! Live playback-effect application and duplicate-control synchronization.
//!
//! Every widget handle is cloned out of its `RefCell` before calling GTK, so
//! a synchronous notify callback can never collide with an outstanding borrow.

use libadwaita::prelude::*;
use reprise_core::library::settings::{self, ReplayGainMode};

use super::strings;
use super::{replay_gain_index, PreferencesContext};

impl PreferencesContext {
    pub(in crate::ui) fn apply_audio_effects(&self) {
        let effects = {
            let conn = self.conn.borrow();
            super::audio_effects::stored(&conn)
        };
        let Some(player) = &self.player else {
            return;
        };
        if let Err(error) = player.set_audio_effects(effects) {
            tracing::warn!(%error, "could not apply audio effects");
            let active = player.active_audio_effects();
            {
                let conn = self.conn.borrow();
                if let Err(persist_error) = super::audio_effects::persist(&conn, &active) {
                    tracing::warn!(%persist_error, "could not restore active audio settings");
                }
            }
            let equalizer_rows = self.equalizer_controls.borrow().clone();
            let equalizer_surfaces = self.equalizer_surfaces.borrow().clone();
            let replaygain_mode = self.replaygain_mode.borrow().clone();
            self.syncing_effect_controls.set(true);
            for row in equalizer_rows {
                row.set_active(active.equalizer_enabled);
            }
            for surface in equalizer_surfaces {
                surface.set_sensitive(active.equalizer_enabled);
            }
            if let Some(row) = replaygain_mode {
                row.set_selected(replay_gain_index(active.replay_gain));
            }
            self.syncing_effect_controls.set(false);
            player.show_toast(&strings::text(strings::AUDIO_EFFECTS_FAILED));
        }
    }

    pub(in crate::ui) fn set_equalizer_enabled(&self, active: bool) {
        let saved = {
            let conn = self.conn.borrow();
            settings::set_equalizer_enabled(&conn, active)
        };
        if let Err(error) = saved {
            tracing::warn!(%error, "could not save equalizer state");
            return;
        }
        let rows = self.equalizer_controls.borrow().clone();
        let surfaces = self.equalizer_surfaces.borrow().clone();
        self.syncing_effect_controls.set(true);
        for row in rows {
            row.set_active(active);
        }
        for surface in surfaces {
            surface.set_sensitive(active);
        }
        self.syncing_effect_controls.set(false);
        self.apply_audio_effects();
    }

    pub(in crate::ui) fn set_replay_gain_mode(&self, mode: ReplayGainMode) {
        let saved = {
            let conn = self.conn.borrow();
            settings::set_replay_gain_mode(&conn, mode)
        };
        if let Err(error) = saved {
            tracing::warn!(%error, "could not save ReplayGain mode");
            return;
        }
        let mode_row = self.replaygain_mode.borrow().clone();
        self.syncing_effect_controls.set(true);
        if let Some(row) = mode_row {
            row.set_selected(replay_gain_index(mode));
        }
        self.syncing_effect_controls.set(false);
        self.apply_audio_effects();
    }
}
