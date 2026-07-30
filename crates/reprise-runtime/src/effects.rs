//! The equalizer and ReplayGain, owned by whoever owns the audio path.
//!
//! Two states, not one. What a user *stored* is their intent and lives in the
//! settings table; what the backend *accepted* is what the sound is actually
//! going through. They differ whenever the pipeline has no element for what
//! was asked — GTK has kept them apart for exactly that reason, and a runtime
//! that reported only the stored value would be telling every surface a
//! promise nothing kept.

use reprise_core::db::Db;
use reprise_core::library::settings::ReplayGainMode;
use reprise_core::library::{audio_effect_settings, settings};
use reprise_core::playback::{AudioEffects, PlaybackBackend};
use reprise_runtime_protocol::effects::{EffectsRequest, EffectsSnapshot, EQUALIZER_BAND_COUNT};

use crate::error::Rejected;

/// What the backend has accepted, and whether that is what was asked for.
pub(crate) struct Effects {
    active: AudioEffects,
    /// Set when the stored settings were refused and `active` is the fallback.
    degraded: bool,
}

impl Effects {
    /// Applies what the user last stored, falling back to no effects at all
    /// if the backend will not take them.
    ///
    /// A missing equalizer element must not cost the user their music. The
    /// same choice GTK's `apply_initial` makes, and for the same reason: the
    /// effects are an enhancement, and refusing to play without them would
    /// turn a cosmetic gap into an outage.
    pub(crate) fn apply_stored(db: &Db, backend: &dyn PlaybackBackend) -> Self {
        let stored = audio_effect_settings::load(db);
        if backend.set_audio_effects(stored.clone()).is_ok() {
            return Self {
                active: stored,
                degraded: false,
            };
        }

        tracing::warn!("stored audio effects are unavailable; falling back to none");
        let fallback = AudioEffects::default();
        if let Err(error) = backend.set_audio_effects(fallback.clone()) {
            // Nothing left to try. The backend is playing through whatever it
            // had, which the snapshot now describes as degraded either way.
            tracing::warn!(%error, "could not explicitly clear audio effects either");
        }
        // The switches are turned off so the next start does not repeat a
        // failure the user has already been shown. The *bands* are left
        // exactly as they were: they are a curve someone dialled in, and
        // flattening them would quietly destroy work over a missing plugin
        // that may well be installed tomorrow.
        if let Err(error) = settings::set_equalizer_enabled(db, false) {
            tracing::warn!(%error, "could not persist the equalizer fallback");
        }
        if let Err(error) = settings::set_replay_gain_mode(db, ReplayGainMode::Off) {
            tracing::warn!(%error, "could not persist the ReplayGain fallback");
        }
        Self {
            active: fallback,
            degraded: true,
        }
    }

    /// Applies a requested change and, only if the backend took it, stores it.
    ///
    /// Persisting first would leave the settings describing something the
    /// audio path refused, which the next start would then read, fail on, and
    /// fall back from — the user's setting silently switching itself off one
    /// launch later, far from the action that caused it.
    pub(crate) fn set(
        &mut self,
        db: &Db,
        backend: &dyn PlaybackBackend,
        request: &EffectsRequest,
    ) -> Result<(), crate::error::RuntimeError> {
        let requested = from_request(request)?;
        backend
            .set_audio_effects(requested.clone())
            .map_err(|error| crate::transport::backend_failed(&error))?;
        if let Err(error) = audio_effect_settings::store(db, &requested) {
            // Applied but not stored: the sound is already what was asked
            // for, so reporting a failure would be wrong about the thing the
            // user can hear. It will not survive a restart, which is the part
            // worth a line in the log.
            tracing::warn!(%error, "audio effects applied but could not be stored");
        }
        self.active = requested;
        // Whatever the backend refused before, it has just accepted this.
        self.degraded = false;
        Ok(())
    }

    pub(crate) fn snapshot(&self) -> EffectsSnapshot {
        EffectsSnapshot {
            equalizer_enabled: self.active.equalizer_enabled,
            equalizer_bands: self.active.equalizer_bands.to_vec(),
            replay_gain: match self.active.replay_gain {
                ReplayGainMode::Off => "off",
                ReplayGainMode::Track => "track",
                ReplayGainMode::Album => "album",
            }
            .to_owned(),
            degraded: self.degraded,
        }
    }
}

/// Validates a wire request into the core's own shape.
///
/// Here rather than in the bus layer: this is where the rejection vocabulary
/// lives (§9.7), and a second validator at the edge is a second answer to the
/// same question.
fn from_request(request: &EffectsRequest) -> Result<AudioEffects, crate::error::RuntimeError> {
    let bands: [f64; EQUALIZER_BAND_COUNT] = request
        .equalizer_bands
        .clone()
        .try_into()
        .map_err(|_| crate::error::RuntimeError::Rejected(Rejected::UnknownEqualizerShape))?;
    let replay_gain = match request.replay_gain.as_str() {
        "off" => ReplayGainMode::Off,
        "track" => ReplayGainMode::Track,
        "album" => ReplayGainMode::Album,
        _ => {
            return Err(crate::error::RuntimeError::Rejected(
                Rejected::UnknownReplayGainMode,
            ))
        }
    };
    Ok(AudioEffects {
        equalizer_enabled: request.equalizer_enabled,
        equalizer_bands: bands,
        replay_gain,
    })
}
