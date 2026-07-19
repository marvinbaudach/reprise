use super::*;

pub(super) struct RawReadyAnalysis {
    pub source_mtime: i64,
    pub source_size: i64,
    pub extractor_version: i64,
    pub profile_version: i64,
    pub analyzed_at: i64,
    pub status: String,
    pub loudness_rms: Option<f64>,
    pub dynamic_range: Option<f64>,
    pub spectral_centroid_hz: Option<f64>,
    pub spectral_rolloff_hz: Option<f64>,
    pub spectral_flux: Option<f64>,
    pub onset_rate: Option<f64>,
    pub tempo_bpm: Option<f64>,
    pub tempo_confidence: Option<f64>,
    pub intensity: Option<f64>,
    pub intensity_confidence: Option<f64>,
    pub brightness: Option<f64>,
    pub brightness_confidence: Option<f64>,
    pub dynamicity: Option<f64>,
    pub dynamicity_confidence: Option<f64>,
    pub rhythmicity: Option<f64>,
    pub rhythmicity_confidence: Option<f64>,
}

impl RawReadyAnalysis {
    pub(super) fn try_into_ready(self) -> Result<ReadyAnalysis, SoundProfileError> {
        if self.status != "ready" {
            return Err(SoundProfileError::CorruptData("expected ready status"));
        }
        let required = |value: Option<f64>| {
            value.ok_or(SoundProfileError::CorruptData(
                "ready analysis has null values",
            ))
        };
        let tempo = match (self.tempo_bpm, self.tempo_confidence) {
            (None, None) => None,
            (Some(bpm), Some(confidence)) => Some(TempoEstimate::new(bpm, confidence)?),
            _ => return Err(SoundProfileError::CorruptData("partial tempo estimate")),
        };
        ReadyAnalysis::new(
            SourceFingerprint::new(self.source_mtime, self.source_size)?,
            AnalysisVersions::new(
                u32::try_from(self.extractor_version)
                    .map_err(|_| SoundProfileError::InvalidVersion)?,
                u32::try_from(self.profile_version)
                    .map_err(|_| SoundProfileError::InvalidVersion)?,
            )?,
            self.analyzed_at,
            AudioEvidence::new(
                required(self.loudness_rms)?,
                required(self.dynamic_range)?,
                required(self.spectral_centroid_hz)?,
                required(self.spectral_rolloff_hz)?,
                required(self.spectral_flux)?,
                required(self.onset_rate)?,
                tempo,
            )?,
            SoundProfile::new(
                ProfileDimension::new(
                    required(self.intensity)?,
                    required(self.intensity_confidence)?,
                )?,
                ProfileDimension::new(
                    required(self.brightness)?,
                    required(self.brightness_confidence)?,
                )?,
                ProfileDimension::new(
                    required(self.dynamicity)?,
                    required(self.dynamicity_confidence)?,
                )?,
                ProfileDimension::new(
                    required(self.rhythmicity)?,
                    required(self.rhythmicity_confidence)?,
                )?,
            ),
        )
    }
}
