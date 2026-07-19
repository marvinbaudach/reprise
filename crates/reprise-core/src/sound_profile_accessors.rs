use super::{AudioEvidence, TempoEstimate};

impl AudioEvidence {
    pub fn loudness_rms(self) -> f64 {
        self.loudness_rms
    }

    pub fn dynamic_range(self) -> f64 {
        self.dynamic_range
    }

    pub fn spectral_centroid_hz(self) -> f64 {
        self.spectral_centroid_hz
    }

    pub fn spectral_rolloff_hz(self) -> f64 {
        self.spectral_rolloff_hz
    }

    pub fn spectral_flux(self) -> f64 {
        self.spectral_flux
    }

    pub fn onset_rate(self) -> f64 {
        self.onset_rate
    }

    pub fn tempo(self) -> Option<TempoEstimate> {
        self.tempo
    }
}
