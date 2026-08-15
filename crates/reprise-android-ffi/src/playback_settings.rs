//! Typed Android playback settings over Core's shared persistence.

use reprise_core::equalizer::{EqualizerBand, EqualizerCurve, EqualizerPoint, EqualizerPreset};
use reprise_core::library::settings;

use crate::{LibraryError, MusicLibrary};

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct AndroidEqualizerPoint {
    pub frequency_hz: f64,
    pub gain_db: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AndroidEqualizerPreset {
    Flat,
    Rock,
    Pop,
    Bass,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct AndroidEqualizerPresetDefinition {
    pub preset: AndroidEqualizerPreset,
    pub curve: Vec<AndroidEqualizerPoint>,
}

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct AndroidEqualizerBand {
    pub frequency_hz: f64,
    pub gain_db: f64,
    pub minimum_gain_db: f64,
    pub maximum_gain_db: f64,
}

/// What one device says its own equalizer can do at one band: where it sits and
/// how far it moves. The input side of [`project_equalizer_curve`].
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct AndroidEqualizerBandCapability {
    pub frequency_hz: f64,
    pub minimum_gain_db: f64,
    pub maximum_gain_db: f64,
}

/// What the device's equalizer is currently doing.
///
/// `None` from `equalizer_snapshot` means there is no audio session to ask at
/// all. `available: false` means there *is* one and the device gave us no
/// equalizer for it — a different fact, and a surface that reports one as the
/// other tells the user to start playback while a track is playing.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct AndroidEqualizerSnapshot {
    pub enabled: bool,
    pub available: bool,
    pub bands: Vec<AndroidEqualizerBand>,
}

impl From<&EqualizerPoint> for AndroidEqualizerPoint {
    fn from(point: &EqualizerPoint) -> Self {
        Self {
            frequency_hz: point.frequency_hz,
            gain_db: point.gain_db,
        }
    }
}

impl From<AndroidEqualizerPoint> for EqualizerPoint {
    fn from(point: AndroidEqualizerPoint) -> Self {
        Self {
            frequency_hz: point.frequency_hz,
            gain_db: point.gain_db,
        }
    }
}

impl From<EqualizerPreset> for AndroidEqualizerPreset {
    fn from(preset: EqualizerPreset) -> Self {
        match preset {
            EqualizerPreset::Flat => Self::Flat,
            EqualizerPreset::Rock => Self::Rock,
            EqualizerPreset::Pop => Self::Pop,
            EqualizerPreset::Bass => Self::Bass,
        }
    }
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct AndroidPlaybackSettings {
    pub equalizer_enabled: bool,
    pub equalizer_curve: Vec<AndroidEqualizerPoint>,
    pub gapless_enabled: bool,
}

impl AndroidPlaybackSettings {
    pub(crate) fn load(db: &reprise_core::db::Db) -> Self {
        Self {
            equalizer_enabled: settings::get_equalizer_enabled(db),
            equalizer_curve: settings::get_equalizer_curve(db)
                .points()
                .iter()
                .map(AndroidEqualizerPoint::from)
                .collect(),
            gapless_enabled: settings::get_gapless_enabled(db),
        }
    }
}

/// The shared curves in the same order used by the desktop selector.
#[uniffi::export]
pub fn standard_equalizer_presets() -> Vec<AndroidEqualizerPresetDefinition> {
    EqualizerPreset::ALL
        .into_iter()
        .map(|preset| AndroidEqualizerPresetDefinition {
            preset: preset.into(),
            curve: preset
                .curve()
                .points()
                .iter()
                .map(AndroidEqualizerPoint::from)
                .collect(),
        })
        .collect()
}

#[uniffi::export]
impl MusicLibrary {
    /// Reads the authored curve. A backend projection is never persisted here.
    pub fn playback_settings(&self) -> Result<AndroidPlaybackSettings, LibraryError> {
        let reader = self.reader()?;
        Ok(AndroidPlaybackSettings::load(&reader))
    }

    pub fn set_equalizer_enabled(&self, enabled: bool) -> Result<(), LibraryError> {
        let writer = self.writer()?;
        settings::set_equalizer_enabled(&writer, enabled).map_err(|error| database_error(&error))
    }

    /// Replaces the authored curve with points from one explicit phone edit.
    pub fn replace_equalizer_curve(
        &self,
        points: Vec<AndroidEqualizerPoint>,
    ) -> Result<(), LibraryError> {
        let curve = EqualizerCurve::new(points.into_iter().map(EqualizerPoint::from).collect())
            .map_err(|error| invalid_playback_setting(&error))?;
        let writer = self.writer()?;
        settings::set_equalizer_curve(&writer, &curve).map_err(|error| database_error(&error))
    }

    pub fn set_gapless_enabled(&self, enabled: bool) -> Result<(), LibraryError> {
        let writer = self.writer()?;
        settings::set_gapless_enabled(&writer, enabled).map_err(|error| database_error(&error))
    }
}

/// Samples an authored curve at one device's band centres and clamps each value
/// to what that band can actually reach.
///
/// The Android engine used to carry its own copy of this arithmetic in Kotlin,
/// which meant the carefully tested Rust version guaranteed nothing about what a
/// real phone rendered. One decision, one implementation, one set of tests: the
/// device contributes its capabilities, the core does the maths.
#[uniffi::export]
pub fn project_equalizer_curve(
    curve: Vec<AndroidEqualizerPoint>,
    bands: Vec<AndroidEqualizerBandCapability>,
) -> Result<Vec<AndroidEqualizerBand>, LibraryError> {
    let curve = EqualizerCurve::new(curve.into_iter().map(EqualizerPoint::from).collect())
        .map_err(|error| invalid_playback_setting(&error))?;
    let capabilities = bands
        .iter()
        .map(|band| EqualizerBand {
            frequency_hz: band.frequency_hz,
            min_gain_db: band.minimum_gain_db,
            max_gain_db: band.maximum_gain_db,
        })
        .collect::<Vec<_>>();
    let projection = curve
        .project(&capabilities)
        .map_err(|error| invalid_playback_setting(&error))?;
    Ok(bands
        .into_iter()
        .zip(projection.band_levels_db)
        .map(|(band, gain_db)| AndroidEqualizerBand {
            frequency_hz: band.frequency_hz,
            gain_db,
            minimum_gain_db: band.minimum_gain_db,
            maximum_gain_db: band.maximum_gain_db,
        })
        .collect())
}

fn invalid_playback_setting(error: &impl ToString) -> LibraryError {
    LibraryError::InvalidPlaybackSetting {
        detail: error.to_string(),
    }
}

fn database_error(error: &impl ToString) -> LibraryError {
    LibraryError::Database {
        detail: error.to_string(),
    }
}

#[cfg(test)]
#[path = "playback_settings_tests.rs"]
mod tests;
