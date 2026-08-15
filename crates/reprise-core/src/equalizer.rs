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

/// The standard curves offered by every Reprise frontend.
///
/// The gains are defined once here so selecting Rock on Android cannot drift
/// from selecting Rock on the desktop. Backends still project the resulting
/// authored curve onto their own live band topology.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EqualizerPreset {
    Flat,
    Rock,
    Pop,
    Bass,
}

impl EqualizerPreset {
    pub const ALL: [Self; 4] = [Self::Flat, Self::Rock, Self::Pop, Self::Bass];

    pub const fn ten_band_levels(self) -> [f64; 10] {
        match self {
            Self::Flat => [0.0; 10],
            Self::Rock => [4.0, 3.0, 2.0, 0.0, -1.0, 0.0, 2.0, 3.0, 4.0, 4.0],
            Self::Pop => [-1.0, 1.0, 3.0, 4.0, 2.0, 0.0, -1.0, -1.0, 1.0, 2.0],
            Self::Bass => [7.0, 6.0, 5.0, 3.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        }
    }

    pub fn curve(self) -> EqualizerCurve {
        EqualizerCurve::from_gstreamer_levels(self.ten_band_levels())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EqualizerPoint {
    pub frequency_hz: f64,
    pub gain_db: f64,
}

/// An ordered, validated curve.
///
/// Deserialization is routed through [`EqualizerCurve::new`] by
/// `#[serde(try_from)]`, so the invariants below are structural rather than a
/// convention every reader has to remember. A derived `Deserialize` would write
/// straight into the private field, and `{"points":[]}` would then be accepted
/// and panic later inside [`EqualizerCurve::project_to_gstreamer`] — the type is
/// `pub` in a `pub` module, and the whole premise of this design is that a
/// stored curve is always a valid one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "EqualizerCurveWire")]
pub struct EqualizerCurve {
    points: Vec<EqualizerPoint>,
}

/// The literal serialized shape, and the only door into [`EqualizerCurve`] that
/// serde knows about.
#[derive(Deserialize)]
struct EqualizerCurveWire {
    points: Vec<EqualizerPoint>,
}

impl TryFrom<EqualizerCurveWire> for EqualizerCurve {
    type Error = EqualizerCurveError;

    fn try_from(wire: EqualizerCurveWire) -> Result<Self, Self::Error> {
        Self::new(wire.points)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EqualizerBand {
    pub frequency_hz: f64,
    pub min_gain_db: f64,
    pub max_gain_db: f64,
}

/// A curve seen through one backend's bands. Read, never stored: writing one
/// back would make a picture of the truth into the truth.
#[derive(Clone, Debug, PartialEq)]
pub struct EqualizerProjection {
    pub band_levels_db: Vec<f64>,
    /// The bands land on the authored points, so nothing was interpolated.
    ///
    /// Deliberately kept though no caller reads it yet: it and [`Self::clipped`]
    /// are how the decided contract — "a non-exact or clipped projection must
    /// never overwrite the authored curve" — is stated in code. Today that rule
    /// is kept structurally instead, by no display path ever writing, so these
    /// two are the record of *why*, not a switch anything flips.
    pub exact: bool,
    /// At least one band could not reach the authored gain and was clamped.
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
    #[error("the stored equalizer curve is malformed: {detail}")]
    Malformed { detail: String },
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

    /// True when the curve's points *are* GStreamer's ten centres.
    ///
    /// The desktop's ten sliders cannot express anything else, so a ten-band
    /// write over a curve that fails this predicate replaces authored points
    /// with projections of them. See `settings::set_equalizer_bands_in`.
    pub fn is_gstreamer_ten_band(&self) -> bool {
        self.points.len() == GSTREAMER_EQUALIZER_CENTRES_HZ.len()
            && self
                .points
                .iter()
                .zip(GSTREAMER_EQUALIZER_CENTRES_HZ)
                .all(|(point, centre_hz)| point.frequency_hz == centre_hz)
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

    /// Reads a stored curve. Validation lives in `Deserialize` now, so this no
    /// longer re-checks anything — it only names the failure for the log.
    pub(crate) fn parse(value: &str) -> Result<Self, EqualizerCurveError> {
        serde_json::from_str::<Self>(value).map_err(|error| EqualizerCurveError::Malformed {
            detail: error.to_string(),
        })
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

    /// The invariants have to be structural, not a convention `parse` remembers.
    /// Before `#[serde(try_from)]`, `Deserialize` wrote straight into the
    /// private field: this payload was *accepted*, and `project_to_gstreamer`
    /// then panicked with "index out of bounds, len is 0" on the first sample.
    /// Removing the attribute turns this test red.
    #[test]
    fn deserialization_cannot_smuggle_a_curve_past_the_invariants() {
        let empty = serde_json::from_str::<EqualizerCurve>(r#"{"points":[]}"#).unwrap_err();
        assert!(
            empty.to_string().contains("at least one point"),
            "an empty curve must be rejected by name, got: {empty}",
        );

        let unordered = serde_json::from_str::<EqualizerCurve>(
            r#"{"points":[{"frequency_hz":100.0,"gain_db":0.0},
                          {"frequency_hz":100.0,"gain_db":1.0}]}"#,
        )
        .unwrap_err();
        assert!(
            unordered.to_string().contains("strictly increasing"),
            "an unordered curve must be rejected by name, got: {unordered}",
        );

        assert!(
            serde_json::from_str::<EqualizerCurve>(
                r#"{"points":[{"frequency_hz":0.0,"gain_db":0.0}]}"#
            )
            .is_err(),
            "a zero frequency is outside the range the sampler assumes",
        );

        // And the door still opens for a curve that is actually valid.
        let curve = EqualizerCurve::from_gstreamer_levels([1.5; 10]);
        assert_eq!(EqualizerCurve::parse(&curve.serialize()).unwrap(), curve);
    }

    #[test]
    fn only_gstreamers_own_ten_centres_count_as_a_ten_band_curve() {
        assert!(EqualizerCurve::flat_gstreamer().is_gstreamer_ten_band());
        // A phone's five bands: same kind of value, a different authored shape.
        let phone = EqualizerCurve::new(
            [60.0, 230.0, 910.0, 3_600.0, 14_000.0]
                .into_iter()
                .map(|frequency_hz| EqualizerPoint {
                    frequency_hz,
                    gain_db: 2.0,
                })
                .collect(),
        )
        .unwrap();
        assert!(!phone.is_gstreamer_ten_band());
        // Ten points, but not at the centres the desktop's sliders mean.
        let shifted = EqualizerCurve::new(
            GSTREAMER_EQUALIZER_CENTRES_HZ
                .into_iter()
                .map(|frequency_hz| EqualizerPoint {
                    frequency_hz: frequency_hz + 1.0,
                    gain_db: 0.0,
                })
                .collect(),
        )
        .unwrap();
        assert!(!shifted.is_gstreamer_ten_band());
    }

    #[test]
    fn standard_presets_match_the_desktop_contract() {
        assert_eq!(
            EqualizerPreset::ALL,
            [
                EqualizerPreset::Flat,
                EqualizerPreset::Rock,
                EqualizerPreset::Pop,
                EqualizerPreset::Bass,
            ],
        );
        assert_eq!(EqualizerPreset::Flat.ten_band_levels(), [0.0; 10]);
        assert_eq!(
            EqualizerPreset::Rock.ten_band_levels(),
            [4.0, 3.0, 2.0, 0.0, -1.0, 0.0, 2.0, 3.0, 4.0, 4.0],
        );
        assert_eq!(
            EqualizerPreset::Pop.ten_band_levels(),
            [-1.0, 1.0, 3.0, 4.0, 2.0, 0.0, -1.0, -1.0, 1.0, 2.0],
        );
        assert_eq!(
            EqualizerPreset::Bass.ten_band_levels(),
            [7.0, 6.0, 5.0, 3.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        );

        for preset in EqualizerPreset::ALL {
            let curve = preset.curve();
            assert!(curve.is_gstreamer_ten_band());
            assert_eq!(curve.project_to_gstreamer(), preset.ten_band_levels());
        }
    }
}
