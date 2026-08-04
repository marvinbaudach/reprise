//! Typed Android playback settings over Core's shared persistence.

use reprise_core::equalizer::{EqualizerCurve, EqualizerPoint};
use reprise_core::library::settings;

use crate::{LibraryError, MusicLibrary};

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct AndroidEqualizerPoint {
    pub frequency_hz: f64,
    pub gain_db: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct AndroidEqualizerBand {
    pub frequency_hz: f64,
    pub gain_db: f64,
    pub minimum_gain_db: f64,
    pub maximum_gain_db: f64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct AndroidEqualizerSnapshot {
    pub enabled: bool,
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

#[uniffi::export]
impl MusicLibrary {
    /// Reads the authored curve. A backend projection is never persisted here.
    pub fn playback_settings(&self) -> Result<AndroidPlaybackSettings, LibraryError> {
        let state = self.lock()?;
        Ok(AndroidPlaybackSettings::load(&state.db))
    }

    pub fn set_equalizer_enabled(&self, enabled: bool) -> Result<(), LibraryError> {
        let state = self.lock()?;
        settings::set_equalizer_enabled(&state.db, enabled).map_err(|error| database_error(&error))
    }

    /// Replaces the authored curve with points from one explicit phone edit.
    pub fn replace_equalizer_curve(
        &self,
        points: Vec<AndroidEqualizerPoint>,
    ) -> Result<(), LibraryError> {
        let curve = EqualizerCurve::new(points.into_iter().map(EqualizerPoint::from).collect())
            .map_err(|error| LibraryError::InvalidPlaybackSetting {
                detail: error.to_string(),
            })?;
        let state = self.lock()?;
        settings::set_equalizer_curve(&state.db, &curve).map_err(|error| database_error(&error))
    }

    pub fn set_gapless_enabled(&self, enabled: bool) -> Result<(), LibraryError> {
        let state = self.lock()?;
        settings::set_gapless_enabled(&state.db, enabled).map_err(|error| database_error(&error))
    }
}

fn database_error(error: &impl ToString) -> LibraryError {
    LibraryError::Database {
        detail: error.to_string(),
    }
}
