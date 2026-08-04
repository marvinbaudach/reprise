//! Backend-independent equalizer curves and deterministic band projections.

use serde::{Deserialize, Serialize};

/// GStreamer's real `equalizer-10bands` centres, not the rounded GTK labels.
pub const GSTREAMER_EQUALIZER_CENTRES_HZ: [f64; 10] = [
    29.0, 59.0, 119.0, 237.0, 474.0, 947.0, 1_889.0, 3_770.0, 7_523.0, 15_011.0,
];

const GSTREAMER_MIN_GAIN_DB: f64 = -12.0;
const GSTREAMER_MAX_GAIN_DB: f64 = 12.0;
const MAX_CURVE_POINTS: usize = 128;
const MIN_FREQUENCY_HZ: f64 = 1.0;
const MAX_FREQUENCY_HZ: f64 = 1_000_000.0;
const MAX_ABSOLUTE_GAIN_DB: f64 = 120.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EqualizerPoint {
    pub frequency_hz: f64,
    pub gain_db: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EqualizerCurve {
    points: Vec<EqualizerPoint>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EqualizerBand {
    pub frequency_hz: f64,
    pub min_gain_db: f64,
    pub max_gain_db: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EqualizerProjection {
    pub band_levels_db: Vec<f64>,
    pub exact: bool,
    pub clipped: bool,
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum EqualizerCurveError {
    #[error("an equalizer curve needs at least one point")]
    Empty,
    #[error("an equalizer curve may contain at most {MAX_CURVE_POINTS} points")]
    TooManyPoints,
    #[error("equalizer point {index} has a non-finite value")]
    NonFinite { index: usize },
    #[error("equalizer point {index} must have a positive frequency")]
    FrequencyOutOfRange { index: usize },
    #[error("equalizer point {index} has a gain outside the corruption guard")]
    GainOutOfRange { index: usize },
    #[error("equalizer frequencies must be strictly increasing at point {index}")]
    FrequenciesNotIncreasing { index: usize },
    #[error("the stored equalizer curve is malformed")]
    Malformed,
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum EqualizerProjectionError {
    #[error("equalizer band {index} is invalid")]
    InvalidBand { index: usize },
    #[error("equalizer band frequencies must be strictly increasing at band {index}")]
    BandsNotIncreasing { index: usize },
}

impl EqualizerCurve {
    pub fn new(points: Vec<EqualizerPoint>) -> Result<Self, EqualizerCurveError> {
        if points.is_empty() {
            return Err(EqualizerCurveError::Empty);
        }
        if points.len() > MAX_CURVE_POINTS {
            return Err(EqualizerCurveError::TooManyPoints);
        }
        for (index, point) in points.iter().enumerate() {
            if !point.frequency_hz.is_finite() || !point.gain_db.is_finite() {
                return Err(EqualizerCurveError::NonFinite { index });
            }
            if !(MIN_FREQUENCY_HZ..=MAX_FREQUENCY_HZ).contains(&point.frequency_hz) {
                return Err(EqualizerCurveError::FrequencyOutOfRange { index });
            }
            if point.gain_db.abs() > MAX_ABSOLUTE_GAIN_DB {
                return Err(EqualizerCurveError::GainOutOfRange { index });
            }
            if index > 0 && point.frequency_hz <= points[index - 1].frequency_hz {
                return Err(EqualizerCurveError::FrequenciesNotIncreasing { index });
            }
        }
        Ok(Self { points })
    }

    pub fn flat_gstreamer() -> Self {
        Self::from_gstreamer_levels([0.0; 10])
    }

    pub fn from_gstreamer_levels(levels: [f64; 10]) -> Self {
        let points = GSTREAMER_EQUALIZER_CENTRES_HZ
            .into_iter()
            .zip(levels)
            .map(|(frequency_hz, gain_db)| EqualizerPoint {
                frequency_hz,
                gain_db: if gain_db.is_finite() {
                    gain_db.clamp(GSTREAMER_MIN_GAIN_DB, GSTREAMER_MAX_GAIN_DB)
                } else {
                    0.0
                },
            })
            .collect();
        Self::new(points).expect("the fixed GStreamer curve is valid")
    }

    pub fn points(&self) -> &[EqualizerPoint] {
        &self.points
    }

    pub fn project(
        &self,
        bands: &[EqualizerBand],
    ) -> Result<EqualizerProjection, EqualizerProjectionError> {
        validate_bands(bands)?;
        let mut exact = bands.len() == self.points.len();
        let mut clipped = false;
        let band_levels_db = bands
            .iter()
            .map(|band| {
                let sampled = self.sample(band.frequency_hz);
                let level = sampled.clamp(band.min_gain_db, band.max_gain_db);
                clipped |= level != sampled;
                exact &= self
                    .points
                    .iter()
                    .any(|point| point.frequency_hz == band.frequency_hz);
                level
            })
            .collect();
        Ok(EqualizerProjection {
            band_levels_db,
            exact: exact && !clipped,
            clipped,
        })
    }

    pub fn project_to_gstreamer(&self) -> [f64; 10] {
        let bands = GSTREAMER_EQUALIZER_CENTRES_HZ.map(|frequency_hz| EqualizerBand {
            frequency_hz,
            min_gain_db: GSTREAMER_MIN_GAIN_DB,
            max_gain_db: GSTREAMER_MAX_GAIN_DB,
        });
        self.project(&bands)
            .expect("the fixed GStreamer capabilities are valid")
            .band_levels_db
            .try_into()
            .expect("the fixed projection has ten bands")
    }

    pub(crate) fn serialize(&self) -> String {
        serde_json::to_string(self).expect("validated finite points serialize as JSON")
    }

    pub(crate) fn parse(value: &str) -> Result<Self, EqualizerCurveError> {
        let decoded =
            serde_json::from_str::<Self>(value).map_err(|_| EqualizerCurveError::Malformed)?;
        Self::new(decoded.points)
    }

    fn sample(&self, frequency_hz: f64) -> f64 {
        if frequency_hz <= self.points[0].frequency_hz {
            return self.points[0].gain_db;
        }
        let last = self.points.last().expect("a validated curve is non-empty");
        if frequency_hz >= last.frequency_hz {
            return last.gain_db;
        }
        let upper = self
            .points
            .partition_point(|point| point.frequency_hz < frequency_hz);
        let left = self.points[upper - 1];
        let right = self.points[upper];
        if frequency_hz == right.frequency_hz {
            return right.gain_db;
        }
        let position = (frequency_hz.ln() - left.frequency_hz.ln())
            / (right.frequency_hz.ln() - left.frequency_hz.ln());
        left.gain_db + position * (right.gain_db - left.gain_db)
    }
}

fn validate_bands(bands: &[EqualizerBand]) -> Result<(), EqualizerProjectionError> {
    for (index, band) in bands.iter().enumerate() {
        if !band.frequency_hz.is_finite()
            || !(MIN_FREQUENCY_HZ..=MAX_FREQUENCY_HZ).contains(&band.frequency_hz)
            || !band.min_gain_db.is_finite()
            || !band.max_gain_db.is_finite()
            || band.min_gain_db > band.max_gain_db
        {
            return Err(EqualizerProjectionError::InvalidBand { index });
        }
        if index > 0 && band.frequency_hz <= bands[index - 1].frequency_hz {
            return Err(EqualizerProjectionError::BandsNotIncreasing { index });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_interpolates_in_log_frequency_and_clamps_only_the_picture() {
        let curve = EqualizerCurve::new(vec![
            EqualizerPoint {
                frequency_hz: 100.0,
                gain_db: -20.0,
            },
            EqualizerPoint {
                frequency_hz: 400.0,
                gain_db: 20.0,
            },
        ])
        .unwrap();

        let projection = curve
            .project(&[EqualizerBand {
                frequency_hz: 200.0,
                min_gain_db: -12.0,
                max_gain_db: 12.0,
            }])
            .unwrap();

        assert!((projection.band_levels_db[0] - 0.0).abs() < 1e-10);
        assert!(!projection.exact);
        assert!(!projection.clipped);
        assert_eq!(curve.points()[0].gain_db, -20.0);
    }

    #[test]
    fn validation_rejects_values_that_cannot_describe_an_ordered_curve() {
        assert_eq!(
            EqualizerCurve::new(Vec::new()),
            Err(EqualizerCurveError::Empty)
        );
        assert!(matches!(
            EqualizerCurve::new(vec![EqualizerPoint {
                frequency_hz: f64::NAN,
                gain_db: 0.0,
            }]),
            Err(EqualizerCurveError::NonFinite { .. })
        ));
        assert!(matches!(
            EqualizerCurve::new(vec![
                EqualizerPoint {
                    frequency_hz: 100.0,
                    gain_db: 0.0,
                },
                EqualizerPoint {
                    frequency_hz: 100.0,
                    gain_db: 1.0,
                },
            ]),
            Err(EqualizerCurveError::FrequenciesNotIncreasing { .. })
        ));
    }
}
