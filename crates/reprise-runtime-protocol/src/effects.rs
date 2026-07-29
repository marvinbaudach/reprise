//! What the audio path is doing to the sound on its way out.
//!
//! Two facts about this facet shape it. It is *applied* state, not stored
//! state — the backend can refuse an equalizer it has no element for, and a
//! surface that shows the stored value in that case is showing a promise
//! nothing kept. And it survives playback: effects apply to whatever comes
//! next, so this is not part of [`crate::playback::PlaybackSnapshot`], which
//! is empty when nothing is loaded.

use zvariant::{DeserializeDict, SerializeDict, Type};

/// How many equalizer bands there are, matching `reprise-core`'s own
/// `AudioEffects`. Fixed rather than negotiated: the bands are a fixed set of
/// centre frequencies, and a client that received a different count would
/// have no way to know which frequency each one is.
pub const EQUALIZER_BAND_COUNT: usize = 10;

/// The audio effects the backend has actually accepted.
#[derive(Debug, Clone, PartialEq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct EffectsSnapshot {
    pub equalizer_enabled: bool,
    /// Gain per band in dB, from the lowest centre frequency upwards.
    /// Always [`EQUALIZER_BAND_COUNT`] long; a shorter or longer list from a
    /// peer is a protocol violation rather than something to interpolate.
    pub equalizer_bands: Vec<f64>,
    /// `off`, `track`, `album`.
    pub replay_gain: String,
    /// Whether the backend refused what was stored and this is the fallback
    /// it accepted instead.
    ///
    /// Its own field because the two situations are otherwise identical on
    /// the wire — a flat, disabled equalizer is exactly what an untouched
    /// installation reports — and they mean opposite things to a user. One is
    /// "you have not set this up", the other is "your settings could not be
    /// applied".
    pub degraded: bool,
}

/// A requested effect change.
///
/// Separate from [`EffectsSnapshot`] because `degraded` is the runtime's
/// answer, not the caller's request: a client cannot ask for a fallback, and
/// a type that let it would invite one to be sent back unchanged.
#[derive(Debug, Clone, PartialEq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct EffectsRequest {
    pub equalizer_enabled: bool,
    /// Exactly [`EQUALIZER_BAND_COUNT`] gains in dB. A different length is
    /// rejected rather than padded: the bands are fixed centre frequencies,
    /// so a shorter list has no defensible reading.
    pub equalizer_bands: Vec<f64>,
    /// `off`, `track`, `album`.
    pub replay_gain: String,
}
